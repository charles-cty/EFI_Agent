#include "busybox.h"
#include "utility_support.h"

#include <dirent.h>
#include <fcntl.h>
#include <sys/stat.h>
#include <unistd.h>

static int applet_date(void)
{
    time_t now = time(NULL);
    struct tm *value = gmtime(&now);
    char text[64];
    if (!value || strftime(text, sizeof(text), "%a %b %e %H:%M:%S UTC %Y", value) == 0)
        return 1;
    puts(text);
    return 0;
}

int sleep_main(int argc, char **argv);

int sed_main(int argc, char **argv);
int cat_main(int argc, char **argv);
int head_main(int argc, char **argv);
int tail_main(int argc, char **argv);
int wc_main(int argc, char **argv);
int tee_main(int argc, char **argv);
int chmod_main(int argc, char **argv);
int awk_main(int argc, char **argv);

static int copy_stream(int input, int output, long limit)
{
    char buffer[65536];
    for (;;) {
        ssize_t count = read(input, buffer, sizeof(buffer));
        if (count == 0)
            return 0;
        if (count < 0)
            return 1;
        if (limit >= 0 && count > limit)
            count = limit;
        ssize_t written = 0;
        while (written < count) {
            ssize_t part = write(output, buffer + written, (size_t)(count - written));
            if (part <= 0)
                return 1;
            written += part;
        }
        if (limit >= 0) {
            limit -= count;
            if (limit == 0)
                return 0;
        }
    }
}

static int applet_ls(int argc, char **argv)
{
    const char *path = argc > 1 ? argv[argc - 1] : ".";
    DIR *directory = opendir(path);
    if (!directory) {
        bb_simple_perror_msg(path);
        return 1;
    }
    struct dirent *entry;
    while ((entry = readdir(directory)) != NULL) {
        if (entry->d_name[0] == '.' && entry->d_name[1] != '\0')
            continue;
        puts(entry->d_name);
    }
    closedir(directory);
    return 0;
}

static int applet_env(void)
{
    for (char **entry = environ; entry && *entry; ++entry)
        puts(*entry);
    return 0;
}

static int applet_yes(int argc, char **argv)
{
    const char *text = argc > 1 ? argv[1] : "y";
    for (;;) {
        if (puts(text) == EOF)
            return 1;
    }
}

int grep_main(int argc, char **argv);

static int applet_mkdir(int argc, char **argv)
{
    int parents = argc > 1 && strcmp(argv[1], "-p") == 0;
    int first = parents ? 2 : 1;
    int status = 0;
    for (int i = first; i < argc; ++i) {
        if (parents) {
            char path[4096];
            size_t length = strlen(argv[i]);
            if (length >= sizeof(path)) {
                status = 1;
                continue;
            }
            memcpy(path, argv[i], length + 1);
            for (char *cursor = path + 1; *cursor; ++cursor) {
                if (*cursor != '/')
                    continue;
                *cursor = '\0';
                if (*path && mkdir(path, 0777) < 0 && errno != EEXIST)
                    status = 1;
                *cursor = '/';
            }
            if (mkdir(path, 0777) < 0 && errno != EEXIST)
                status = 1;
        } else if (mkdir(argv[i], 0777) < 0) {
            bb_simple_perror_msg(argv[i]);
            status = 1;
        }
    }
    return status;
}

static int applet_rm(int argc, char **argv)
{
    int status = 0;
    for (int i = 1; i < argc; ++i) {
        if (unlink(argv[i]) < 0) {
            bb_simple_perror_msg(argv[i]);
            status = 1;
        }
    }
    return status;
}

static int applet_cp_mv(int argc, char **argv, int move)
{
    if (argc != 3) {
        fputs(move ? "mv: expected source and destination\n" : "cp: expected source and destination\n", stderr);
        return 2;
    }
    if (move)
        return rename(argv[1], argv[2]) == 0 ? 0 : (bb_simple_perror_msg(argv[1]), 1);
    int input = open(argv[1], O_RDONLY);
    int output = open(argv[2], O_WRONLY | O_CREAT | O_TRUNC, 0666);
    if (input < 0 || output < 0) {
        bb_simple_perror_msg(input < 0 ? argv[1] : argv[2]);
        if (input >= 0) close(input);
        if (output >= 0) close(output);
        return 1;
    }
    int status = copy_stream(input, output, -1);
    close(input);
    close(output);
    return status;
}

int efi_busybox_applet(int argc, char **argv)
{
    const char *name = bb_basename(argv[0]);
    applet_name = name;
    xfunc_error_retval = EXIT_FAILURE;
    if (strcmp(name, "cat") == 0) return cat_main(argc, argv);
    if (strcmp(name, "head") == 0) return head_main(argc, argv);
    if (strcmp(name, "tail") == 0) return tail_main(argc, argv);
    if (strcmp(name, "wc") == 0) return wc_main(argc, argv);
    if (strcmp(name, "tee") == 0) return tee_main(argc, argv);
    if (strcmp(name, "ls") == 0) return applet_ls(argc, argv);
    if (strcmp(name, "env") == 0) return applet_env();
    if (strcmp(name, "yes") == 0) return applet_yes(argc, argv);
    if (strcmp(name, "grep") == 0) return grep_main(argc, argv);
    if (strcmp(name, "mkdir") == 0) return applet_mkdir(argc, argv);
    if (strcmp(name, "rm") == 0) return applet_rm(argc, argv);
    if (strcmp(name, "cp") == 0) return applet_cp_mv(argc, argv, 0);
    if (strcmp(name, "mv") == 0) return applet_cp_mv(argc, argv, 1);
    if (strcmp(name, "date") == 0) return applet_date();
    if (strcmp(name, "sleep") == 0) return sleep_main(argc, argv);
    if (strcmp(name, "sed") == 0) return sed_main(argc, argv);
    if (strcmp(name, "chmod") == 0) return chmod_main(argc, argv);
    if (strcmp(name, "awk") == 0) return awk_main(argc, argv);
    fputs("busybox: applet is unavailable\n", stderr);
    return 127;
}
