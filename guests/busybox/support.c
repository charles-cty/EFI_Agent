#include "busybox.h"
#include "shell_common.h"
#include <sys/times.h>

const char bb_PATH_root_path[] = "PATH=/bin";
const char bb_msg_memory_exhausted[] = "memory exhausted";
const char bb_msg_requires_arg[] = "%s requires an argument";
const char bb_msg_write_error[] = "write error";
const char bb_dev_null[] = "/dev/null";
const char bb_banner[] = "BusyBox 1.37.0 (UEFI WASM port)";
const char defifsvar[] = "IFS= \t\n";
const char defoptindvar[] = "OPTIND=1";
const char *applet_name = "ash";
const char *bb_busybox_exec_path = "ash";
int bb_got_signal;

const char *bb_basename(const char *path)
{
    const char *separator = strrchr(path, '/');
    return separator ? separator + 1 : path;
}

int index_in_strings(const char *strings, const char *value)
{
    int index = 0;
    while (*strings) {
        if (strcmp(strings, value) == 0)
            return index;
        strings += strlen(strings) + 1;
        ++index;
    }
    return -1;
}

void bb_error_msg(const char *format, ...)
{
    va_list arguments;
    va_start(arguments, format);
    vfprintf(stderr, format, arguments);
    va_end(arguments);
    fputc('\n', stderr);
}

void bb_simple_error_msg(const char *text)
{
    bb_error_msg("%s", text);
}

void bb_simple_perror_msg(const char *text)
{
    bb_error_msg("%s: %s", text, strerror(errno));
}

void xfunc_die(void)
{
    exit(2);
}

void bb_simple_error_msg_and_die(const char *text)
{
    bb_simple_error_msg(text);
    xfunc_die();
}
void bb_error_msg_and_die(const char *format, ...)
{
    va_list arguments;
    va_start(arguments, format);
    vfprintf(stderr, format, arguments);
    va_end(arguments);
    fputc('\n', stderr);
    xfunc_die();
}

void *xmalloc(size_t size)
{
    void *result = malloc(size ? size : 1);
    if (!result) {
        bb_simple_error_msg_and_die(bb_msg_memory_exhausted);
    }
    return result;
}
void *xzalloc(size_t size)
{
    void *result = xmalloc(size);
    memset(result, 0, size);
    return result;
}
void *xrealloc(void *pointer, size_t size)
{
    void *result = realloc(pointer, size ? size : 1);
    if (!result) {
        bb_simple_error_msg_and_die(bb_msg_memory_exhausted);
    }
    return result;
}
void *xmemdup(const void *pointer, size_t size)
{
    void *result = xmalloc(size);
    memcpy(result, pointer, size);
    return result;
}

char *xstrdup(const char *text)
{
    return xmemdup(text, strlen(text) + 1);
}

void fflush_all(void)
{
    fflush(NULL);
}

ssize_t safe_read(int fd, void *bytes, size_t size)
{
    return read(fd, bytes, size);
}

ssize_t nonblock_immune_read(int fd, void *bytes, size_t size)
{
    return read(fd, bytes, size);
}

ssize_t safe_write(int fd, const void *bytes, size_t size)
{
    return write(fd, bytes, size);
}

int pipe(int descriptors[2])
{
    int32_t result = efi_process_pipe((uint32_t *)descriptors);
    if (result == 0) {
        /* Tasks select needed internal pipe ends explicitly as stdio. */
        if (efi_process_set_fd_flags(descriptors[0], 1) != 0
            || efi_process_set_fd_flags(descriptors[1], 1) != 0)
            __builtin_trap();
        return 0;
    }
    switch (result) {
    case -14: errno = EFAULT; break;
    case -24: errno = EMFILE; break;
    default: __builtin_trap();
    }
    return -1;
}

int dup2(int source, int target)
{
    int32_t result = efi_process_dup2(source, target);
    if (result >= 0)
        return result;
    if (result != -9)
        __builtin_trap();
    errno = EBADF;
    return -1;
}
int efi_ash_dupfd(int source, int minimum)
{
    int32_t result = efi_process_dupfd(source, minimum);
    if (result < 0) {
        switch (result) {
        case -9: errno = EBADF; break;
        case -22: errno = EINVAL; break;
        case -24: errno = EMFILE; break;
        default: __builtin_trap();
        }
        return -1;
    }
    return result;
}

