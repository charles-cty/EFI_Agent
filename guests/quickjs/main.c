#include "quickjs.h"
#include "efi_shell.h"
#include "efi_wasi.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define OUTPUT_LIMIT (512 * 1024)

struct command {
    struct efi_shell_task task;
    JSValue callbacks[2];
    char *output;
    char *error;
};

struct host {
    struct command *commands[EFI_SHELL_MAX_TASKS];
    uint32_t count;
};

static JSValue start_command(JSContext *context, JSValueConst this_value,
    int argc, JSValueConst *argv)
{
    (void)this_value;
    struct host *host = JS_GetContextOpaque(context);
    if (argc != 1 || host->count == EFI_SHELL_MAX_TASKS) {
        return JS_ThrowTypeError(context, "Expected a command and an available process slot");
    }
    size_t length;
    const char *text = JS_ToCStringLen(context, &length, argv[0]);
    if (!text) {
        return JS_EXCEPTION;
    }
    if (memchr(text, 0, length) || length > 65520) {
        JS_FreeCString(context, text);
        return JS_ThrowTypeError(context, "Command contains NUL or exceeds the argument limit");
    }
    struct command *command = calloc(1, sizeof *command);
    if (!command) {
        JS_FreeCString(context, text);
        return JS_ThrowOutOfMemory(context);
    }
    command->output = malloc(OUTPUT_LIMIT);
    command->error = malloc(OUTPUT_LIMIT);
    char *scratch = malloc(65536);
    JSValue promise = JS_UNDEFINED;
    if (!command->output || !command->error || !scratch) {
        JS_ThrowOutOfMemory(context);
        goto failure;
    }
    promise = JS_NewPromiseCapability(context, command->callbacks);
    if (JS_IsException(promise)) {
        goto failure;
    }
    const char *arguments[] = {"ash", "-c", text};
    int32_t result = efi_shell_start(&command->task, arguments, 3, scratch, 65536,
        command->output, OUTPUT_LIMIT, command->error, OUTPUT_LIMIT);
    free(scratch);
    JS_FreeCString(context, text);
    if (result != 0) {
        JS_FreeValue(context, command->callbacks[0]);
        JS_FreeValue(context, command->callbacks[1]);
        JS_FreeValue(context, promise);
        free(command->output);
        free(command->error);
        free(command);
        return JS_ThrowInternalError(context, "Cannot start ash: %d", result);
    }
    host->commands[host->count++] = command;
    return promise;

failure:
    free(scratch);
    free(command->output);
    free(command->error);
    free(command);
    JS_FreeCString(context, text);
    return JS_EXCEPTION;
}

static JSValue print(JSContext *context, JSValueConst this_value,
    int argc, JSValueConst *argv)
{
    (void)this_value;
    for (int index = 0; index < argc; ++index) {
        size_t size;
        const char *text = JS_ToCStringLen(context, &size, argv[index]);
        if (!text) {
            return JS_EXCEPTION;
        }
        if (index != 0) {
            putchar(' ');
        }
        fwrite(text, 1, size, stdout);
        JS_FreeCString(context, text);
    }
    putchar('\n');
    return JS_UNDEFINED;
}

static int advance(JSContext *context, struct host *host)
{
    struct efi_shell_task *tasks[EFI_SHELL_MAX_TASKS];
    for (uint32_t index = 0; index < host->count; ++index) {
        tasks[index] = &host->commands[index]->task;
    }
    int32_t result = efi_shell_step(tasks, host->count, 1);
    if (result != 0) {
        fprintf(stderr, "Shell dispatch error: %d\n", result);
        return -1;
    }
    for (uint32_t index = 0; index < host->count;) {
        struct command *command = host->commands[index];
        if (!efi_shell_done(&command->task)) {
            ++index;
            continue;
        }
        JSValue value = JS_NewObject(context);
        JS_SetPropertyStr(context, value, "exitCode", JS_NewInt32(context, command->task.status));
        JS_SetPropertyStr(context, value, "stdout", JS_NewStringLen(context,
            command->output, command->task.output[0].size));
        JS_SetPropertyStr(context, value, "stderr", JS_NewStringLen(context,
            command->error, command->task.output[1].size));
        JS_SetPropertyStr(context, value, "captureError", JS_NewInt32(context, command->task.error));
        int failed = command->task.status != 0 || command->task.error != 0;
        JSValue returned = JS_Call(context, command->callbacks[failed], JS_UNDEFINED, 1, &value);
        JS_FreeValue(context, returned);
        JS_FreeValue(context, value);
        JS_FreeValue(context, command->callbacks[0]);
        JS_FreeValue(context, command->callbacks[1]);
        free(command->output);
        free(command->error);
        free(command);
        host->commands[index] = host->commands[--host->count];
    }
    return 0;
}

