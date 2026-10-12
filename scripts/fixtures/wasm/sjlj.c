#include "efi_sjlj.h"
#include "efi_process.h"
#include "efi_wasi.h"
#include <setjmp.h>
#include <stdio.h>
#include <stdlib.h>

static volatile int phase;
static volatile int entries;

static void require(int condition)
{
    if (!condition) {
        __builtin_trap();
    }
}

__attribute__((noinline)) static void deep_jump(unsigned depth, jmp_buf destination, int value)
{
    volatile unsigned sentinel[31];
    sentinel[0] = depth * 7 + 3;
    if (depth != 0) {
        deep_jump(depth - 1, destination, value);
    } else {
        require(efi_wasi_yield() == 0);
        longjmp(destination, value);
    }
    require(sentinel[0] == depth * 7 + 3);
    __builtin_trap();
}

__attribute__((noinline)) static int application(int argc, char **argv)
{
    ++entries;
    jmp_buf outer;
    jmp_buf inner;
    volatile int live = 91;
    int value = setjmp(outer);
    if (value == 0) {
        require(entries == 1 && phase == 0);
        live = 123;
        phase = 1;
        int nested = setjmp(inner);
        if (nested == 0) {
            phase = 2;
            deep_jump(13, inner, 0);
        }
        require(nested == 1 && phase == 2 && live == 123 && entries == 1);
        phase = 3;
        deep_jump(17, outer, -37);
    }
    require(value == -37 && phase == 3 && live == 123 && entries == 1);
    /* Re-capture the same stack location with different live values. */
    for (int iteration = 0; iteration < 7; ++iteration) {
        volatile int preserved = iteration;
        int resumed = setjmp(inner);
        if (resumed == 0) {
            deep_jump(3, inner, iteration + 2);
        }
        require(resumed == preserved + 2);
    }
    volatile int repeated = 0;
    int again = setjmp(inner);
    require(again == repeated);
    if (repeated < 5) {
        ++repeated;
        deep_jump(5, inner, repeated);
    }
    uint32_t seed = argc > 1 ? (uint32_t)strtoul(argv[1], NULL, 10) : 11;
    uint32_t state = seed;
    uint32_t digest = 2166136261u;
    for (unsigned iteration = 0; iteration < 37; ++iteration) {
        state = state * 1664525u + 1013904223u;
        volatile int expected = (int)(state % 2001u) - 1000;
        volatile unsigned depth = (state >> 24) % 21;
        int result = setjmp(inner);
        if (result == 0) {
            deep_jump(depth, inner, expected);
        }
        require(result == (expected == 0 ? 1 : expected));
        digest = (digest ^ (uint32_t)result) * 16777619u;
    }
    printf("SJLJ_ORACLE %u %u\n", seed, digest);
    puts("SJLJ_NESTED_REPEATED_OK");
    return 0;
}

int main(int argc, char **argv)
{
    if (argc == 1) {
        uint32_t children[2];
        char scratch[64];
        const char *arguments[] = {"sjlj", NULL};
        for (unsigned index = 0; index < 2; ++index) {
            arguments[1] = index == 0 ? "17" : "29";
            int32_t pid = efi_process_spawn_argv(arguments, 2, scratch, sizeof scratch);
            require(pid > 0);
            children[index] = (uint32_t)pid;
        }
        require(efi_sjlj_run(application, argc, argv) == 0);
        for (unsigned index = 0; index < 2; ++index) {
            int32_t status;
            require(efi_process_waitpid(children[index], &status) == (int32_t)children[index]);
            require(status == 0);
        }
        puts("SJLJ_MULTI_INSTANCE_OK");
        return 0;
    }
    return efi_sjlj_run(application, argc, argv);
}
