#include "libbb.h"
#include <mntent.h>
#include <sys/statfs.h>
#include <sys/statvfs.h>

int efi_filesystem_statistics(int fd, const char *path, uint64_t fields[12]);
int efi_mount_snapshot(void);

static int statistics(int fd, const char *path, struct statvfs *info)
{
    uint64_t fields[12];
    if (efi_filesystem_statistics(fd, path, fields) < 0)
        return -1;
    memset(info, 0, sizeof(*info));
    info->f_bsize = fields[0];
    info->f_frsize = fields[1];
    info->f_blocks = fields[2];
    info->f_bfree = fields[3];
    info->f_bavail = fields[4];
    info->f_files = fields[5];
    info->f_ffree = fields[6];
    info->f_favail = fields[7];
    info->f_fsid = fields[8];
    info->f_flag = fields[9];
    info->f_namemax = fields[10];
    info->f_type = fields[11];
    return 0;
}

int __wrap_statvfs(const char *path, struct statvfs *info)
{
    return statistics(AT_FDCWD, path, info);
}

int __wrap_fstatvfs(int fd, struct statvfs *info)
{
    return statistics(fd, NULL, info);
}

int statfs(const char *path, struct statfs *info)
{
    uint64_t fields[12];
    if (efi_filesystem_statistics(AT_FDCWD, path, fields) < 0)
        return -1;
    memset(info, 0, sizeof(*info));
    info->f_bsize = fields[0];
    info->f_frsize = fields[1];
    info->f_blocks = fields[2];
    info->f_bfree = fields[3];
    info->f_bavail = fields[4];
    info->f_files = fields[5];
    info->f_ffree = fields[6];
    /* BusyBox reads fsid words most significant first. Runtime IDs are u32. */
    info->f_fsid[1] = fields[8];
    info->f_flags = fields[9];
    info->f_namelen = fields[10];
    info->f_type = fields[11];
    return 0;
}

const char bb_path_mtab_file[] = "/proc/mounts";

FILE *setmntent(const char *path, const char *mode)
{
    if (strcmp(path, bb_path_mtab_file) != 0 || strcmp(mode, "r") != 0) {
        errno = EINVAL;
        return NULL;
    }
    int fd = efi_mount_snapshot();
    if (fd < 0)
        return NULL;
    FILE *stream = fdopen(fd, "r");
    if (!stream)
        close(fd);
    return stream;
}

static char *unescape(char *field)
{
    char *output = field;
    for (char *input = field; *input;) {
        if (input[0] == '\\' && input[1] >= '0' && input[1] <= '7'
            && input[2] >= '0' && input[2] <= '7'
            && input[3] >= '0' && input[3] <= '7') {
            *output++ = ((input[1] - '0') << 6) | ((input[2] - '0') << 3) | (input[3] - '0');
            input += 4;
        } else {
            *output++ = *input++;
        }
    }
    *output = '\0';
    return field;
}

struct mntent *getmntent(FILE *stream)
{
    static struct mntent entry;
    static char *line;
    static size_t capacity;
    if (getline(&line, &capacity, stream) < 0) {
        if (ferror(stream))
            bb_simple_perror_msg_and_die("mount table");
        return NULL;
    }
    char *fields[6];
    char *state = NULL;
    char *field = strtok_r(line, " \t\n", &state);
    for (unsigned i = 0; i < 6; i++) {
        if (!field)
            bb_simple_error_msg_and_die("invalid mount table");
        fields[i] = field;
        field = strtok_r(NULL, " \t\n", &state);
    }
    if (field)
        bb_simple_error_msg_and_die("invalid mount table");
    entry.mnt_fsname = unescape(fields[0]);
    entry.mnt_dir = unescape(fields[1]);
    entry.mnt_type = unescape(fields[2]);
    entry.mnt_opts = fields[3];
    entry.mnt_freq = 0;
    entry.mnt_passno = 0;
    return &entry;
}

int endmntent(FILE *stream)
{
    if (fclose(stream) != 0)
        bb_simple_perror_msg_and_die("mount table");
    return 1;
}
