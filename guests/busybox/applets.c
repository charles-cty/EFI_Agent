#include "busybox.h"
#include "utility_support.h"

#include <dirent.h>
#include <fcntl.h>
#include <sys/stat.h>
#include <unistd.h>

int date_main(int argc, char **argv);

int sleep_main(int argc, char **argv);

int sed_main(int argc, char **argv);
int cat_main(int argc, char **argv);
int head_main(int argc, char **argv);
int tail_main(int argc, char **argv);
int wc_main(int argc, char **argv);
int tee_main(int argc, char **argv);
int chmod_main(int argc, char **argv);
int awk_main(int argc, char **argv);
int basename_main(int argc, char **argv);
int dirname_main(int argc, char **argv);
int pwd_main(int argc, char **argv);
int env_main(int argc, char **argv);
int printenv_main(int argc, char **argv);
int seq_main(int argc, char **argv);
int yes_main(int argc, char **argv);
int true_main(int argc, char **argv);
int false_main(int argc, char **argv);
int mkdir_main(int argc, char **argv);
int rmdir_main(int argc, char **argv);
int rm_main(int argc, char **argv);
int touch_main(int argc, char **argv);
int sort_main(int argc, char **argv);
int uniq_main(int argc, char **argv);
int cut_main(int argc, char **argv);
int tr_main(int argc, char **argv);
int realpath_main(int argc, char **argv);
int od_main(int argc, char **argv);
int baseNUM_main(int argc, char **argv);
int md5_sha1_sum_main(int argc, char **argv);
int ln_main(int argc, char **argv);
int hexdump_main(int argc, char **argv);
int xargs_main(int argc, char **argv);
int find_main(int argc, char **argv);

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

int grep_main(int argc, char **argv);

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
    if (strcmp(name, "env") == 0) return env_main(argc, argv);
    if (strcmp(name, "yes") == 0) return yes_main(argc, argv);
    if (strcmp(name, "basename") == 0) return basename_main(argc, argv);
    if (strcmp(name, "dirname") == 0) return dirname_main(argc, argv);
    if (strcmp(name, "pwd") == 0) return pwd_main(argc, argv);
    if (strcmp(name, "printenv") == 0) return printenv_main(argc, argv);
    if (strcmp(name, "seq") == 0) return seq_main(argc, argv);
    if (strcmp(name, "true") == 0) return true_main(argc, argv);
    if (strcmp(name, "false") == 0) return false_main(argc, argv);
    if (strcmp(name, "grep") == 0) return grep_main(argc, argv);
    if (strcmp(name, "mkdir") == 0) return mkdir_main(argc, argv);
    if (strcmp(name, "rmdir") == 0) return rmdir_main(argc, argv);
    if (strcmp(name, "touch") == 0) return touch_main(argc, argv);
    if (strcmp(name, "sort") == 0) return sort_main(argc, argv);
    if (strcmp(name, "uniq") == 0) return uniq_main(argc, argv);
    if (strcmp(name, "cut") == 0) return cut_main(argc, argv);
    if (strcmp(name, "tr") == 0) return tr_main(argc, argv);
    if (strcmp(name, "realpath") == 0) return realpath_main(argc, argv);
    if (strcmp(name, "od") == 0) return od_main(argc, argv);
    if (strcmp(name, "base64") == 0) return baseNUM_main(argc, argv);
    if (strcmp(name, "sha256sum") == 0) return md5_sha1_sum_main(argc, argv);
    if (strcmp(name, "ln") == 0) return ln_main(argc, argv);
    if (strcmp(name, "hexdump") == 0) return hexdump_main(argc, argv);
    if (strcmp(name, "xargs") == 0) return xargs_main(argc, argv);
    if (strcmp(name, "find") == 0) return find_main(argc, argv);
    if (strcmp(name, "rm") == 0) return rm_main(argc, argv);
    if (strcmp(name, "cp") == 0) return applet_cp_mv(argc, argv, 0);
    if (strcmp(name, "mv") == 0) return applet_cp_mv(argc, argv, 1);
    if (strcmp(name, "date") == 0) return date_main(argc, argv);
    if (strcmp(name, "sleep") == 0) return sleep_main(argc, argv);
    if (strcmp(name, "sed") == 0) return sed_main(argc, argv);
    if (strcmp(name, "chmod") == 0) return chmod_main(argc, argv);
    if (strcmp(name, "awk") == 0) return awk_main(argc, argv);
    fputs("busybox: applet is unavailable\n", stderr);
    return 127;
}
