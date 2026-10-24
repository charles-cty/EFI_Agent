#ifndef EFI_XARGS_SUPPORT_H
#define EFI_XARGS_SUPPORT_H

#define CURRENT_TTY "/dev/tty"
#define xfopen_for_read(path) xfopen((path), "r")
#define bb_putchar_stderr(value) fputc((value), stderr)
#define spawn efi_xargs_spawn
#define spawn_and_wait efi_xargs_run
#define safe_waitpid efi_xargs_wait
#define wait_any_nohang efi_xargs_poll

pid_t efi_xargs_spawn(char *const arguments[]);
int efi_xargs_run(char *const arguments[]);
pid_t efi_xargs_wait(pid_t pid, int *status, int flags);
pid_t efi_xargs_poll(int *status);
unsigned bb_arg_max(void);
unsigned count_strstr(const char *source, const char *substring);
char *xmalloc_substitute_string(const char *source, int count,
    const char *substring, const char *replacement);
void xdup2(int source, int destination);

#endif
