#include "efi_shell.h"
#include "efi_wasi.h"

static char output[2][65537];
static char errors[2][70003];

static void require(int success)
{
    if (!success) {
        efi_wasi_exit(99);
    }
}

static void check(const char *bytes, uint32_t count, uint32_t modulus)
{
    for (uint32_t index = 0; index < count; ++index) {
        require((uint8_t)bytes[index] == index % modulus);
    }
}

void _start(void)
{
    char scratch[32];
    const char *arguments[] = {"dual"};
    struct efi_shell_task tasks[2];
    struct efi_shell_task *group[] = {&tasks[0], &tasks[1]};
    for (uint32_t index = 0; index < 2; ++index) {
        require(efi_shell_start(&tasks[index], arguments, 1, scratch, sizeof scratch,
            output[index], sizeof output[index], errors[index], sizeof errors[index]) == 0);
    }
    while (!efi_shell_done(&tasks[0]) || !efi_shell_done(&tasks[1])) {
        require(efi_shell_step(group, 2, 1) == 0);
    }
    for (uint32_t index = 0; index < 2; ++index) {
        require(tasks[index].error == 0 && tasks[index].status == 23);
        require(tasks[index].output[0].size == sizeof output[index]);
        require(tasks[index].output[1].size == sizeof errors[index]);
        check(output[index], sizeof output[index], 251);
        check(errors[index], sizeof errors[index], 239);
    }

    /* One byte below the stdout boundary must flag overflow, still consume
     * both streams, wait for the child, and release its process slot. */
    require(efi_shell_start(&tasks[0], arguments, 1, scratch, sizeof scratch,
        output[0], sizeof output[0] - 1, errors[0], sizeof errors[0]) == 0);
    while (!efi_shell_done(&tasks[0])) {
        require(efi_shell_step(group, 1, 0) == 0);
    }
    require(tasks[0].error == -27 && tasks[0].status == 23);
    require(tasks[0].output[0].size == sizeof output[0] - 1);
    require(tasks[0].output[1].size == sizeof errors[0]);

    const char *missing[] = {"missing"};
    require(efi_shell_start(&tasks[0], missing, 1, scratch, sizeof scratch,
        output[0], 1, errors[0], 1) == -2);
    require(efi_shell_done(&tasks[0]));
    static char message[] = "MULTI_CHILD_SHELL_CAPTURE_OK";
    struct efi_wasi_iovec vector = {message, sizeof message - 1};
    uint32_t written;
    require(efi_wasi_write(1, &vector, 1, &written) == 0);
    require(written == sizeof message - 1);
}
