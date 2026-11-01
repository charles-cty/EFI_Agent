#include "libbb.h"

FILE *fopen_for_write(const char *path)
{
    return fopen(path, "w");
}

void *xmalloc_open_read_close(const char *path, size_t *size)
{
    int fd = open_or_warn(path, O_RDONLY);
    if (fd < 0)
        return NULL;
    char *bytes = xmalloc_read(fd, size);
    int error = errno;
    close(fd);
    errno = error;
    return bytes;
}

char xread_char(int fd)
{
    char byte;
    xread(fd, &byte, 1);
    return byte;
}

void parse_chown_usergroup_or_die(struct bb_uidgid_t *ids, const char *text)
{
    char *copy = xstrdup(text);
    char *group = strchr(copy, ':');
    if (group)
        *group++ = '\0';
    ids->uid = (uid_t)-1;
    ids->gid = (gid_t)-1;
    if (*copy) {
        struct passwd *user = getpwnam(copy);
        ids->uid = user ? user->pw_uid : xatou(copy);
    }
    if (group && *group) {
        struct group *current = getgrgid(getgid());
        ids->gid = current && strcmp(group, current->gr_name) == 0
            ? current->gr_gid : xatou(group);
    }
    free(copy);
}

/* The application owns cooked input editing. Keep piped input byte exact. */
int read_line_input(void *state UNUSED_PARAM, const char *prompt, char *buffer, int size)
{
    if (size < 2) {
        errno = EINVAL;
        return -1;
    }
    fputs(prompt, stdout);
    fflush(stdout);
    if (!fgets(buffer, size, stdin))
        return -1;
    return strlen(buffer);
}
