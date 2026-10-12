#include "efi_process.h"
#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <wasi/api.h>

static void require(int condition, const char *step)
{
    if (!condition) {
        fprintf(stderr, "directory %s: errno %d\n", step, errno);
        exit(99);
    }
}

static void record(FILE *output, unsigned kind, const unsigned char *name, size_t length)
{
    require(fprintf(output, "%u:", kind) > 0, "record type");
    for (size_t index = 0; index < length; ++index) {
        require(fprintf(output, "%02x", name[index]) == 2, "record name");
    }
    require(fputc('\n', output) == '\n', "record newline");
}

static void raw_listing(int fd, uint64_t cookie, FILE *output, int forbid_added)
{
    unsigned char buffer[512];
    size_t capacity = 23;
    size_t completed = 0;
    for (;;) {
        __wasi_size_t used = 999;
        memset(buffer, 0xa5, sizeof(buffer));
        require(__wasi_fd_readdir(fd, buffer, capacity, cookie, &used) == 0, "raw read");
        require(used <= capacity, "raw count");
        for (size_t index = used; index < sizeof(buffer); ++index) {
            require(buffer[index] == 0xa5, "raw buffer tail");
        }
        if (used == 0) {
            break;
        }
        size_t offset = 0;
        size_t before = completed;
        while (used - offset >= sizeof(__wasi_dirent_t)) {
            __wasi_dirent_t header;
            memcpy(&header, buffer + offset, sizeof(header));
            offset += sizeof(header);
            if (header.d_namlen > used - offset) {
                break;
            }
            require(header.d_ino == 0 && header.d_namlen != 0, "raw header");
            require(header.d_next != cookie, "cookie progress");
            if (forbid_added) {
                require(header.d_namlen != 9 || memcmp(buffer + offset, "added.txt", 9) != 0, "stable iteration");
            }
            record(output, header.d_type, buffer + offset, header.d_namlen);
            offset += header.d_namlen;
            cookie = header.d_next;
            ++completed;
        }
        if (completed == before) {
            require(capacity < sizeof(buffer), "fixture name fits buffer");
            capacity = capacity * 2 < sizeof(buffer) ? capacity * 2 : sizeof(buffer);
        } else {
            capacity = 11 + completed * 17 % 113;
        }
    }
    require(completed > 0, "nonempty listing");
}

int main(void)
{
    int fd = open("directory-fixture", O_RDONLY | O_DIRECTORY);
    require(fd >= 0, "open");
    unsigned char buffer[64];
    __wasi_size_t used = 999;
    memset(buffer, 0xa5, sizeof(buffer));
    require(__wasi_fd_readdir(fd, buffer, sizeof(buffer), 1, &used) == __WASI_ERRNO_INVAL
        && used == 999, "unissued cookie");
    require(__wasi_fd_readdir(fd, buffer, sizeof(buffer), 0,
        (__wasi_size_t *)(uintptr_t)0xfffffffc) == __WASI_ERRNO_FAULT, "bad count pointer");
    require(buffer[0] == 0xa5, "fault does not write buffer");
    require(__wasi_fd_readdir(fd, buffer, sizeof(buffer), 1, &used) == __WASI_ERRNO_INVAL,
        "fault does not start iteration");
    require(__wasi_fd_readdir(fd, buffer, 0, 0, &used) == 0 && used == 0, "empty buffer");
    require(__wasi_fd_readdir(fd, buffer, 23, 0, &used) == 0 && used == 23, "partial header");
    require(buffer[23] == 0xa5, "partial header tail");
    require(__wasi_fd_readdir(fd, buffer, 24, 0, &used) == 0 && used == 24, "header boundary");
    __wasi_dirent_t first;
    memcpy(&first, buffer, sizeof(first));
    require(first.d_namlen == 1 && first.d_type == __WASI_FILETYPE_DIRECTORY, "dot header");
    require(__wasi_fd_readdir(fd, buffer, 25, 0, &used) == 0 && used == 25 && buffer[24] == '.', "name boundary");
    memcpy(&first, buffer, sizeof(first));
    require(__wasi_fd_readdir(fd, buffer, sizeof(buffer), UINT64_MAX, &used) == __WASI_ERRNO_INVAL,
        "invalid cookie");
    require(__wasi_fd_readdir(127, buffer, sizeof(buffer), 0, &used) == __WASI_ERRNO_BADF, "bad descriptor");
    require(__wasi_fd_readdir(1, buffer, sizeof(buffer), 0, &used) == __WASI_ERRNO_NOTDIR, "stream descriptor");

    __wasi_fd_t restricted;
    require(__wasi_path_open(3, 0, "directory-fixture", __WASI_OFLAGS_DIRECTORY,
        __WASI_RIGHTS_FD_FILESTAT_GET, 0, 0, &restricted) == 0, "restricted open");
    require(__wasi_fd_readdir(restricted, buffer, sizeof(buffer), 0, &used) == __WASI_ERRNO_NOTCAPABLE,
        "directory rights");
    require(close(restricted) == 0, "restricted close");
    require(efi_process_dup2(fd, 40) == 40 && close(fd) == 0, "duplicate retains iteration");
    int created = open("directory-fixture/added.txt", O_CREAT | O_EXCL | O_RDONLY, 0666);
    require(created >= 0 && close(created) == 0, "create during iteration");
    FILE *output = fopen("directory-list.txt", "w");
    require(output != NULL, "old listing output");
    record(output, first.d_type, (const unsigned char *)".", 1);
    raw_listing(40, first.d_next, output, 1);
    require(fclose(output) == 0, "old listing close");
    output = fopen("directory-current.txt", "w");
    require(output != NULL, "new listing output");
    raw_listing(40, 0, output, 0);
    require(fclose(output) == 0 && close(40) == 0, "new listing close");

    DIR *directory = opendir("directory-fixture");
    require(directory != NULL, "libc open");
    struct dirent *entry = readdir(directory);
    require(entry != NULL, "libc first");
    long cookie = telldir(directory);
    require(cookie >= 0, "libc tell");
    entry = readdir(directory);
    require(entry != NULL, "libc second");
    char next_name[256];
    strcpy(next_name, entry->d_name);
    seekdir(directory, cookie);
    entry = readdir(directory);
    require(entry != NULL && strcmp(entry->d_name, next_name) == 0, "libc seek");
    rewinddir(directory);
    output = fopen("directory-libc.txt", "w");
    require(output != NULL, "libc listing output");
    errno = 0;
    while ((entry = readdir(directory)) != NULL) {
        unsigned kind = entry->d_type == DT_DIR ? __WASI_FILETYPE_DIRECTORY
            : entry->d_type == DT_REG ? __WASI_FILETYPE_REGULAR_FILE
            : entry->d_type == DT_LNK ? __WASI_FILETYPE_SYMBOLIC_LINK
            : __WASI_FILETYPE_UNKNOWN;
        record(output, kind, (const unsigned char *)entry->d_name, strlen(entry->d_name));
    }
    require(errno == 0, "libc EOF");
    require(fclose(output) == 0 && closedir(directory) == 0, "libc close");
    directory = opendir("directory-fixture/empty");
    require(directory != NULL, "empty open");
    size_t count = 0;
    errno = 0;
    while ((entry = readdir(directory)) != NULL) {
        require(strcmp(entry->d_name, ".") == 0 || strcmp(entry->d_name, "..") == 0, "empty names");
        ++count;
    }
    require(count == 2 && errno == 0 && closedir(directory) == 0, "empty EOF");
    puts("WASM_DIRECTORY_OK");
    return 0;
}
