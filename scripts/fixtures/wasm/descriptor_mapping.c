#include "efi_process.h"
#include <stdint.h>
#include <stdio.h>
#include <fcntl.h>
#include <string.h>
#include <unistd.h>
#include <wasi/api.h>

#define CHECK(condition) do { if (!(condition)) __builtin_trap(); } while (0)

int main(int argc, char **argv)
{
    (void)argv;
    const unsigned char expected[] = {'a', 0, 255, 'Z', 17, 'q'};
    unsigned char bytes[sizeof expected];
    if (argc == 3) {
        for (unsigned target = 0; target < 128; ++target) {
            __wasi_fdstat_t stat;
            CHECK(__wasi_fd_fdstat_get(target, &stat) == 0);
            if (target != 3)
                CHECK(stat.fs_flags == 0);
        }
        return 0;
    }
    if (argc == 2) {
        CHECK(efi_process_get_fd_flags(4) == 0);
        CHECK(efi_process_get_fd_flags(127) == 0);
        __wasi_fdstat_t stat;
        CHECK(__wasi_fd_fdstat_get(3, &stat) == 0);
        CHECK(stat.fs_filetype == __WASI_FILETYPE_DIRECTORY);
        CHECK(__wasi_fd_fdstat_get(0, &stat) == __WASI_ERRNO_BADF);
        CHECK(read(4, bytes, 2) == 2);
        CHECK(close(4) == 0);
        CHECK(read(127, bytes + 2, 4) == 4);
        CHECK(memcmp(bytes, expected, sizeof bytes) == 0);
        CHECK(read(127, bytes, 1) == 0);
        CHECK(close(127) == 0);
        return 0;
    }
    CHECK(argc == 1);
    int32_t fd = efi_process_input_snapshot(expected, sizeof expected);
    CHECK(fd >= 4);
    CHECK(efi_process_get_fd_flags(fd) == 0);
    CHECK(efi_process_set_fd_flags(fd, 1) == 0);
    CHECK(efi_process_get_fd_flags(fd) == 1);
    CHECK(efi_process_set_fd_flags(fd, 2) == -22);
    CHECK(efi_process_get_fd_flags(fd) == 1);
    const char arguments[] = "descriptor_mapping\0child";
    struct efi_process_fd_mapping maps[] = {
        {1, 1}, {2, 2}, {4, (uint32_t)fd}, {127, (uint32_t)fd}
    };
    struct efi_process_fd_mapping invalid = {3, (uint32_t)fd};
    CHECK(efi_process_spawn_mapped_fds_raw(arguments, sizeof arguments,
        NULL, 0, &invalid, 1) == -22);
    int32_t pid = efi_process_spawn_mapped_fds_raw(arguments, sizeof arguments,
        NULL, 0, maps, sizeof maps / sizeof maps[0]);
    CHECK(pid > 0);
    int32_t status = -1;
    CHECK(efi_process_waitpid(pid, &status) == pid);
    CHECK(status == 0);
    CHECK(efi_process_get_fd_flags(fd) == 1);
    CHECK(read(fd, bytes, 1) == 0);
    CHECK(close(fd) == 0);
    CHECK(efi_process_get_fd_flags(fd) == -9);
    CHECK(efi_process_set_fd_flags(fd, 0) == -9);
    int32_t reused = efi_process_input_snapshot(NULL, 0);
    CHECK(reused == fd);
    CHECK(efi_process_get_fd_flags(reused) == 0);
    CHECK(close(reused) == 0);

    int file = open("descriptor-flags.txt", O_CREAT | O_RDWR | O_TRUNC, 0600);
    CHECK(file >= 4);
    CHECK(efi_process_set_fd_flags(file, 1) == 0);
    CHECK(efi_process_dup2(file, file) == file);
    CHECK(efi_process_get_fd_flags(file) == 1);
    CHECK(efi_process_dup2(file, 127) == 127);
    CHECK(efi_process_get_fd_flags(127) == 0);
    CHECK(efi_process_set_fd_flags(127, 1) == 0);
    CHECK(__wasi_fd_renumber(127, 126) == 0);
    CHECK(efi_process_get_fd_flags(126) == 1);
    CHECK(efi_process_get_fd_flags(127) == -9);
    CHECK(efi_process_dup2(file, 126) == 126);
    CHECK(efi_process_get_fd_flags(126) == 0);
    CHECK(close(file) == 0);
    CHECK(close(126) == 0);
    CHECK(efi_process_set_fd_flags(1, 1) == 0);
    CHECK(efi_process_get_fd_flags(1) == 1);
    CHECK(efi_process_set_fd_flags(1, 0) == 0);
    uint32_t pipe_fds[2];
    CHECK(efi_process_pipe(pipe_fds) == 0);
    CHECK(efi_process_set_fd_flags(pipe_fds[0], 1) == 0);
    CHECK(efi_process_get_fd_flags(pipe_fds[0]) == 1);
    CHECK(efi_process_get_fd_flags(pipe_fds[1]) == 0);
    CHECK(close(pipe_fds[0]) == 0);
    CHECK(close(pipe_fds[1]) == 0);

    /* Every permitted target, including 127, fits in the map. All entries
     * explicitly duplicate one source; none consumes another parent's slot.
     */
    struct efi_process_fd_mapping full[127];
    unsigned count = 0;
    for (unsigned target = 0; target < 128; ++target) {
        if (target != 3)
            full[count++] = (struct efi_process_fd_mapping){target, 1};
    }
    CHECK(count == 127);
    const char full_arguments[] = "descriptor_mapping\0full\0map";
    pid = efi_process_spawn_mapped_fds_raw(full_arguments, sizeof full_arguments,
        NULL, 0, full, count);
    CHECK(pid > 0);
    CHECK(efi_process_waitpid(pid, &status) == pid);
    CHECK(status == 0);
    puts("UEFI_DESCRIPTOR_MAPPING_OK");
    return 0;
}
