/* Exercise the same libc wrappers linked into the BusyBox multicall image. */
#include "libbb.h"
#include <assert.h>

void efi_busybox_restore_cwd(void);

void *xmalloc(size_t size)
{
    void *result = malloc(size);
    assert(result);
    return result;
}

char *xasprintf(const char *format, ...)
{
    va_list arguments;
    va_start(arguments, format);
    char *result;
    int length = vasprintf(&result, format, arguments);
    va_end(arguments);
    assert(length >= 0);
    return result;
}

void bb_simple_error_msg_and_die(const char *text)
{
    fprintf(stderr, "%s\n", text);
    exit(1);
}

void bb_simple_perror_msg_and_die(const char *text)
{
    perror(text);
    exit(1);
}

int main(void)
{
    efi_busybox_restore_cwd();
    assert(mkdir("c-metadata", 0700) == 0);
    int root = open("c-metadata/root", O_CREAT | O_WRONLY, 0600);
    assert(root >= 0);
    assert(close(root) == 0);
    assert(mkdir("c-metadata/sub", 0700) == 0);
    assert(chdir("c-metadata/sub") == 0);
    const char *expected_cwd = "/c-metadata/sub";
    size_t cwd_size = strlen(expected_cwd) + 1;
    char cwd_buffer[64];
    assert(getcwd(cwd_buffer, cwd_size) == cwd_buffer);
    assert(strcmp(cwd_buffer, expected_cwd) == 0);
    errno = 0;
    assert(getcwd(cwd_buffer, cwd_size - 1) == NULL && errno == ERANGE);
    errno = 0;
    assert(getcwd(cwd_buffer, 0) == NULL && errno == EINVAL);
    char *allocated_cwd = getcwd(NULL, 0);
    assert(allocated_cwd && strcmp(allocated_cwd, expected_cwd) == 0);
    free(allocated_cwd);
    allocated_cwd = getcwd(NULL, cwd_size);
    assert(allocated_cwd && strcmp(allocated_cwd, expected_cwd) == 0);
    free(allocated_cwd);
    int local = open("local", O_CREAT | O_WRONLY, 0640);
    assert(local >= 0);
    struct stat info;
    assert(stat("../root", &info) == 0 && (info.st_mode & 07777) == 0600);
    assert(fstatat(3, "c-metadata/root", &info, 0) == 0);
    assert((info.st_mode & 07777) == 0600 && info.st_uid == 1 && info.st_gid == 1);
    errno = 0;
    assert(fstatat(3, "local", &info, 0) < 0 && errno == ENOENT);
    int directory = open(".", O_RDONLY | O_DIRECTORY);
    assert(directory >= 0);
    assert(fstatat(directory, "local", &info, 0) == 0 && (info.st_mode & 07777) == 0640);
    assert(chmod("local", 0440) == 0);
    assert(fstat(local, &info) == 0 && (info.st_mode & 07777) == 0440);
    assert(fchmod(local, 0600) == 0);
    assert(fstatat(directory, "local", &info, 0) == 0 && (info.st_mode & 07777) == 0600);
    assert(close(directory) == 0 && close(local) == 0);
    assert(symlink("created-target", "dangling") == 0);
    int target = open("dangling", O_CREAT | O_WRONLY, 0400);
    assert(target >= 0);
    assert(close(target) == 0);
    assert(stat("created-target", &info) == 0 && (info.st_mode & 07777) == 0400);
    target = open("dangling", O_CREAT | O_RDONLY, 0777);
    assert(target >= 0 && close(target) == 0);
    assert(stat("created-target", &info) == 0 && (info.st_mode & 07777) == 0400);
    puts("C_METADATA_ANCHORS_OK");
    return 0;
}
