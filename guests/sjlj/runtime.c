#include "efi_sjlj.h"
#include "setjmp.h"

#include <stdlib.h>
#include <string.h>

#define MAX_ENVIRONMENTS 64
#define FRAME_CAPACITY (1024 * 1024)
#define ASYNCIFY_IMPORT(name) \
    __attribute__((import_module("asyncify"), import_name(name)))
#define RUNTIME __attribute__((noinline))

struct asyncify_data {
    unsigned char *position;
    unsigned char *end;
};

ASYNCIFY_IMPORT("start_unwind") void start_unwind(struct asyncify_data *data);
ASYNCIFY_IMPORT("stop_unwind") void stop_unwind(void);
ASYNCIFY_IMPORT("start_rewind") void start_rewind(struct asyncify_data *data);
ASYNCIFY_IMPORT("stop_rewind") void stop_rewind(void);

uint32_t efi_sjlj_get_sp(void);
void efi_sjlj_set_sp(uint32_t value);

struct saved_environment {
    uintptr_t *address;
    unsigned char *frames;
    size_t size;
    uint32_t stack_pointer;
};

static struct saved_environment environments[MAX_ENVIRONMENTS];
static unsigned char frames[FRAME_CAPACITY] __attribute__((aligned(16)));
static struct asyncify_data data;
static struct saved_environment *target;
static uint32_t saved_stack_pointer;
static int jump_value;
static int capturing;
static int driver_active;
static int state;

static struct saved_environment *find_environment(uintptr_t *address, int create)
{
    for (unsigned index = 0; index < MAX_ENVIRONMENTS; ++index) {
        if (environments[index].address == address) {
            return &environments[index];
        }
    }
    if (create) {
        for (unsigned index = 0; index < MAX_ENVIRONMENTS; ++index) {
            if (environments[index].address == NULL) {
                environments[index].address = address;
                return &environments[index];
            }
        }
    }
    /* Invalid longjmp targets and exhausted frame slots are fatal. */
    __builtin_trap();
}

static void expire_deeper_frames(uint32_t stack_pointer)
{
    for (unsigned index = 0; index < MAX_ENVIRONMENTS; ++index) {
        struct saved_environment *environment = &environments[index];
        // A shallower capture or longjmp means these C frames have returned
        // or have been abandoned. Their jump targets are no longer valid.
        if (environment->address && environment->stack_pointer < stack_pointer) {
            free(environment->frames);
            *environment = (struct saved_environment){0};
        }
    }
}

RUNTIME int efi_sjlj_setjmp(jmp_buf environment)
{
    if (state == 2) {
        if (target->address != environment) {
            __builtin_trap();
        }
        stop_rewind();
        state = 0;
        return jump_value;
    }
    if (!driver_active || state != 0) {
        __builtin_trap();
    }
    saved_stack_pointer = efi_sjlj_get_sp();
    expire_deeper_frames(saved_stack_pointer);
    target = find_environment(environment, 1);
    capturing = 1;
    jump_value = 0;
    data = (struct asyncify_data){frames, frames + sizeof frames};
    state = 1;
    start_unwind(&data);
    return 0;
}

RUNTIME void efi_sjlj_longjmp(jmp_buf environment, int value)
{
    if (!driver_active || state != 0) {
        __builtin_trap();
    }
    target = find_environment(environment, 0);
    if (!target->frames) {
        __builtin_trap();
    }
    capturing = 0;
    jump_value = value == 0 ? 1 : value;
    data = (struct asyncify_data){frames, frames + sizeof frames};
    state = 1;
    start_unwind(&data);
}

RUNTIME int efi_sjlj_run(int (*entry)(int, char **), int argc, char **argv)
{
    if (driver_active) {
        __builtin_trap();
    }
    driver_active = 1;
    for (;;) {
        int result = entry(argc, argv);
        if (state == 0) {
            driver_active = 0;
            for (unsigned index = 0; index < MAX_ENVIRONMENTS; ++index) {
                free(environments[index].frames);
                environments[index] = (struct saved_environment){0};
            }
            return result;
        }
        stop_unwind();
        state = 0;
        if (capturing) {
            size_t size = (size_t)(data.position - frames);
            if (size > sizeof frames) {
                __builtin_trap();
            }
            unsigned char *saved = realloc(target->frames, size);
            if (!saved) {
                __builtin_trap();
            }
            target->frames = saved;
            target->size = size;
            target->stack_pointer = saved_stack_pointer;
            memcpy(target->frames, frames, size);
        } else {
            memcpy(frames, target->frames, target->size);
            expire_deeper_frames(target->stack_pointer);
        }
        data.position = frames + target->size;
        data.end = frames + sizeof frames;
        state = 2;
        start_rewind(&data);
        efi_sjlj_set_sp(target->stack_pointer);
    }
}
