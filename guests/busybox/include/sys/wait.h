#ifndef EFI_ASH_WAIT_H
#define EFI_ASH_WAIT_H
#include <sys/types.h>
#define WNOHANG 1
#define WUNTRACED 2
#define WIFEXITED(status) (((status) & 127) == 0)
#define WEXITSTATUS(status) (((status) >> 8) & 255)
#define WIFSIGNALED(status) (((status) & 127) != 0 && ((status) & 127) != 127)
#define WTERMSIG(status) ((status) & 127)
#define WIFSTOPPED(status) (((status) & 255) == 127)
#define WSTOPSIG(status) WEXITSTATUS(status)
#define WCOREDUMP(status) ((status) & 128)
pid_t waitpid(pid_t pid, int *status, int options);
#endif