static void report_exception(JSContext *context, JSValueConst value)
{
    const char *text = JS_ToCString(context, value);
    fprintf(stderr, "QuickJS: %s\n", text ? text : "exception");
    JS_FreeCString(context, text);
}

int main(int argc, char **argv)
{
    if (argc != 3 || strcmp(argv[1], "-e") != 0) {
        fputs("Usage: quickjs -e SCRIPT\n", stderr);
        return 2;
    }
    JSRuntime *runtime = JS_NewRuntime();
    if (!runtime) {
        return 1;
    }
    JS_SetMemoryLimit(runtime, 32 * 1024 * 1024);
    JS_SetMaxStackSize(runtime, 512 * 1024);
    JSContext *context = JS_NewContext(runtime);
    if (!context) {
        JS_FreeRuntime(runtime);
        return 1;
    }
    struct host host = {0};
    JS_SetContextOpaque(context, &host);
    JSValue global = JS_GetGlobalObject(context);
    JS_SetPropertyStr(context, global, "__shell", JS_NewCFunction(context, start_command, "__shell", 1));
    JS_SetPropertyStr(context, global, "print", JS_NewCFunction(context, print, "print", 1));
    JS_FreeValue(context, global);
    /* Template substitutions are always quoted as one shell word. Arrays
     * expand to separate quoted words. Literal segments remain shell syntax. */
    const char *bootstrap =
        "globalThis.$ = (strings, ...values) => {"
        " const quote = value => { const s = String(value);"
        "  if (s.includes('\\0')) throw new TypeError('NUL in shell argument');"
        "  return \"'\" + s.replaceAll(\"'\", \"'\\\"'\\\"'\") + \"'\"; };"
        " let command = strings[0];"
        " for (let i = 0; i < values.length; i++) {"
        "  command += (Array.isArray(values[i]) ? values[i].map(quote).join(' ') : quote(values[i]));"
        "  command += strings[i + 1]; }"
        " return __shell(command); };";
    JSValue initialized = JS_Eval(context, bootstrap, strlen(bootstrap), "<shell>", JS_EVAL_TYPE_GLOBAL);
    int failed = JS_IsException(initialized);
    JS_FreeValue(context, initialized);
    JSValue evaluation = JS_UNDEFINED;
    if (!failed) {
        evaluation = JS_Eval(context, argv[2], strlen(argv[2]), "<command>",
            JS_EVAL_TYPE_GLOBAL | JS_EVAL_FLAG_ASYNC);
        failed = JS_IsException(evaluation);
    }
    while (!failed) {
        JSContext *job_context;
        int job;
        while ((job = JS_ExecutePendingJob(runtime, &job_context)) > 0) {}
        if (job < 0) {
            JSValue exception = JS_GetException(job_context);
            report_exception(job_context, exception);
            JS_FreeValue(job_context, exception);
            failed = 1;
            break;
        }
        if (host.count == 0) {
            break;
        }
        failed = advance(context, &host) != 0;
    }
    if (failed) {
        JSValue exception = JS_GetException(context);
        report_exception(context, exception);
        JS_FreeValue(context, exception);
    } else if (JS_PromiseState(context, evaluation) != JS_PROMISE_FULFILLED) {
        JSValue result = JS_PromiseResult(context, evaluation);
        report_exception(context, result);
        JS_FreeValue(context, result);
        failed = 1;
    }
    /* Drain already-started commands even when script evaluation fails. */
    while (host.count != 0) {
        if (advance(context, &host) != 0) {
            return 1;
        }
    }
    JS_FreeValue(context, evaluation);
    JS_FreeContext(context);
    JS_FreeRuntime(runtime);
    return failed;
}
