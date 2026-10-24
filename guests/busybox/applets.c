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
int od_main(int argc, char **argv);
int hexdump_main(int argc, char **argv);
int baseNUM_main(int argc, char **argv);
int md5_sha1_sum_main(int argc, char **argv);
int realpath_main(int argc, char **argv);
int stat_main(int argc, char **argv);
int find_main(int argc, char **argv);
int which_main(int argc, char **argv);
int cp_main(int argc, char **argv);
int mv_main(int argc, char **argv);
int xargs_main(int argc, char **argv);
int gzip_main(int argc, char **argv);
int gunzip_main(int argc, char **argv);
int unzip_main(int argc, char **argv);
int du_main(int argc, char **argv);
int df_main(int argc, char **argv);
int ln_main(int argc, char **argv);
int tar_main(int argc, char **argv);
int ls_main(int argc, char **argv);
int mkdir_main(int argc, char **argv);
int rmdir_main(int argc, char **argv);
int rm_main(int argc, char **argv);
int touch_main(int argc, char **argv);
int sort_main(int argc, char **argv);
int uniq_main(int argc, char **argv);
int cut_main(int argc, char **argv);
int tr_main(int argc, char **argv);

int grep_main(int argc, char **argv);

int efi_busybox_applet(int argc, char **argv)
{
    const char *name = bb_basename(argv[0]);
    applet_name = name;
    xfunc_error_retval = EXIT_FAILURE;
    if (strcmp(name, "cat") == 0) return cat_main(argc, argv);
    if (strcmp(name, "echo") == 0) return echo_main(argc, argv);
    if (strcmp(name, "xargs") == 0) return xargs_main(argc, argv);
    if (strcmp(name, "gzip") == 0) return gzip_main(argc, argv);
    if (strcmp(name, "unzip") == 0) return unzip_main(argc, argv);
    if (strcmp(name, "du") == 0) return du_main(argc, argv);
    if (strcmp(name, "df") == 0) return df_main(argc, argv);
    if (strcmp(name, "ln") == 0) return ln_main(argc, argv);
    if (strcmp(name, "tar") == 0) return tar_main(argc, argv);
    if (strcmp(name, "gunzip") == 0 || strcmp(name, "zcat") == 0)
        return gunzip_main(argc, argv);
    if (strcmp(name, "head") == 0) return head_main(argc, argv);
    if (strcmp(name, "tail") == 0) return tail_main(argc, argv);
    if (strcmp(name, "wc") == 0) return wc_main(argc, argv);
    if (strcmp(name, "tee") == 0) return tee_main(argc, argv);
    if (strcmp(name, "ls") == 0) return ls_main(argc, argv);
    if (strcmp(name, "env") == 0) return env_main(argc, argv);
    if (strcmp(name, "yes") == 0) return yes_main(argc, argv);
    if (strcmp(name, "basename") == 0) return basename_main(argc, argv);
    if (strcmp(name, "dirname") == 0) return dirname_main(argc, argv);
    if (strcmp(name, "pwd") == 0) return pwd_main(argc, argv);
    if (strcmp(name, "printenv") == 0) return printenv_main(argc, argv);
    if (strcmp(name, "seq") == 0) return seq_main(argc, argv);
    if (strcmp(name, "true") == 0) return true_main(argc, argv);
    if (strcmp(name, "false") == 0) return false_main(argc, argv);
    if (strcmp(name, "od") == 0) return od_main(argc, argv);
    if (strcmp(name, "hexdump") == 0) return hexdump_main(argc, argv);
    if (strcmp(name, "base64") == 0) return baseNUM_main(argc, argv);
    if (strcmp(name, "sha256sum") == 0) return md5_sha1_sum_main(argc, argv);
    if (strcmp(name, "realpath") == 0) return realpath_main(argc, argv);
    if (strcmp(name, "stat") == 0) return stat_main(argc, argv);
    if (strcmp(name, "find") == 0) return find_main(argc, argv);
    if (strcmp(name, "which") == 0) return which_main(argc, argv);
    if (strcmp(name, "grep") == 0) return grep_main(argc, argv);
    if (strcmp(name, "mkdir") == 0) return mkdir_main(argc, argv);
    if (strcmp(name, "rmdir") == 0) return rmdir_main(argc, argv);
    if (strcmp(name, "touch") == 0) return touch_main(argc, argv);
    if (strcmp(name, "sort") == 0) return sort_main(argc, argv);
    if (strcmp(name, "uniq") == 0) return uniq_main(argc, argv);
    if (strcmp(name, "cut") == 0) return cut_main(argc, argv);
    if (strcmp(name, "tr") == 0) return tr_main(argc, argv);
    if (strcmp(name, "rm") == 0) return rm_main(argc, argv);
    if (strcmp(name, "cp") == 0) return cp_main(argc, argv);
    if (strcmp(name, "mv") == 0) return mv_main(argc, argv);
    if (strcmp(name, "date") == 0) return date_main(argc, argv);
    if (strcmp(name, "sleep") == 0) return sleep_main(argc, argv);
    if (strcmp(name, "sed") == 0) return sed_main(argc, argv);
    if (strcmp(name, "chmod") == 0) return chmod_main(argc, argv);
    if (strcmp(name, "awk") == 0) return awk_main(argc, argv);
    fputs("busybox: applet is unavailable\n", stderr);
    return 127;
}
