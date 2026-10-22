#include "libbb.h"
#include "common_bufsiz.h"
#include <wasi/api.h>

char bb_common_bufsiz1[COMMON_BUFSIZE] __attribute__((aligned(16)));
const char bb_msg_standard_input[] = "standard input";

void setup_common_bufsiz(void)
{
    memset(bb_common_bufsiz1, 0, sizeof(bb_common_bufsiz1));
}

unsigned string_array_len(char **array)
{
    unsigned length = 0;
    while (array[length])
        ++length;
    return length;
}

int xatoi_positive(const char *text)
{
    char *end;
    errno = 0;
    unsigned long value = strtoul(text, &end, 10);
    if (!*text || *text == '-' || *end || errno || value > INT_MAX)
        bb_error_msg_and_die("invalid number '%s'", text);
    return (int)value;
}

FILE *xfopen_for_write(const char *path)
{
    return xfopen(path, "w");
}

FILE *xfdopen_for_write(int fd)
{
    FILE *stream = fdopen(fd, "w");
    if (!stream)
        bb_simple_perror_msg_and_die("fdopen");
    return stream;
}

FILE *fopen_or_warn(const char *path, const char *mode)
{
    FILE *stream = fopen(path, mode);
    if (!stream)
        bb_simple_perror_msg(path);
    return stream;
}

int fclose_if_not_stdin(FILE *stream)
{
    return stream == stdin ? 0 : fclose(stream);
}

int xmkstemp(char *name)
{
    static const char alphabet[] =
        "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_-";
    size_t length = strlen(name);
    if (length < 6 || strcmp(name + length - 6, "XXXXXX") != 0)
        bb_simple_error_msg_and_die("temporary name requires XXXXXX suffix");
    for (unsigned attempt = 0; attempt < 128; ++attempt) {
        unsigned char bytes[6];
        __wasi_errno_t error = __wasi_random_get(bytes, sizeof(bytes));
        if (error != 0) {
            errno = error;
            bb_simple_perror_msg_and_die("temporary name entropy");
        }
        for (unsigned index = 0; index < sizeof(bytes); ++index)
            name[length - 6 + index] = alphabet[bytes[index] & 63];
        int fd = open(name, O_CREAT | O_EXCL | O_RDWR, 0600);
        if (fd >= 0)
            return fd;
        if (errno != EEXIST)
            bb_simple_perror_msg_and_die(name);
    }
    bb_simple_perror_msg_and_die(name);
}

void xrename(const char *source, const char *destination)
{
    if (rename(source, destination) < 0)
        bb_perror_msg_and_die("rename '%s' to '%s'", source, destination);
}
