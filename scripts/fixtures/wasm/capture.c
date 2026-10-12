#include "efi_process.h"

#define WASI_IMPORT(name) \
    __attribute__((import_module("wasi_snapshot_preview1"), import_name(name)))

struct IoVector {
    const void *buffer;
    uint32_t size;
};

WASI_IMPORT("fd_read") int32_t wasi_read(uint32_t fd, const struct IoVector *vectors,
    uint32_t count, uint32_t *size);
WASI_IMPORT("fd_write") int32_t wasi_write(uint32_t fd, const struct IoVector *vectors,
    uint32_t count, uint32_t *size);
WASI_IMPORT("fd_close") int32_t wasi_close(uint32_t fd);
WASI_IMPORT("proc_exit") __attribute__((noreturn)) void wasi_exit(int32_t status);

static void require(int success)
{
    if (!success) {
        wasi_exit(99);
    }
}

void _start(void)
{
    uint32_t input[2];
    uint32_t output[2];
    require(efi_process_pipe(input) == 0);
    require(efi_process_pipe(output) == 0);

    char scratch[64];
    const char *arguments[] = {"copy", "", "argument"};
    /* Check the capacity boundary before starting the actual child. */
    require(efi_process_pack_argv(arguments, 3, scratch, 14) == -7);
    require(efi_process_pack_argv(arguments, 3, scratch, 15) == 15);
    int32_t child = efi_process_spawn_argv_fds(arguments, 3, scratch, sizeof scratch,
        input[0], output[1], 2);
    require(child > 0);
    require(wasi_close(input[0]) == 0);
    require(wasi_close(output[1]) == 0);

    static const char payload[] = "C_CAPTURE_BINARY_\0END";
    struct IoVector vector = {payload, sizeof payload - 1};
    uint32_t size;
    require(wasi_write(input[1], &vector, 1, &size) == 0);
    require(size == vector.size);
    require(wasi_close(input[1]) == 0);

    char received[sizeof payload];
    uint32_t used = 0;
    for (;;) {
        /* Short reads force the C caller to assemble the result. */
        char chunk[3];
        vector.buffer = chunk;
        vector.size = sizeof chunk;
        require(wasi_read(output[0], &vector, 1, &size) == 0);
        if (size == 0) {
            break;
        }
        require(size <= sizeof received - used);
        for (uint32_t index = 0; index < size; ++index) {
            received[used++] = chunk[index];
        }
    }
    require(used == sizeof payload - 1);
    for (uint32_t index = 0; index < used; ++index) {
        require(received[index] == payload[index]);
    }
    require(wasi_close(output[0]) == 0);
    int32_t status;
    require(efi_process_waitpid((uint32_t)child, &status) == child);
    require(status == 0);

    static const char success[] = "C_SPAWN_CAPTURE_OK";
    vector.buffer = success;
    vector.size = sizeof success - 1;
    require(wasi_write(1, &vector, 1, &size) == 0);
    require(size == vector.size);
}
