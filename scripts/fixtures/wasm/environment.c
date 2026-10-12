#include "efi_process.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void require(int condition)
{
    if (!condition) {
        exit(99);
    }
}

static void wait_child(int32_t pid)
{
    int32_t status;
    require(pid > 0);
    require(efi_process_waitpid((uint32_t)pid, &status) == pid);
    require(status == 0);
}

int main(int argc, char **argv)
{
    char scratch[256];
    if (argc == 2 && strcmp(argv[1], "child") == 0) {
        require(strcmp(getenv("EMPTY"), "") == 0);
        require(strcmp(getenv("VALUE"), "a=b") == 0);
        require(strcmp(getenv("UNICODE"), "λ") == 0);
        const char *cleared[] = {"environment", "clear"};
        wait_child(efi_process_spawn_argv_env_fds(cleared, 2, NULL, 0,
            scratch, sizeof scratch, 0, 1, 2));
        require(strcmp(getenv("VALUE"), "a=b") == 0);
        const char *arguments[] = {"environment", "grandchild"};
        wait_child(efi_process_spawn_argv(arguments, 2, scratch, sizeof scratch));
        return 0;
    }
    if (argc == 2 && strcmp(argv[1], "grandchild") == 0) {
        require(strcmp(getenv("EMPTY"), "") == 0);
        require(strcmp(getenv("VALUE"), "a=b") == 0);
        require(strcmp(getenv("UNICODE"), "λ") == 0);
        return 0;
    }
    if (argc == 2 && strcmp(argv[1], "clear") == 0) {
        require(getenv("EMPTY") == NULL && getenv("VALUE") == NULL);
        return 0;
    }
    require(argc == 1 && getenv("VALUE") == NULL);
    const char *arguments[] = {"environment", "child"};
    const char *environment[] = {"EMPTY=", "VALUE=a=b", "UNICODE=λ"};
    /* 18 bytes of argv + 28 environment bytes: both sides of capacity. */
    require(efi_process_spawn_argv_env_fds(arguments, 2, environment, 3,
        scratch, 45, 0, 1, 2) == -7);
    wait_child(efi_process_spawn_argv_env_fds(arguments, 2, environment, 3,
        scratch, 46, 0, 1, 2));
    require(getenv("VALUE") == NULL); /* No mutation of the parent. */

    const char *duplicate[] = {"DUP=1", "DUP=2"};
    require(efi_process_spawn_argv_env_fds(arguments, 2, duplicate, 2,
        scratch, sizeof scratch, 0, 1, 2) == -22);
    const char *invalid[] = {"=value"};
    require(efi_process_spawn_argv_env_fds(arguments, 2, invalid, 1,
        scratch, sizeof scratch, 0, 1, 2) == -22);
    const char *malformed[] = {"MISSING_EQUALS"};
    require(efi_process_spawn_argv_env_fds(arguments, 2, malformed, 1,
        scratch, sizeof scratch, 0, 1, 2) == -22);

    arguments[1] = "clear";
    wait_child(efi_process_spawn_argv_env_fds(arguments, 2, NULL, 0,
        scratch, sizeof scratch, 0, 1, 2));
    puts("PROCESS_ENVIRONMENT_OK");
    return 0;
}
