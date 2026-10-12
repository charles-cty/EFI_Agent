#include "efi_process.h"
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int efi_ash_exec(const char *path, char *const arguments[], char *const environment[]);

static void require(int condition)
{
    if (!condition) {
        exit(99);
    }
}

int main(int argc, char **argv)
{
    if (argc == 2 && strcmp(argv[1], "child") == 0) {
        require(strcmp(getenv("VALUE"), "a=b") == 0);
        require(strcmp(getenv("EMPTY"), "") == 0);
        require(efi_process_getppid() > 0);
        puts("ASH_EXEC_FRESH_INSTANCE_OK");
        return 37;
    }
    char *environment[] = {"VALUE=a=b", "EMPTY=", NULL};
    char *missing[] = {"missing", NULL};
    require(efi_ash_exec("missing", missing, environment) == -1 && errno == ENOENT);
    char *invalid[] = {"DUP=1", "DUP=2", NULL};
    char *arguments[] = {"ash_exec", "child", NULL};
    require(efi_ash_exec("ash_exec", arguments, invalid) == -1 && errno == EINVAL);
    require(argc == 1);
    efi_ash_exec("ash_exec", arguments, environment);
    return 98;
}
