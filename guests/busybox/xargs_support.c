#include "libbb.h"
#include "xargs_support.h"
#include <sched.h>

/* Each xargs invocation has its own Store and child table. Shell tasks use a
 * separate instance, so polling these children cannot reap the caller's jobs. */
static pid_t children[32];

pid_t efi_xargs_spawn(char *const arguments[])
{
    unsigned slot;
    for (slot = 0; slot < ARRAY_SIZE(children); ++slot)
        if (children[slot] == 0)
            break;
    if (slot == ARRAY_SIZE(children)) {
        errno = EAGAIN;
        return -1;
    }
    pid_t pid = efi_ash_spawn(arguments, environ);
    if (pid > 0)
        children[slot] = pid;
    return pid;
}

int efi_xargs_run(char *const arguments[])
{
    return efi_ash_run(arguments, environ);
}

pid_t efi_xargs_poll(int *status)
{
    unsigned live = 0;
    for (unsigned slot = 0; slot < ARRAY_SIZE(children); ++slot) {
        pid_t pid = children[slot];
        if (pid == 0)
            continue;
        ++live;
        int32_t code;
        int32_t result = efi_process_waitpid_nohang(pid, &code);
        if (result == 0)
            continue;
        if (result != pid)
            bb_simple_error_msg_and_die("xargs child wait failed");
        children[slot] = 0;
        *status = ((uint32_t)code & 255) << 8;
        return pid;
    }
    if (live)
        return 0;
    errno = ECHILD;
    return -1;
}

pid_t efi_xargs_wait(pid_t pid, int *status, int flags)
{
    if (pid != -1 || flags != 0)
        bb_simple_error_msg_and_die("invalid xargs wait request");
    for (;;) {
        pid_t result = efi_xargs_poll(status);
        if (result != 0)
            return result;
        if (sched_yield() < 0)
            bb_simple_perror_msg_and_die("xargs yield");
    }
}

unsigned bb_arg_max(void)
{
    size_t bytes = 0;
    for (char **entry = environ; entry && *entry; ++entry)
        bytes += strlen(*entry) + 1;
    return bytes >= 65536 ? 0 : 65536 - bytes;
}

void xdup2(int source, int destination)
{
    if (dup2(source, destination) < 0)
        bb_simple_perror_msg_and_die("dup2");
}
