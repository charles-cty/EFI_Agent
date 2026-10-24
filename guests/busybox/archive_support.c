#include "libbb.h"
#include "bb_archive.h"

struct archive_attributes {
    struct archive_attributes *next;
    char *path;
    mode_t mode;
    unsigned depth;
    bool restore_time;
    struct timespec times[2];
};

static struct archive_attributes *deferred;

static void archive_times(struct timespec times[2], unsigned date, unsigned value)
{
    struct tm calendar = {
        .tm_year = 80 + (date >> 9),
        .tm_mon = ((date >> 5) & 15) - 1,
        .tm_mday = date & 31,
        .tm_hour = value >> 11,
        .tm_min = (value >> 5) & 63,
        .tm_sec = (value & 31) * 2,
    };
    struct tm original = calendar;
    time_t seconds = mktime(&calendar);
    if (seconds < 0 || calendar.tm_year != original.tm_year
        || calendar.tm_mon != original.tm_mon || calendar.tm_mday != original.tm_mday
        || calendar.tm_hour != original.tm_hour || calendar.tm_min != original.tm_min
        || calendar.tm_sec != original.tm_sec)
        bb_simple_error_msg_and_die("invalid ZIP timestamp");
    times[0] = times[1] = (struct timespec){.tv_sec = seconds};
}

void efi_archive_file_attributes(int fd, mode_t mode, unsigned date, unsigned time)
{
    struct timespec times[2];
    if (date) {
        archive_times(times, date, time);
        if (futimens(fd, times) < 0)
            bb_simple_perror_msg_and_die("archive file timestamp");
    }
    if (fchmod(fd, mode & 07777) < 0)
        bb_simple_perror_msg_and_die("archive file mode");
}

static struct archive_attributes *remember_attributes(const char *path, mode_t mode)
{
    struct archive_attributes *entry;
    char *normalized = xstrdup(path);
    size_t length = strlen(normalized);
    while (length > 1 && normalized[length - 1] == '/')
        normalized[--length] = '\0';
    for (entry = deferred; entry; entry = entry->next)
        if (strcmp(entry->path, normalized) == 0)
            break;
    if (!entry) {
        entry = xzalloc(sizeof(*entry));
        entry->path = normalized;
        for (const char *part = entry->path; *part; ++part)
            entry->depth += *part == '/';
        entry->next = deferred;
        deferred = entry;
    } else
        free(normalized);
    entry->mode = mode;
    return entry;
}

void efi_archive_remember_attributes(const char *path, mode_t mode,
    unsigned date, unsigned time)
{
    struct archive_attributes *entry = remember_attributes(path, mode);
    entry->restore_time = date != 0;
    if (entry->restore_time)
        archive_times(entry->times, date, time);
}

void efi_tar_attributes(const char *path, const file_header_t *header, unsigned flags)
{
    mode_t mode = header->mode;
    if (flags & ARCHIVE_DONT_RESTORE_PERM) {
        mode_t mask = umask(0);
        umask(mask);
        mode &= ~mask;
    }
    struct archive_attributes *entry = remember_attributes(path, mode);
    entry->restore_time = (flags & ARCHIVE_RESTORE_DATE) != 0;
    entry->times[0] = entry->times[1] = (struct timespec){.tv_sec = header->mtime};
}

void efi_archive_begin(void)
{
    if (efi_archive_root_fd != -1)
        __builtin_trap();
    efi_archive_root_fd = xopen(".", O_RDONLY | O_DIRECTORY | O_CLOEXEC);
}

void efi_archive_end(void)
{
    /* Children must be restored before a parent's restrictive mode or mtime. */
    while (deferred) {
        struct archive_attributes **selected = &deferred;
        for (struct archive_attributes **item = &deferred; *item; item = &(*item)->next)
            if ((*item)->depth > (*selected)->depth)
                selected = item;
        struct archive_attributes *entry = *selected;
        *selected = entry->next;
        if (entry->restore_time) {
            if (utimensat(efi_archive_root_fd, entry->path, entry->times, AT_SYMLINK_NOFOLLOW) < 0)
                bb_simple_perror_msg_and_die(entry->path);
        }
        if (!S_ISLNK(entry->mode) && chmod(entry->path, entry->mode & 07777) < 0)
            bb_simple_perror_msg_and_die(entry->path);
        free(entry->path);
        free(entry);
    }
    int fd = efi_archive_root_fd;
    efi_archive_root_fd = -1;
    if (fd != -1)
        xclose(fd);
}

uint32_t efi_archive_crc(const void *data, size_t size)
{
    if (!global_crc32_table)
        global_crc32_new_table_le();
    return ~crc32_block_endian0(~0u, data, size, global_crc32_table);
}

int xopen3(const char *path, int flags, int mode)
{
    int fd = open(path, flags, mode);
    if (fd < 0)
        bb_simple_perror_msg_and_die(path);
    return fd;
}

void xread(int fd, void *data, size_t size)
{
    ssize_t count = full_read(fd, data, size);
    if (count < 0)
        bb_simple_perror_msg_and_die(bb_msg_read_error);
    if ((size_t)count != size)
        bb_simple_error_msg_and_die("short read");
}

void xchdir(const char *path)
{
    if (chdir(path) < 0)
        bb_simple_perror_msg_and_die(path);
}
