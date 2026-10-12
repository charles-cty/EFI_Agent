#include "efi_process.h"
#include <stdint.h>
#include <stdio.h>
#include <unistd.h>

#define CHECK(condition) do { if (!(condition)) __builtin_trap(); } while (0)

static void reject(const unsigned char *bytes, uint32_t size, const char *length)
{
    int32_t fd = efi_process_input_snapshot(bytes, size);
    CHECK(fd >= 4);
    const char *arguments[] = {"ash", "--efi-task", length, "4"};
    char scratch[128];
    int32_t packed = efi_process_pack_argv(arguments, 4, scratch, sizeof scratch);
    CHECK(packed > 0);
    struct efi_process_fd_mapping maps[] = {
        {0, 0}, {1, 1}, {2, 2}, {4, (uint32_t)fd}
    };
    int32_t pid = efi_process_spawn_mapped_fds_raw(scratch, packed, NULL, 0,
        maps, sizeof maps / sizeof maps[0]);
    CHECK(pid > 0);
    CHECK(close(fd) == 0);
    int32_t status = -1;
    CHECK(efi_process_waitpid(pid, &status) == pid);
    CHECK(status == 2);
}

int main(void)
{
    const unsigned char invalid_flags[] = {8, 0, 0, 0};
    const unsigned char invalid_version[] = {0, 0, 0, 0, 0, 0, 0, 0};
    const unsigned char truncated_state[] = {0, 0, 0, 0, 'A', 'S', 'T', '1'};
    reject(NULL, 0, "8");
    reject(invalid_flags, sizeof invalid_flags, "8");
    reject(invalid_flags, sizeof invalid_flags, "3");
    reject(invalid_flags, sizeof invalid_flags, "4");
    reject(invalid_version, sizeof invalid_version, "8");
    reject(truncated_state, sizeof truncated_state, "8");
    puts("ASH_TASK_BINARY_REJECTION_OK");
    return 0;
}
