#ifndef EFI_SHELL_H
#define EFI_SHELL_H

#include <stdint.h>

#define EFI_SHELL_MAX_TASKS 31

struct efi_shell_output {
    char *bytes;
    uint32_t capacity;
    uint32_t size;
    int32_t descriptor;
};

struct efi_shell_task {
    uint32_t pid;
    int32_t status;
    int32_t error;
    struct efi_shell_output output[2];
};

/* Start a fresh instance with empty stdin and distinct stdout/stderr pipes.
 * Output storage belongs to the caller and must remain live through completion.
 * argv[0] names a registered program; a shell caller passes ash, -c, command.
 */
int32_t efi_shell_start(struct efi_shell_task *task, const char *const argv[],
    uint32_t argc, char *scratch, uint32_t scratch_size,
    char *stdout_bytes, uint32_t stdout_capacity,
    char *stderr_bytes, uint32_t stderr_capacity);

/* Drain all tasks together. Blocking mode waits for pipe readiness, unless a
 * task has closed its pipes but has not exited. Then it yields and checks exit
 * without blocking other tasks. An output overflow sets task.error to -27;
 * excess output is still drained so the child can exit and be reaped.
 * Returns 0 or a negative errno. Individual task errors stay in task.error.
 */
int32_t efi_shell_step(struct efi_shell_task *const tasks[], uint32_t count, int wait);
int efi_shell_done(const struct efi_shell_task *task);

#endif
