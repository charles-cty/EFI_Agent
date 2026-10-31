#include "libbb.h"

smallint logmode = LOGMODE_STDIO;
const int const_int_0;
const char bb_hexdigits_upcase[] = "0123456789ABCDEF";

FILE *xfopen_for_read(const char *path)
{
    return xfopen(path, "r");
}

char *skip_non_whitespace(const char *text)
{
    while (*text && !isspace((unsigned char)*text))
        ++text;
    return (char *)text;
}

void bb_verror_msg(const char *format, va_list arguments, const char *error)
{
    if (logmode == LOGMODE_NONE)
        return;
    vfprintf(stderr, format, arguments);
    if (error)
        fprintf(stderr, ": %s", error);
    fputc('\n', stderr);
}

char *xmalloc_reads(int fd, size_t *maximum)
{
    size_t limit = maximum ? *maximum : INT_MAX;
    size_t used = 0;
    size_t capacity = 128;
    char *buffer = xmalloc(capacity);
    /* patch requires the descriptor to stop immediately after this line. */
    while (used < limit) {
        char byte;
        ssize_t count = nonblock_immune_read(fd, &byte, 1);
        if (count != 1) {
            if (!used) {
                free(buffer);
                return NULL;
            }
            break;
        }
        if (byte == '\n')
            break;
        if (used + 1 == capacity) {
            capacity += 128;
            buffer = xrealloc(buffer, capacity);
        }
        buffer[used++] = byte;
    }
    buffer[used] = '\0';
    if (maximum)
        *maximum = used;
    return buffer;
}

void *malloc_or_warn(size_t size)
{
    void *result = malloc(size);
    if (!result)
        bb_simple_error_msg(bb_msg_memory_exhausted);
    return result;
}

void *xmmap_anon(size_t size)
{
    /* dd requires an aligned zero-filled buffer, with process lifetime. */
    void *buffer = NULL;
    int error = posix_memalign(&buffer, bb_getpagesize(), size);
    if (error) {
        errno = error;
        bb_simple_perror_msg_and_die("allocate aligned buffer");
    }
    memset(buffer, 0, size);
    return buffer;
}
