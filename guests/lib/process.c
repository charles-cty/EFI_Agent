#include "efi_process.h"

int32_t efi_process_pack_argv(
    const char *const arguments[], uint32_t count, char *scratch, uint32_t capacity)
{
    if (count == 0 || arguments[0][0] == '\0') {
        return -22;
    }
    if (capacity > 65536) {
        capacity = 65536;
    }

    uint32_t used = 0;
    for (uint32_t index = 0; index < count; ++index) {
        const char *argument = arguments[index];
        for (;;) {
            if (used == capacity) {
                return -7; /* E2BIG */
            }
            char byte = *argument++;
            scratch[used++] = byte;
            if (byte == '\0') {
                break;
            }
        }
    }
    return (int32_t)used;
}

int32_t efi_process_spawn_argv(
    const char *const arguments[], uint32_t count, char *scratch, uint32_t capacity)
{
    int32_t size = efi_process_pack_argv(arguments, count, scratch, capacity);
    if (size < 0) {
        return size;
    }
    return efi_process_spawn_raw(scratch, (uint32_t)size);
}

int32_t efi_process_spawn_argv_fds(
    const char *const arguments[], uint32_t count, char *scratch, uint32_t capacity,
    uint32_t input, uint32_t output, uint32_t error)
{
    int32_t size = efi_process_pack_argv(arguments, count, scratch, capacity);
    if (size < 0) {
        return size;
    }
    return efi_process_spawn_fds_raw(scratch, (uint32_t)size, input, output, error);
}

static int32_t pack_argv_environment(
    const char *const arguments[], uint32_t count,
    const char *const environment[], uint32_t environment_count,
    char *scratch, uint32_t capacity, uint32_t *argument_size)
{
    if (capacity > 65536) {
        capacity = 65536;
    }
    int32_t size = efi_process_pack_argv(arguments, count, scratch, capacity);
    if (size < 0) {
        return size;
    }
    *argument_size = (uint32_t)size;
    uint32_t used = (uint32_t)size;
    for (uint32_t index = 0; index < environment_count; ++index) {
        const char *entry = environment[index];
        for (;;) {
            if (used == capacity) {
                return -7;
            }
            char byte = *entry++;
            scratch[used++] = byte;
            if (byte == '\0') {
                break;
            }
        }
    }
    return (int32_t)used;
}

int32_t efi_process_spawn_argv_env_fds(
    const char *const arguments[], uint32_t count,
    const char *const environment[], uint32_t environment_count,
    char *scratch, uint32_t capacity, uint32_t input, uint32_t output, uint32_t error)
{
    uint32_t size;
    int32_t used = pack_argv_environment(arguments, count, environment,
        environment_count, scratch, capacity, &size);
    if (used < 0)
        return used;
    return efi_process_spawn_env_fds_raw(scratch, size,
        scratch + size, (uint32_t)used - size, input, output, error);
}

int32_t efi_process_spawn_argv_env_mapped(
    const char *const arguments[], uint32_t count,
    const char *const environment[], uint32_t environment_count,
    char *scratch, uint32_t capacity,
    const struct efi_process_fd_mapping *mappings, uint32_t mapping_count)
{
    uint32_t size;
    int32_t used = pack_argv_environment(arguments, count, environment,
        environment_count, scratch, capacity, &size);
    if (used < 0)
        return used;
    return efi_process_spawn_mapped_fds_raw(scratch, size,
        scratch + size, (uint32_t)used - size, mappings, mapping_count);
}
