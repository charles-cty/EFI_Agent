#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>
#include <wasi/api.h>
#include "efi_process.h"

static void require(int condition, const char *step)
{
    if (!condition) {
        fprintf(stderr, "filesystem %s: errno %d\n", step, errno);
        exit(99);
    }
}

int main(void)
{
    const char *name = "wasm-file.bin";
    int readonly = open("wasm-readonly.bin", O_CREAT | O_RDONLY, 0666);
    unsigned char empty;
    require(readonly >= 0 && read(readonly, &empty, 1) == 0 && close(readonly) == 0, "readonly create");
    readonly = open("wasm-readonly.bin", O_CREAT | O_RDONLY, 0666);
    require(readonly >= 0 && close(readonly) == 0, "readonly existing create");
    int fd = open(name, O_CREAT | O_TRUNC | O_RDWR, 0666);
    require(fd >= 0, "create");
    const unsigned char bytes[] = {'a', 0, 'b', 0xff, 'z'};
    require(write(fd, bytes, sizeof(bytes)) == sizeof(bytes), "write");
    require(lseek(fd, 1, SEEK_SET) == 1, "seek");
    unsigned char buffer[8] = {0};
    require(read(fd, buffer, 2) == 2 && buffer[0] == 0 && buffer[1] == 'b', "read");
    require(efi_process_dup2(fd, 40) == 40, "dup");
    require(read(40, buffer, 1) == 1 && buffer[0] == 0xff, "shared cursor");
    require(lseek(fd, 0, SEEK_CUR) == 4, "tell");
    int independent = open(name, O_RDONLY);
    require(independent >= 0, "second open");
    require(read(independent, buffer, 1) == 1 && buffer[0] == 'a', "independent cursor");
    require(close(independent) == 0, "close second");
    require(close(fd) == 0 && read(40, buffer, 1) == 1 && buffer[0] == 'z', "last duplicate");
    require(close(40) == 0, "close dup");

    fd = open(name, O_WRONLY | O_APPEND);
    require(fd >= 0 && lseek(fd, 0, SEEK_SET) == 0, "append open");
    require(__wasi_fd_fdstat_set_flags(fd, 0) == __WASI_ERRNO_NOTSUP, "append flag change rejected");
    require(write(fd, "!", 1) == 1 && fsync(fd) == 0, "append write");
    require(close(fd) == 0, "append close");
    struct stat metadata;
    require(stat(name, &metadata) == 0 && metadata.st_size == 6 && S_ISREG(metadata.st_mode), "stat");
    require(open(name, O_CREAT | O_EXCL | O_WRONLY, 0666) == -1 && errno == EEXIST, "exclusive");
    require(open("missing/../wasm-file.bin", O_RDONLY) == -1 && errno == ENOENT, "missing parent");
    require(open("wasm-file.bin/../wasm-file.bin", O_RDONLY) == -1 && errno == ENOTDIR, "file parent");
    require(stat("subdir/../wasm-file.bin", &metadata) == 0 && metadata.st_size == 6, "existing parent");
    int directory = open("subdir", O_RDONLY | O_DIRECTORY);
    require(directory >= 0, "open directory");
    int relative = openat(directory, "λ.txt", O_RDONLY);
    require(relative >= 0 && read(relative, buffer, 3) == 3 && memcmp(buffer, "xyz", 3) == 0, "directory relative Unicode file");
    require(close(relative) == 0 && close(directory) == 0, "directory close");
    directory = open("subdir", O_RDONLY);
    require(directory >= 0 && fstat(directory, &metadata) == 0 && S_ISDIR(metadata.st_mode), "implicit directory open");
    require(close(directory) == 0, "implicit directory close");

    __wasi_fd_t opened = 123;
    __wasi_errno_t result = __wasi_path_open(3, 0, name, __WASI_OFLAGS_TRUNC,
        __WASI_RIGHTS_FD_WRITE, 0, 0, (__wasi_fd_t *)(uintptr_t)0xfffffffc);
    require(result == __WASI_ERRNO_FAULT, "bad output pointer");
    require(stat(name, &metadata) == 0 && metadata.st_size == 6, "no truncate on fault");
    result = __wasi_path_open(3, 0, "../outside", 0, __WASI_RIGHTS_FD_READ, 0, 0, &opened);
    require(result == __WASI_ERRNO_NOTCAPABLE && opened == 123, "root traversal");
    require(efi_process_dupfd(127, 10) == -9, "dup bad source");
    require(efi_process_dupfd(1, 128) == -22, "dup invalid minimum");
    int duplicate = efi_process_dupfd(1, 60);
    require(duplicate == 60 && efi_process_dupfd(1, 60) == 61, "dup lowest free");
    require(close(60) == 0 && close(61) == 0, "dup cleanup");

    result = __wasi_path_open(3, 0, name, 0, __WASI_RIGHTS_FD_READ, 0, 0, &opened);
    require(result == 0, "reduced rights");
    __wasi_ciovec_t vector = {(const uint8_t *)"bad", 3};
    __wasi_size_t count = 999;
    require(__wasi_fd_write(opened, &vector, 1, &count) == __WASI_ERRNO_NOTCAPABLE && count == 999, "write rights");
    __wasi_filesize_t position = 999;
    require(__wasi_fd_seek(opened, 0, __WASI_WHENCE_SET, &position) == __WASI_ERRNO_NOTCAPABLE && position == 999, "seek rights");
    require(__wasi_fd_close(opened) == 0, "reduced rights close");
    for (int index = 4; index < 128; ++index) {
        require(efi_process_dupfd(1, 4) == index, "descriptor allocation boundary");
    }
    require(efi_process_dupfd(1, 4) == -24, "descriptor limit");
    opened = 123;
    result = __wasi_path_open(3, 0, name, __WASI_OFLAGS_TRUNC, __WASI_RIGHTS_FD_WRITE, 0, 0, &opened);
    require(result == __WASI_ERRNO_MFILE && opened == 123, "no open on full table");
    for (int index = 4; index < 128; ++index) {
        require(close(index) == 0, "descriptor release");
    }
    require(stat(name, &metadata) == 0 && metadata.st_size == 6, "no truncate on descriptor limit");

    fd = open("wasm-cursor.bin", O_CREAT | O_TRUNC | O_RDWR, 0666);
    require(fd >= 0, "cursor create");
    unsigned char oracle[257] = {0};
    require(write(fd, oracle, sizeof(oracle)) == sizeof(oracle), "cursor initialize");
    for (size_t index = 0; index < 97; ++index) {
        size_t offset = (index * 73 + 11) % sizeof(oracle);
        unsigned char value = (index * 19 + 7) % 256;
        require(lseek(fd, offset, SEEK_SET) == offset && write(fd, &value, 1) == 1, "cursor overwrite");
        oracle[offset] = value;
        require(lseek(fd, offset, SEEK_SET) == offset && read(fd, buffer, 1) == 1 && buffer[0] == value, "cursor verify");
    }
    require(lseek(fd, -257, SEEK_END) == 0, "negative end seek");
    unsigned char actual[257];
    require(read(fd, actual, sizeof(actual)) == sizeof(actual) && memcmp(actual, oracle, sizeof(actual)) == 0, "cursor oracle");
    require(lseek(fd, -1, SEEK_SET) == -1 && errno == EINVAL, "negative absolute seek");
    require(lseek(fd, 0, SEEK_CUR) == 257 && read(fd, buffer, 1) == 0, "failed seek preserves cursor and EOF");
    __wasi_subscription_t subscription = {0};
    subscription.userdata = 731;
    subscription.u.tag = __WASI_EVENTTYPE_FD_READ;
    subscription.u.u.fd_read.file_descriptor = fd;
    __wasi_event_t event;
    __wasi_size_t events = 0;
    require(__wasi_poll_oneoff(&subscription, &event, 1, &events) == 0 && events == 1
        && event.userdata == 731 && event.error == 0 && event.fd_readwrite.nbytes == 0, "EOF readiness");
    require(lseek(fd, 1, SEEK_SET) == 1, "readiness seek");
    require(__wasi_poll_oneoff(&subscription, &event, 1, &events) == 0 && events == 1
        && event.fd_readwrite.nbytes == 256, "readiness remaining bytes");
    require(close(fd) == 0, "cursor close");
    fd = open("wasm-resize.bin", O_CREAT | O_TRUNC | O_RDWR, 0666);
    require(fd >= 0 && write(fd, "abcdef", 6) == 6, "resize create");
    require(lseek(fd, 9, SEEK_SET) == 9 && ftruncate(fd, 3) == 0, "shrink beyond cursor");
    require(lseek(fd, 0, SEEK_CUR) == 9, "shrink retains cursor");
    require(read(fd, buffer, 1) == 0, "read beyond truncated EOF");
    require(lseek(fd, 0, SEEK_CUR) == 9, "EOF read retains cursor");
    require(efi_process_dup2(fd, 40) == 40 && ftruncate(40, 8195) == 0, "duplicate grow");
    require(lseek(fd, 0, SEEK_CUR) == 9, "grow retains shared cursor");
    require(fstat(fd, &metadata) == 0 && metadata.st_size == 8195, "grown metadata");
    require(lseek(fd, 0, SEEK_SET) == 0 && read(fd, buffer, 3) == 3 && memcmp(buffer, "abc", 3) == 0, "shrink prefix");
    unsigned char zeros[521];
    size_t remaining = 8192;
    while (remaining != 0) {
        size_t wanted = remaining < sizeof(zeros) ? remaining : sizeof(zeros);
        memset(zeros, 0xff, sizeof(zeros));
        require(read(fd, zeros, wanted) == (ssize_t)wanted, "extended bytes read");
        for (size_t index = 0; index < wanted; ++index) { require(zeros[index] == 0, "extended bytes zero"); }
        remaining -= wanted;
    }
    require(ftruncate(fd, -1) == -1 && errno == EINVAL, "negative length");
    require(__wasi_fd_filestat_set_size(fd, UINT64_MAX) == __WASI_ERRNO_FBIG, "oversized length");
    require(fstat(fd, &metadata) == 0 && metadata.st_size == 8195 && lseek(fd, 0, SEEK_CUR) == 8195,
        "invalid resize preserves state");
    require(__wasi_fd_filestat_set_size(127, 0) == __WASI_ERRNO_BADF, "resize bad descriptor");
    require(__wasi_fd_filestat_set_size(3, 0) == __WASI_ERRNO_ISDIR, "resize directory");
    require(__wasi_fd_filestat_set_size(1, 0) == __WASI_ERRNO_INVAL, "resize stream");
    require(__wasi_path_open(3, 0, "wasm-resize.bin", 0, __WASI_RIGHTS_FD_READ, 0, 0, &opened) == 0,
        "resize restricted open");
    require(__wasi_fd_filestat_set_size(opened, 0) == __WASI_ERRNO_NOTCAPABLE, "resize rights");
    require(close(opened) == 0 && close(40) == 0, "resize close duplicates");
    require(ftruncate(fd, 0) == 0 && ftruncate(fd, 7) == 0, "zero shrink and regrow");
    require(lseek(fd, 0, SEEK_SET) == 0 && read(fd, buffer, 7) == 7, "regrow read");
    for (size_t index = 0; index < 7; ++index) { require(buffer[index] == 0, "no old contents after regrow"); }
    require(fsync(fd) == 0 && close(fd) == 0, "resize sync close");
    fd = open("wasm-resize.bin", O_WRONLY | O_APPEND);
    require(fd >= 0 && lseek(fd, 2, SEEK_SET) == 2 && ftruncate(fd, 4) == 0
        && lseek(fd, 0, SEEK_CUR) == 2, "append resize retains cursor");
    require(write(fd, "!", 1) == 1 && close(fd) == 0, "append after resize");
    fd = open("wasm-resize-oracle.bin", O_CREAT | O_TRUNC | O_RDWR, 0666);
    require(fd >= 0, "resize oracle create");
    unsigned char state[9000] = {0};
    size_t length = 0;
    for (size_t index = 0; index < 71; ++index) {
        size_t next = (index * 997 + 13) % sizeof(state);
        if (next > length) { memset(state + length, 0, next - length); }
        off_t cursor = (index * 43 + 7) % 10001;
        require(lseek(fd, cursor, SEEK_SET) == cursor && ftruncate(fd, next) == 0
            && lseek(fd, 0, SEEK_CUR) == cursor, "resize oracle cursor");
        length = next;
        size_t offset = (index * 71 + 3) % length;
        unsigned char value = (index * 17 + 1) % 256;
        require(lseek(fd, offset, SEEK_SET) == (off_t)offset && write(fd, &value, 1) == 1, "resize oracle overwrite");
        state[offset] = value;
        unsigned char observed[9000];
        require(lseek(fd, 0, SEEK_SET) == 0 && read(fd, observed, length) == (ssize_t)length
            && memcmp(observed, state, length) == 0, "resize oracle contents");
    }
    require(fsync(fd) == 0 && close(fd) == 0, "resize oracle close");
    puts("WASM_FILESYSTEM_OK");
    return 0;
}
