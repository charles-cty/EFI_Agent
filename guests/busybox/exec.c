#include "efi_process.h"

#include <errno.h>
#include <stdlib.h>
#include <unistd.h>

unsigned efi_ash_inherit_fds(struct efi_process_fd_mapping mappings[127])
{
    unsigned count = 0;
    for (unsigned fd = 0; fd < 128; ++fd) {
        if (fd == 3)
            continue;
        int32_t flags = efi_process_get_fd_flags(fd);
        if (flags == 0)
            mappings[count++] = (struct efi_process_fd_mapping){fd, fd};
        else if (flags != 1 && flags != -9)
            __builtin_trap();
    }
    return count;
}

/* Spawn a fresh guest instance with the caller's inheritable descriptors. */
int efi_ash_spawn(char *const arguments[], char *const environment[])
{
    uint32_t count = 0;
    uint32_t environment_count = 0;
    while (arguments[count]) {
        ++count;
    }
    while (environment && environment[environment_count]) {
        ++environment_count;
    }
    char *scratch = malloc(65536);
    if (!scratch) {
        errno = ENOMEM;
        return -1;
    }
    struct efi_process_fd_mapping mappings[127];
    unsigned mapping_count = efi_ash_inherit_fds(mappings);
    int32_t pid = efi_process_spawn_argv_env_mapped(
        (const char *const *)arguments, count, (const char *const *)environment,
        environment_count, scratch, 65536, mappings, mapping_count);
    free(scratch);
    if (pid < 0) {
        // efi_process uses POSIX errno numbers; WASI libc uses WASI numbers.
        switch (pid) {
        case -2: errno = ENOENT; break;
        case -7: errno = E2BIG; break;
        case -8: errno = ENOEXEC; break;
        case -9: errno = EBADF; break;
        case -12: errno = ENOMEM; break;
        case -13: errno = EACCES; break;
        case -20: errno = ENOTDIR; break;
        case -40: errno = ELOOP; break;
        case -11: errno = EAGAIN; break;
        case -14: errno = EFAULT; break;
        case -22: errno = EINVAL; break;
        default: __builtin_trap();
        }
        return -1;
    }
    return pid;
}

int efi_ash_run(char *const arguments[], char *const environment[])
{
    int32_t pid = efi_ash_spawn(arguments, environment);
    if (pid < 0)
        return -1;
    int32_t status;
    if (efi_process_waitpid((uint32_t)pid, &status) != pid) {
        __builtin_trap();
    }
    /* The process ABI reports a direct i32 code. Shell statuses use the low
     * eight bits; a negative guest exit must not look like a spawn failure.
     */
    return (uint32_t)status & 255;
}

int efi_ash_exec(const char *path, char *const arguments[], char *const environment[])
{
    unsigned count = 0;
    while (arguments[count])
        ++count;
    char **resolved = malloc((count + 1) * sizeof(*resolved));
    if (!resolved) {
        errno = ENOMEM;
        return -1;
    }
    for (unsigned index = 0; index <= count; ++index)
        resolved[index] = arguments[index];
    resolved[0] = (char *)path;
    int status = efi_ash_run(resolved, environment);
    free(resolved);
    if (status < 0) {
        return -1;
    }
    _exit(status);
}
