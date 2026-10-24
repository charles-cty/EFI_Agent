#include "libbb.h"
#include <wasi/api.h>

/* Each applet Store owns its extraction directory anchor. */
int efi_archive_root_fd = -1;

/* Preview 1 does not carry POSIX permissions. Share the private endpoint
 * used by the unchanged Python distribution instead of changing its ABI. */
int __real_open(const char *path, int flags, ...);
int __real_stat(const char *path, struct stat *info);
int __real_lstat(const char *path, struct stat *info);
int __real_fstat(int fd, struct stat *info);
int __real_fstatat(int fd, const char *path, struct stat *info, int flags);
int __real_mkdir(const char *path, mode_t mode);
int __real_chdir(const char *path);
int __real_symlink(const char *target, const char *path);
int __wrap_fstatat(int fd, const char *path, struct stat *info, int flags);

static int path_fd(int fd, const char *path)
{
    if (efi_archive_root_fd < 0)
        return fd;
    if (path[0] == '/') {
        errno = ENOTCAPABLE;
        return -1;
    }
    return fd == AT_FDCWD ? efi_archive_root_fd : fd;
}

static int request(uint32_t *words, size_t count, const char *path, uint32_t response[2])
{
    size_t length = path ? strlen(path) : 0;
    if (length > 4096) {
        errno = ENAMETOOLONG;
        return -1;
    }
    /* Do not use libc path resolution here. Metadata wrappers can run inside
     * chdir/fstatat while libc's shared path scratch buffer is still live. */
    __wasi_fd_t channel;
    __wasi_errno_t opened = __wasi_path_open(3, 0, ".efi-process", 0,
        __WASI_RIGHTS_FD_READ | __WASI_RIGHTS_FD_WRITE, 0, 0, &channel);
    if (opened) {
        errno = opened;
        return -1;
    }
    size_t size = count * 4 + length;
    unsigned char *packet = xmalloc(size);
    memcpy(packet, words, count * 4);
    if (length)
        memcpy(packet + count * 4, path, length);
    ssize_t written = write(channel, packet, size);
    free(packet);
    if (written != (ssize_t)size)
        bb_simple_error_msg_and_die("incomplete metadata request");
    if (read(channel, response, 8) != 8)
        bb_simple_error_msg_and_die("incomplete metadata response");
    close(channel);
    if ((int32_t)response[0] < 0) {
        errno = EINVAL;
        return -1;
    }
    return 0;
}

void efi_busybox_restore_cwd(void)
{
    uint32_t words[] = {16};
    uint32_t response[2];
    if (request(words, 1, NULL, response) < 0)
        bb_simple_perror_msg_and_die("current directory");
    int fd = response[0];
    char path[4098];
    ssize_t size = read(fd, path, sizeof(path) - 1);
    close(fd);
    if (size < 0)
        bb_simple_perror_msg_and_die("current directory");
    path[size] = '\0';
    if (__real_chdir(path) < 0)
        bb_simple_perror_msg_and_die(path);
}

int __wrap_chdir(const char *path)
{
    char *previous = getcwd(NULL, 0);
    if (!previous)
        return -1;
    if (__real_chdir(path) < 0) {
        free(previous);
        return -1;
    }
    char *absolute = getcwd(NULL, 0);
    if (!absolute) {
        free(previous);
        return -1;
    }
    uint32_t words[] = {17};
    uint32_t response[2];
    int result = request(words, 1, absolute, response);
    free(absolute);
    if (result < 0 || response[0]) {
        int error = result < 0 ? errno : (int)response[0];
        if (__real_chdir(previous) < 0)
            bb_simple_perror_msg_and_die("restore current directory");
        free(previous);
        errno = error;
        return -1;
    }
    free(previous);
    return 0;
}