int efi_ash_input_snapshot(const void *bytes, size_t size)
{
    int32_t descriptor = efi_process_input_snapshot(bytes, (uint32_t)size);
    if (descriptor >= 0)
        return descriptor;
    switch (descriptor) {
    case -7: errno = E2BIG; break;
    case -14: errno = EFAULT; break;
    case -24: errno = EMFILE; break;
    default: __builtin_trap();
    }
    return -1;
}
int efi_ash_dupfd_cloexec(int source, int minimum)
{
    int descriptor = efi_ash_dupfd(source, minimum);
    if (descriptor >= 0 && efi_process_set_fd_flags(descriptor, 1) != 0)
        __builtin_trap();
    return descriptor;
}

ssize_t full_write(int fd, const void *bytes, size_t size)
{
    size_t done = 0;
    while (done < size) {
        ssize_t result = write(fd, (const char *)bytes + done, size - done);
        if (result < 0) {
            return done ? (ssize_t)done : -1;
        }
        if (result == 0) {
            break;
        }
        done += (size_t)result;
    }
    return (ssize_t)done;
}
void xwrite(int fd, const void *bytes, size_t size)
{
    if (full_write(fd, bytes, size) != (ssize_t)size) {
        bb_simple_error_msg_and_die("write failed");
    }
}
void close_on_exec_on(int fd)
{
    if (efi_process_set_fd_flags(fd, 1) != 0) {
        bb_simple_error_msg_and_die("cannot set descriptor flags");
    }
}
char *is_prefixed_with(const char *text, const char *prefix)
{
    size_t size = strlen(prefix);
    return strncmp(text, prefix, size) == 0 ? (char *)text + size : NULL;
}
const char *endofname(const char *text)
{
    if (!isalpha((unsigned char)*text) && *text != '_') {
        return text;
    }
    while (isalnum((unsigned char)*text) || *text == '_') {
        ++text;
    }
    return text;
}
char *safe_strncpy(char *target, const char *source, size_t size)
{
    if (size) {
        strncpy(target, source, size - 1);
        target[size - 1] = 0;
    }
    return target;
}
int varcmp(const char *left, const char *right)
{
    while (*left == *right && *left != '=' && *left != 0) {
        ++left;
        ++right;
    }
    return (*left == '=' ? 0 : (unsigned char)*left) - (*right == '=' ? 0 : (unsigned char)*right);
}
char *itoa(int value)
{
    static char result[32];
    snprintf(result, sizeof result, "%d", value);
    return result;
}

char *utoa(unsigned value)
{
    static char result[32];
    snprintf(result, sizeof result, "%u", value);
    return result;
}
int bb_putchar(int value)
{
    return putchar(value);
}

char *skip_whitespace(const char *text)
{
    while (isspace((unsigned char)*text)) {
        ++text;
    }
    return (char *)text;
}

char **skip_dash_dash(char **arguments)
{
    ++arguments;
    if (*arguments && strcmp(*arguments, "--") == 0) {
        ++arguments;
    }
    return arguments;
}

void overlapping_strcpy(char *target, const char *source)
{
    memmove(target, source, strlen(source) + 1);
}

void bb_show_usage(void)
{
    bb_simple_error_msg_and_die("invalid applet arguments");
}

/* Guest identity is local, with no host user database or privileged UID. */
uid_t getuid(void)
{
    return 1;
}

uid_t geteuid(void)
{
    return 1;
}

gid_t getgid(void)
{
    return 1;
}

gid_t getegid(void)
{
    return 1;
}

/* Guests have no supplementary identities or host group database. */
gid_t *bb_getgroups(int *count, gid_t *groups)
{
    *count = 0;
    return groups;
}

struct passwd *getpwnam(const char *name)
{
    (void)name;
    errno = ENOENT;
    return NULL;
}

/* Interfaces which need later platform work report failure explicitly. */
int shell_builtin_ulimit(char **arguments)
{
    (void)arguments;
    bb_simple_error_msg("ulimit is not implemented in this port");
    return 2;
}
clock_t times(struct tms *result)
{
    (void)result;
    errno = ENOTSUP;
    return (clock_t)-1;
}

unsigned bb_clk_tck(void)
{
    return 100;
}

mode_t umask(mode_t mask)
{
    (void)mask;
    errno = ENOTSUP;
    return (mode_t)-1;
}

mode_t bb_parse_mode(const char *text, mode_t initial)
{
    (void)text;
    (void)initial;
    errno = ENOTSUP;
    return (mode_t)-1;
}

const char *get_signame(int number)
{
    return number == 0 ? "EXIT" : "UNSUPPORTED";
}
int get_signum(const char *name)
{
    return strcmp(name, "EXIT") == 0 || strcmp(name, "0") == 0 ? 0 : -1;
}
void (*signal(int number, void (*handler)(int)))(int)
{
    (void)number;
    (void)handler;
    errno = ENOTSUP;
    return SIG_ERR;
}
