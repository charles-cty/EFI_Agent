#include "quickjs.h"
#include "efi_shell.h"
#include "efi_process.h"
#include "efi_wasi.h"
#include "zx-bootstrap.h"

#include <fcntl.h>
#include <unistd.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define OUTPUT_LIMIT (512 * 1024)

struct command {
    struct efi_shell_task task;
    JSValue callbacks[2];
    char *output;
    char *error;
    char *combined;
};

struct rejection {
    JSValue promise;
    JSValue reason;
    struct rejection *next;
};

struct host {
    struct command *commands[EFI_SHELL_MAX_TASKS];
    uint32_t count;
    struct rejection *rejections;
    int rejection_allocation_failed;
};

static JSValue start_command(JSContext *context, JSValueConst this_value,
    int argc, JSValueConst *argv)
{
    (void)this_value;
    struct host *host = JS_GetContextOpaque(context);
    if (argc != 2 || host->count == EFI_SHELL_MAX_TASKS) {
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
    command->combined = malloc(OUTPUT_LIMIT * 2);
    char *scratch = malloc(65536);
    JSValue promise = JS_UNDEFINED;
    if (!command->output || !command->error || !command->combined || !scratch) {
        JS_ThrowOutOfMemory(context);
        goto failure;
    }
    promise = JS_NewPromiseCapability(context, command->callbacks);
    if (JS_IsException(promise)) {
        goto failure;
    }
    const char *arguments[] = {"ash", "-c", text};
    size_t input_size;
    const char *input = JS_ToCStringLen(context, &input_size, argv[1]);
    if (!input) {
        JS_FreeValue(context, command->callbacks[0]);
        JS_FreeValue(context, command->callbacks[1]);
        JS_FreeValue(context, promise);
        goto failure;
    }
    int32_t input_fd = input_size > OUTPUT_LIMIT ? -27 :
        efi_process_input_snapshot(input, (uint32_t)input_size);
    JS_FreeCString(context, input);
    int32_t result = input_fd < 0 ? input_fd :
        efi_shell_start_input(&command->task, arguments, 3, scratch, 65536,
            command->output, OUTPUT_LIMIT, command->error, OUTPUT_LIMIT, input_fd);
    if (input_fd >= 0)
        efi_wasi_close((uint32_t)input_fd);
    command->task.combined.bytes = command->combined;
    command->task.combined.capacity = OUTPUT_LIMIT * 2;
    free(scratch);
    JS_FreeCString(context, text);
    if (result != 0) {
        JS_FreeValue(context, command->callbacks[0]);
        JS_FreeValue(context, command->callbacks[1]);
        JS_FreeValue(context, promise);
        free(command->output);
        free(command->error);
        free(command->combined);
        free(command);
        return JS_ThrowInternalError(context, "Cannot start ash: %d", result);
    }
    host->commands[host->count++] = command;
    return promise;

failure:
    free(scratch);
    free(command->output);
    free(command->error);
    free(command->combined);
    free(command);
    JS_FreeCString(context, text);
    return JS_EXCEPTION;
}

static JSValue utf8(JSContext *context, JSValueConst this_value,
    int argc, JSValueConst *argv)
{
    (void)this_value;
    if (argc != 1)
        return JS_ThrowTypeError(context, "Expected one string");
    size_t size;
    const char *text = JS_ToCStringLen(context, &size, argv[0]);
    if (!text)
        return JS_EXCEPTION;
    JSValue value = JS_NewArrayBufferCopy(context, (const uint8_t *)text, size);
    JS_FreeCString(context, text);
    return value;
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
    fflush(stdout);
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
        JS_SetPropertyStr(context, value, "stdall", JS_NewStringLen(context,
            command->combined, command->task.combined.size));
        int failed = command->task.status != 0 || command->task.error != 0;
        JSValue returned = JS_Call(context, command->callbacks[failed], JS_UNDEFINED, 1, &value);
        JS_FreeValue(context, returned);
        JS_FreeValue(context, value);
        JS_FreeValue(context, command->callbacks[0]);
        JS_FreeValue(context, command->callbacks[1]);
        free(command->output);
        free(command->error);
        free(command->combined);
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

/* The tracker removes rejections when a handler is attached in a later job. */
static void track_rejection(JSContext *context, JSValueConst promise,
    JSValueConst reason, JS_BOOL handled, void *opaque)
{
    struct host *host = opaque;
    if (handled) {
        struct rejection **entry = &host->rejections;
        while (*entry) {
            struct rejection *current = *entry;
            if (JS_VALUE_GET_PTR(current->promise) == JS_VALUE_GET_PTR(promise)) {
                *entry = current->next;
                JS_FreeValue(context, current->promise);
                JS_FreeValue(context, current->reason);
                js_free(context, current);
                break;
            }
            entry = &current->next;
        }
    } else {
        struct rejection *entry = js_malloc(context, sizeof *entry);
        if (!entry) {
            host->rejection_allocation_failed = 1;
            return;
        }
        entry->promise = JS_DupValue(context, promise);
        entry->reason = JS_DupValue(context, reason);
        entry->next = host->rejections;
        host->rejections = entry;
    }
}

static JSValue quit(JSContext *context, JSValueConst this_value,
    int argc, JSValueConst *argv)
{
    (void)this_value;
    int32_t status = 0;
    if (argc && JS_ToInt32(context, &status, argv[0]))
        return JS_EXCEPTION;
    efi_wasi_exit(status & 255);
}

static JSValue print_error(JSContext *context, JSValueConst this_value,
    int argc, JSValueConst *argv)
{
    (void)this_value;
    for (int index = 0; index < argc; ++index) {
        size_t size;
        const char *text = JS_ToCStringLen(context, &size, argv[index]);
        if (!text)
            return JS_EXCEPTION;
        if (index)
            fputc(' ', stderr);
        fwrite(text, 1, size, stderr);
        JS_FreeCString(context, text);
    }
    fputc('\n', stderr);
    return JS_UNDEFINED;
}

/* Cancel before closing pipes so no suspended child can retain its buffers. */
static int discard_commands(JSContext *context, struct host *host)
{
    for (uint32_t index = 0; index < host->count; ++index) {
        struct command *command = host->commands[index];
        if (command->task.pid) {
            int32_t status;
            if (efi_process_cancel_child(command->task.pid) < 0 ||
                efi_process_waitpid(command->task.pid, &status) < 0)
                return -1;
        }
        for (int stream = 0; stream < 2; ++stream) {
            if (command->task.output[stream].descriptor >= 0)
                efi_wasi_close(command->task.output[stream].descriptor);
            JS_FreeValue(context, command->callbacks[stream]);
        }
        free(command->output);
        free(command->error);
        free(command->combined);
        free(command);
    }
    host->count = 0;
    return 0;
}

enum evaluation_status {
    EVALUATION_OK,
    EVALUATION_ERROR,
    EVALUATION_RESET
};

static void write_value(void *opaque, const char *bytes, size_t size)
{
    fwrite(bytes, 1, size, opaque);
}

static enum evaluation_status evaluate(JSContext *context, struct host *host,
    const char *source, size_t size, int display_result)
{
    JSRuntime *runtime = JS_GetRuntime(context);
    JSValue evaluation = JS_Eval(context, source, size, "<command>",
        JS_EVAL_TYPE_GLOBAL | JS_EVAL_FLAG_ASYNC);
    int failed = JS_IsException(evaluation);
    int reset = 0;
    if (failed) {
        JSValue exception = JS_GetException(context);
        report_exception(context, exception);
        JS_FreeValue(context, exception);
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
        if (JS_PromiseState(context, evaluation) == JS_PROMISE_REJECTED) {
            JSValue reason = JS_PromiseResult(context, evaluation);
            report_exception(context, reason);
            JS_FreeValue(context, reason);
            failed = 1;
            break;
        }
        if (host->rejections || host->rejection_allocation_failed) {
            failed = 1;
            break;
        }
        if (host->count == 0) {
            if (JS_PromiseState(context, evaluation) == JS_PROMISE_PENDING) {
                fputs("QuickJS: pending Promise has no event source\n", stderr);
                failed = 1;
                /* A saved resolver could otherwise resume this request later. */
                reset = 1;
            }
            break;
        }
        failed = advance(context, host) != 0;
    }
    if (host->rejections || host->rejection_allocation_failed)
        failed = 1;
    while (host->rejections) {
        struct rejection *entry = host->rejections;
        /* The outer evaluation rejection is already reported above. */
        if (JS_VALUE_GET_PTR(entry->promise) != JS_VALUE_GET_PTR(evaluation))
            report_exception(context, entry->reason);
        host->rejections = entry->next;
        JS_FreeValue(context, entry->promise);
        JS_FreeValue(context, entry->reason);
        js_free(context, entry);
    }
    host->rejection_allocation_failed = 0;
    if (!failed && display_result) {
        /* Async global evaluation fulfills with { value: completionValue }. */
        JSValue completed = JS_PromiseResult(context, evaluation);
        JSValue value = JS_GetPropertyStr(context, completed, "value");
        if (JS_IsException(value)) {
            JSValue exception = JS_GetException(context);
            report_exception(context, exception);
            JS_FreeValue(context, exception);
            failed = 1;
        } else {
            JS_PrintValue(context, write_value, stdout, value, NULL);
            putchar('\n');
        }
        JS_FreeValue(context, value);
        JS_FreeValue(context, completed);
    }
    if (discard_commands(context, host) != 0)
        efi_wasi_exit(1);
    if (failed && !JS_IsException(evaluation) &&
        JS_PromiseState(context, evaluation) == JS_PROMISE_PENDING)
        reset = 1;
    JS_FreeValue(context, evaluation);
    JS_RunGC(runtime);
    return reset ? EVALUATION_RESET : failed ? EVALUATION_ERROR : EVALUATION_OK;
}

static int read_frame(int fd, void *bytes, size_t size)
{
    char *cursor = bytes;
    while (size) {
        ssize_t count = read(fd, cursor, size);
        if (count == 0 && cursor == (char *)bytes)
            return 1;
        if (count <= 0)
            return -1;
        cursor += count;
        size -= count;
    }
    return 0;
}

static int run_session(JSContext *context, struct host *host)
{
    int fd = open("/.efi-session", O_RDWR | O_CLOEXEC);
    if (fd < 0) {
        fputs("QuickJS: cannot open code session channel\n", stderr);
        return 1;
    }
    for (;;) {
        uint32_t header[3];
        int input = read_frame(fd, header, sizeof header);
        if (input == 1) {
            close(fd);
            return 0;
        }
        if (input != 0 || header[1] > 60 * 1024 || header[2] > 1)
            break;
        char *source = malloc(header[1] + 1);
        if (!source || read_frame(fd, source, header[1])) {
            free(source);
            break;
        }
        source[header[1]] = '\0';
        enum evaluation_status status = evaluate(context, host, source, header[1], header[2]);
        free(source);
        if (status == EVALUATION_RESET) {
            close(fd);
            return 1;
        }
        uint32_t response[2] = {header[0], (uint32_t)status};
        if (fflush(NULL))
            response[1] = 1;
        if (write(fd, response, sizeof response) != sizeof response)
            break;
    }
    close(fd);
    fputs("QuickJS: code session channel failed\n", stderr);
    return 1;
}

int main(int argc, char **argv)
{
    int session = argc == 2 && strcmp(argv[1], "--efi-session") == 0;
    if (!session && (argc != 3 || strcmp(argv[1], "-e") != 0)) {
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
    JS_SetHostPromiseRejectionTracker(runtime, track_rejection, &host);
    JSValue global = JS_GetGlobalObject(context);
    JS_SetPropertyStr(context, global, "__shell", JS_NewCFunction(context, start_command, "__shell", 1));
    JS_SetPropertyStr(context, global, "__utf8", JS_NewCFunction(context, utf8, "__utf8", 1));
    JS_SetPropertyStr(context, global, "print", JS_NewCFunction(context, print, "print", 1));
    JS_SetPropertyStr(context, global, "quit", JS_NewCFunction(context, quit, "quit", 1));
    JSValue console = JS_NewObject(context);
    JS_SetPropertyStr(context, console, "log", JS_NewCFunction(context, print, "log", 1));
    JS_SetPropertyStr(context, console, "error", JS_NewCFunction(context, print_error, "error", 1));
    JS_SetPropertyStr(context, global, "console", console);
    JS_FreeValue(context, global);
    JSValue initialized = JS_Eval(context, zx_bootstrap, sizeof zx_bootstrap - 1,
        "<zx>", JS_EVAL_TYPE_GLOBAL);
    int failed = JS_IsException(initialized);
    JS_FreeValue(context, initialized);
    if (failed) {
        JSValue exception = JS_GetException(context);
        report_exception(context, exception);
        JS_FreeValue(context, exception);
    } else {
        failed = session ? run_session(context, &host) :
            evaluate(context, &host, argv[2], strlen(argv[2]), 0);
    }
    JS_FreeContext(context);
    JS_FreeRuntime(runtime);
    return failed != 0;
}
