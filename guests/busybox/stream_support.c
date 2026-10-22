#include "libbb.h"

const char bb_msg_standard_output[] = "standard output";
const char bb_msg_read_error[] = "read error";

int fputs_stdout(const char *text)
{
    return fputs(text, stdout);
}

void bb_perror_msg(const char *format, ...)
{
    int error = errno;
    va_list arguments;
    va_start(arguments, format);
    vfprintf(stderr, format, arguments);
    va_end(arguments);
    fprintf(stderr, ": %s\n", strerror(error));
}

int open_or_warn(const char *path, int flags)
{
    int fd = open(path, flags);
    if (fd < 0)
        bb_simple_perror_msg(path);
    return fd;
}

off_t xlseek(int fd, off_t offset, int whence)
{
    off_t result = lseek(fd, offset, whence);
    if (result < 0)
        bb_simple_perror_msg_and_die("seek");
    return result;
}

void xmove_fd(int source, int target)
{
    if (source != target) {
        if (dup2(source, target) < 0)
            bb_simple_perror_msg_and_die("dup2");
        if (close(source) < 0)
            bb_simple_perror_msg_and_die("close");
    }
}

void bb_perror_nomsg_and_die(void)
{
    bb_simple_perror_msg_and_die(applet_name);
}

void die_if_ferror(FILE *stream, const char *name)
{
    if (ferror(stream))
        bb_error_msg_and_die("%s: I/O error", name);
}

void die_if_ferror_stdout(void)
{
    die_if_ferror(stdout, bb_msg_standard_output);
}

void BB_EXECVP_or_die(char **arguments)
{
    int status = efi_ash_run(arguments, environ);
    if (status >= 0)
        exit(status);
    xfunc_error_retval = errno == ENOENT ? 127 : 126;
    bb_error_msg_and_die("can't execute '%s': %s", arguments[0], strerror(errno));
}
