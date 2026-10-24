/* Keep libc relative paths tied to the runtime's retained directory anchor. */
#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <wasi/api.h>

/* Link with --undefined=chdir so wasm-ld resolves this archive reference even
 * when the application never calls chdir itself. */
int __real_chdir(const char *path);

static int cwd_request(uint32_t operation, const char *path, uint32_t reply[2])
{
    size_t length = path ? strlen(path) : 0;
    const char *prefix = "/proc/self/cwd/";
    size_t user_length = length;
    if (path && strncmp(path, prefix, strlen(prefix)) == 0)
        user_length -= strlen(prefix);
    if (user_length > 4096) {
        errno = ENAMETOOLONG;
        return -1;
    }
    /* Bypass libc's shared path buffer, which can be active in chdir. */
    __wasi_fd_t channel;
    __wasi_errno_t error = __wasi_path_open(3, 0, ".efi-process", 0,
        __WASI_RIGHTS_FD_READ | __WASI_RIGHTS_FD_WRITE, 0, 0, &channel);
    if (error) {
        errno = error;
        return -1;
    }
    size_t size = sizeof(operation) + length;
    unsigned char *packet = malloc(size);
    if (!packet) {
        close(channel);
        errno = ENOMEM;
        return -1;
    }
    memcpy(packet, &operation, sizeof(operation));
    if (length)
        memcpy(packet + sizeof(operation), path, length);
    ssize_t count = write(channel, packet, size);
    free(packet);
    if (count != (ssize_t)size) {
        int saved = count < 0 ? errno : EIO;
        close(channel);
        errno = saved;
        return -1;
    }
    count = read(channel, reply, 8);
    int saved = errno;
    close(channel);
    if (count != 8) {
        errno = count < 0 ? saved : EIO;
        return -1;
    }
    if ((int32_t)reply[0] < 0) {
        /* Frame errors use process errno values. Filesystem errors use WASI. */
        switch (-(int32_t)reply[0]) {
        case 7: errno = E2BIG; break;
        case 22: errno = EINVAL; break;
        case 24: errno = EMFILE; break;
        default: errno = -(int32_t)reply[0]; break;
        }
        return -1;
    }
    return 0;
}

__attribute__((constructor))
void efi_filesystem_initialize(void)
{
    if (__real_chdir("/proc/self/cwd") < 0) {
        perror("current directory");
        exit(1);
    }
}

char *__wrap_getcwd(char *buffer, size_t capacity)
{
    if (buffer && capacity == 0) {
        errno = EINVAL;
        return NULL;
    }
    uint32_t reply[2];
    if (cwd_request(16, NULL, reply) < 0)
        return NULL;
    int fd = reply[0];
    size_t size = reply[1];
    if (capacity && capacity <= size) {
        close(fd);
        errno = ERANGE;
        return NULL;
    }
    int allocated = buffer == NULL;
    if (!buffer) {
        buffer = malloc(capacity ? capacity : size + 1);
        if (!buffer) {
            close(fd);
            errno = ENOMEM;
            return NULL;
        }
    }
    size_t copied = 0;
    while (copied < size) {
        ssize_t count = read(fd, buffer + copied, size - copied);
        if (count <= 0) {
            int error = count < 0 ? errno : EIO;
            close(fd);
            if (allocated)
                free(buffer);
            errno = error;
            return NULL;
        }
        copied += count;
    }
    close(fd);
    buffer[size] = '\0';
    return buffer;
}

int __wrap_chdir(const char *path)
{
    if (!*path) {
        errno = ENOENT;
        return -1;
    }
    char *absolute = NULL;
    if (path[0] != '/') {
        const char *prefix = "/proc/self/cwd/";
        size_t length = strlen(path);
        if (length > 4096) {
            errno = ENAMETOOLONG;
            return -1;
        }
        absolute = malloc(strlen(prefix) + length + 1);
        if (!absolute)
            return -1;
        strcpy(absolute, prefix);
        strcat(absolute, path);
    }
    uint32_t reply[2];
    int result = cwd_request(17, absolute ? absolute : path, reply);
    free(absolute);
    if (result < 0)
        return -1;
    if (reply[0]) {
        errno = reply[0];
        return -1;
    }
    return 0;
}
