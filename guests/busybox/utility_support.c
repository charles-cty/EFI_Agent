#include "libbb.h"
#include "common_bufsiz.h"
#include <wasi/api.h>

char bb_common_bufsiz1[COMMON_BUFSIZE] __attribute__((aligned(16)));
const char bb_msg_standard_input[] = "standard input";
const char bb_msg_invalid_arg_to[] = "invalid argument '%s' to '%s'";

DIR *warn_opendir(const char *path)
{
    DIR *directory = opendir(path);
    if (!directory)
        bb_perror_msg("can't open '%s'", path);
    return directory;
}

int FAST_FUNC get_terminal_width(int fd)
{
    const char *columns = getenv("COLUMNS");
    int width = columns ? atoi(columns) : 80;

    (void)fd;
    return width > 1 && width < 30000 ? width : 80;
}


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

char *bin2hex(char *destination, const char *source, int count)
{
    static const char digits[] = "0123456789abcdef";
    for (int index = 0; index < count; ++index) {
        unsigned char value = (unsigned char)source[index];
        destination[index * 2] = digits[value >> 4];
        destination[index * 2 + 1] = digits[value & 15];
    }
    destination[count * 2] = '\0';
    return destination;
}

char *xmalloc_realpath_coreutils(char *path)
{
    char *resolved = realpath(path, NULL);
    if (resolved || errno != ENOENT)
        return resolved;

    char *link_target = malloc(PATH_MAX + 1);
    if (!link_target)
        return NULL;
    ssize_t link_size = readlink(path, link_target, PATH_MAX);
    if (link_size >= 0 && link_size < PATH_MAX) {
        link_target[link_size] = '\0';
        if (link_target[0] == '/')
            return xmalloc_realpath_coreutils(link_target);
        char *slash = strrchr(path, '/');
        size_t parent_size = slash ? (size_t)(slash - path + 1) : 0;
        size_t target_size = strlen(link_target);
        char *joined = malloc(parent_size + target_size + 1);
        if (!joined) {
            free(link_target);
            return NULL;
        }
        memcpy(joined, path, parent_size);
        memcpy(joined + parent_size, link_target, target_size + 1);
        free(link_target);
        resolved = xmalloc_realpath_coreutils(joined);
        free(joined);
        return resolved;
    }
    free(link_target);

    size_t path_size = strlen(path);
    if (path_size == 0 || path[path_size - 1] == '/')
        return NULL;
    char *slash = strrchr(path, '/');
    const char *name = slash ? slash + 1 : path;
    if (!*name)
        return NULL;
    char *parent;
    if (!slash)
        parent = realpath(".", NULL);
    else if (slash == path)
        parent = realpath("/", NULL);
    else {
        size_t parent_size = (size_t)(slash - path);
        char *parent_path = malloc(parent_size + 1);
        if (!parent_path)
            return NULL;
        memcpy(parent_path, path, parent_size);
        parent_path[parent_size] = '\0';
        parent = realpath(parent_path, NULL);
        free(parent_path);
    }
    if (!parent)
        return NULL;
    size_t parent_size = strlen(parent);
    size_t name_size = strlen(name);
    char *result = malloc(parent_size + name_size + 2);
    if (!result) {
        free(parent);
        return NULL;
    }
    memcpy(result, parent, parent_size);
    if (parent_size != 1 || parent[0] != '/')
        result[parent_size++] = '/';
    memcpy(result + parent_size, name, name_size + 1);
    free(parent);
    return result;
}

char *xmalloc_readlink_or_warn(const char *path)
{
    char *target = xmalloc(PATH_MAX + 1);
    ssize_t length = readlink(path, target, PATH_MAX);
    if (length < 0) {
        int error = errno;
        free(target);
        bb_error_msg("%s: cannot read link: %s", path,
                     error == EINVAL ? "not a symlink" : strerror(error));
        return NULL;
    }
    target[length] = '\0';
    return target;
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

int open3_or_warn(const char *path, int flags, int mode)
{
    int fd = open(path, flags, mode);
    if (fd < 0)
        bb_simple_perror_msg(path);
    return fd;
}

static int preserve_owner(const char *path, uid_t owner, gid_t group, int follow)
{
    struct stat info;
    if ((follow ? stat(path, &info) : lstat(path, &info)) < 0)
        return -1;
    if ((owner != (uid_t)-1 && owner != info.st_uid)
        || (group != (gid_t)-1 && group != info.st_gid)) {
        errno = EPERM;
        return -1;
    }
    return 0;
}

int chown(const char *path, uid_t owner, gid_t group)
{
    return preserve_owner(path, owner, group, 1);
}

int lchown(const char *path, uid_t owner, gid_t group)
{
    return preserve_owner(path, owner, group, 0);
}

int mknod(const char *path, mode_t mode, dev_t device)
{
    (void)path;
    (void)mode;
    (void)device;
    errno = ENOTSUP;
    return -1;
}
