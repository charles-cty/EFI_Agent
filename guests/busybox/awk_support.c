#include "busybox.h"
#include "awk_support.h"

/* Each command has its own instance. Keep popen ownership local to it. */
struct command_stream {
    FILE *stream;
    uint32_t pid;
    struct command_stream *next;
};
static struct command_stream *command_streams;

void bb_die_memory_exhausted(void)
{
    bb_simple_error_msg_and_die(bb_msg_memory_exhausted);
}

void bb_simple_perror_msg_and_die(const char *text)
{
    bb_simple_perror_msg(text);
    xfunc_die();
}

void bb_perror_msg_and_die(const char *format, ...)
{
    if (logmode == LOGMODE_NONE)
        xfunc_die();
    int error = errno;
    va_list arguments;
    va_start(arguments, format);
    vfprintf(stderr, format, arguments);
    va_end(arguments);
    fprintf(stderr, ": %s\n", strerror(error));
    xfunc_die();
}

char *xstrndup(const char *value, size_t length)
{
    size_t size = strnlen(value, length);
    char *result = xmalloc(size + 1);
    memcpy(result, value, size);
    result[size] = '\0';
    return result;
}

char *xasprintf(const char *format, ...)
{
    va_list arguments;
    va_list copy;
    va_start(arguments, format);
    va_copy(copy, arguments);
    int size = vsnprintf(NULL, 0, format, copy);
    va_end(copy);
    if (size < 0)
        bb_simple_error_msg_and_die("cannot format string");
    char *result = xmalloc((size_t)size + 1);
    int written = vsnprintf(result, (size_t)size + 1, format, arguments);
    va_end(arguments);
    if (written != size)
        bb_simple_error_msg_and_die("formatted string size changed");
    return result;
}

FILE *xfopen(const char *path, const char *mode)
{
    FILE *stream = fopen(path, mode);
    if (!stream)
        bb_simple_perror_msg_and_die(path);
    return stream;
}

FILE *fopen_for_read(const char *path)
{
    return fopen(path, "r");
}

char *xmalloc_read(int fd, size_t *size)
{
    size_t limit = size ? *size : SIZE_MAX - 1;
    size_t length = 0;
    size_t capacity = 0;
    char *result = NULL;
    while (length < limit) {
        size_t chunk = limit - length;
        if (chunk > 65536)
            chunk = 65536;
        capacity = length + chunk + 1;
        result = xrealloc(result, capacity);
        ssize_t count = safe_read(fd, result + length, chunk);
        if (count < 0) {
            int error = errno;
            free(result);
            errno = error;
            return NULL;
        }
        if (count == 0)
            break;
        length += (size_t)count;
    }
    if (!result)
        result = xmalloc(1);
    result[length] = '\0';
    if (size)
        *size = length;
    return result;
}

static int spawn_error(int32_t result)
{
    switch (result) {
    case -2: errno = ENOENT; break;
    case -7: errno = E2BIG; break;
    case -8: errno = ENOEXEC; break;
    case -9: errno = EBADF; break;
    case -11: errno = EAGAIN; break;
    case -12: errno = ENOMEM; break;
    case -14: errno = EFAULT; break;
    case -22: errno = EINVAL; break;
    case -24: errno = EMFILE; break;
    default: __builtin_trap();
    }
    return -1;
}

static int spawn_shell(const char *command, int pipe_end, int target)
{
    const char *arguments[] = {"/bin/ash", "-c", command};
    uint32_t environment_count = 0;
    while (environ && environ[environment_count])
        ++environment_count;
    struct efi_process_fd_mapping mappings[127];
    unsigned count = efi_ash_inherit_fds(mappings);
    if (pipe_end >= 0) {
        unsigned index = 0;
        while (index < count && mappings[index].target != (unsigned)target)
            ++index;
        if (index == count)
            ++count;
        mappings[index] = (struct efi_process_fd_mapping){(uint32_t)target, (uint32_t)pipe_end};
    }
    char *scratch = malloc(65536);
    if (!scratch) {
        errno = ENOMEM;
        return -1;
    }
    fflush_all();
    int32_t pid = efi_process_spawn_argv_env_mapped(arguments, ARRAY_SIZE(arguments),
        (const char *const *)environ, environment_count, scratch, 65536, mappings, count);
    free(scratch);
    return pid < 0 ? spawn_error(pid) : pid;
}

static int wait_shell(uint32_t pid)
{
    int32_t status;
    int32_t result = efi_process_waitpid(pid, &status);
    if (result < 0) {
        errno = ECHILD;
        return -1;
    }
    if (result != (int32_t)pid)
        __builtin_trap();
    return ((uint32_t)status & 255) << 8;
}

int efi_bb_system(const char *command)
{
    if (!command)
        return efi_process_program_exists("ash", 3) == 1;
    int pid = spawn_shell(command, -1, -1);
    return pid < 0 ? -1 : wait_shell((uint32_t)pid);
}

FILE *efi_bb_popen(const char *command, const char *mode)
{
    if (strcmp(mode, "r") != 0 && strcmp(mode, "w") != 0) {
        errno = EINVAL;
        return NULL;
    }
    struct command_stream *entry = malloc(sizeof(*entry));
    if (!entry) {
        errno = ENOMEM;
        return NULL;
    }
    int descriptors[2];
    if (pipe(descriptors) < 0) {
        free(entry);
        return NULL;
    }
    int reading = mode[0] == 'r';
    int parent = descriptors[reading ? 0 : 1];
    int child = descriptors[reading ? 1 : 0];
    FILE *stream = fdopen(parent, mode);
    if (!stream) {
        int error = errno;
        close(parent);
        close(child);
        free(entry);
        errno = error;
        return NULL;
    }
    /* Both original ends are close-on-exec. Only the mapped standard
     * descriptor reaches the child; prior popen streams also stay private. */
    int pid = spawn_shell(command, child, reading ? STDOUT_FILENO : STDIN_FILENO);
    int error = errno;
    close(child);
    if (pid < 0) {
        fclose(stream);
        free(entry);
        errno = error;
        return NULL;
    }
    *entry = (struct command_stream){stream, (uint32_t)pid, command_streams};
    command_streams = entry;
    return stream;
}

int efi_bb_pclose(FILE *stream)
{
    struct command_stream **link = &command_streams;
    while (*link && (*link)->stream != stream)
        link = &(*link)->next;
    if (!*link) {
        errno = EINVAL;
        return -1;
    }
    struct command_stream *entry = *link;
    *link = entry->next;
    uint32_t pid = entry->pid;
    free(entry);
    int closed = fclose(stream);
    int error = errno;
    int status = wait_shell(pid);
    if (closed != 0) {
        errno = error;
        return -1;
    }
    return status;
}