static int metadata(uint32_t operation, int fd, const char *path, int follow, mode_t *mode)
{
    char *absolute = NULL;
    if (path) {
        fd = path_fd(fd, path);
        if (fd == -1)
            return -1;
        if (*path == '/') {
            fd = 3;
        } else if (fd == AT_FDCWD) {
            char *cwd = getcwd(NULL, 0);
            if (!cwd)
                return -1;
            absolute = xasprintf("%s%s%s", cwd, strcmp(cwd, "/") == 0 ? "" : "/", path);
            free(cwd);
            path = absolute;
            fd = 3;
        }
    }
    uint32_t words[] = {operation, (uint32_t)fd, (uint32_t)follow, (uint32_t)*mode};
    if (operation == 14)
        words[2] = *mode;
    unsigned count;
    switch (operation) {
    case 11: count = 3; break;
    case 12: count = 4; break;
    case 13: count = 2; break;
    case 14: count = 3; break;
    default: __builtin_trap();
    }
    uint32_t response[2];
    int result = request(words, count, path, response);
    free(absolute);
    if (result < 0)
        return -1;
    if (response[0]) {
        errno = response[0];
        return -1;
    }
    *mode = response[1];
    return 0;
}

static int patch_stat(int result, int fd, const char *path, int follow, struct stat *info)
{
    if (result < 0)
        return result;
    mode_t mode = 0;
    if (metadata(path ? 11 : 13, fd, path, follow, &mode) < 0)
        return -1;
    info->st_mode = (info->st_mode & ~07777) | mode;
    info->st_uid = 1;
    info->st_gid = 1;
    return 0;
}

int __wrap_stat(const char *path, struct stat *info)
{
    if (efi_archive_root_fd >= 0)
        return __wrap_fstatat(AT_FDCWD, path, info, 0);
    return patch_stat(__real_stat(path, info), AT_FDCWD, path, 1, info);
}

int __wrap_lstat(const char *path, struct stat *info)
{
    if (efi_archive_root_fd >= 0)
        return __wrap_fstatat(AT_FDCWD, path, info, AT_SYMLINK_NOFOLLOW);
    return patch_stat(__real_lstat(path, info), AT_FDCWD, path, 0, info);
}

int __wrap_fstat(int fd, struct stat *info)
{
    return patch_stat(__real_fstat(fd, info), fd, NULL, 1, info);
}

int __wrap_fstatat(int fd, const char *path, struct stat *info, int flags)
{
    fd = path_fd(fd, path);
    if (fd == -1)
        return -1;
    return patch_stat(__real_fstatat(fd, path, info, flags), fd, path,
        !(flags & AT_SYMLINK_NOFOLLOW), info);
}

int __wrap_chmod(const char *path, mode_t mode)
{
    mode &= 07777;
    return metadata(12, AT_FDCWD, path, 1, &mode);
}

int __wrap_fchmod(int fd, mode_t mode)
{
    mode &= 07777;
    return metadata(14, fd, NULL, 1, &mode);
}

mode_t umask(mode_t mask)
{
    uint32_t words[] = {15, mask};
    uint32_t response[2];
    if (request(words, 2, NULL, response) < 0)
        return (mode_t)-1;
    return response[0];
}

static uint32_t creation_mode(uint32_t mode)
{
    uint32_t words[] = {19, mode};
    uint32_t response[2];
    if (request(words, 2, NULL, response) < 0 || response[0])
        bb_simple_error_msg_and_die("creation mode request failed");
    return response[1];
}

int __wrap_mkdir(const char *path, mode_t mode)
{
    int fd = path_fd(AT_FDCWD, path);
    if (fd == -1)
        return -1;
    uint32_t previous = creation_mode(mode & 07777);
    int result = fd == AT_FDCWD ? __real_mkdir(path, mode) : mkdirat(fd, path, mode);
    int error = errno;
    creation_mode(previous);
    errno = error;
    return result;
}

int __wrap_open(const char *path, int flags, ...)
{
    int directory = path_fd(AT_FDCWD, path);
    if (directory == -1)
        return -1;
    mode_t mode = 0;
    if (flags & O_CREAT) {
        va_list arguments;
        va_start(arguments, flags);
        mode = va_arg(arguments, mode_t);
        va_end(arguments);
        uint32_t previous = creation_mode(mode & 07777);
        int fd = directory == AT_FDCWD ? __real_open(path, flags, mode)
            : openat(directory, path, flags, mode);
        int error = errno;
        creation_mode(previous);
        errno = error;
        return fd;
    }
    return directory == AT_FDCWD ? __real_open(path, flags, mode)
        : openat(directory, path, flags, mode);
}

int __wrap_symlink(const char *target, const char *path)
{
    int fd = path_fd(AT_FDCWD, path);
    if (fd == -1)
        return -1;
    return fd == AT_FDCWD ? __real_symlink(target, path) : symlinkat(target, fd, path);
}
