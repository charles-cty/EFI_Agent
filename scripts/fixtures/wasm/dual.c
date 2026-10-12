#include "efi_wasi.h"

static void emit(uint32_t fd, uint32_t count, uint32_t modulus)
{
    char bytes[4096];
    uint32_t position = 0;
    while (position < count) {
        uint32_t size = count - position;
        if (size > sizeof bytes) {
            size = sizeof bytes;
        }
        for (uint32_t index = 0; index < size; ++index) {
            bytes[index] = (char)((position + index) % modulus);
        }
        uint32_t sent = 0;
        while (sent < size) {
            struct efi_wasi_iovec vector = {bytes + sent, size - sent};
            uint32_t written;
            if (efi_wasi_write(fd, &vector, 1, &written) != 0 || written == 0) {
                efi_wasi_exit(99);
            }
            sent += written;
        }
        position += size;
    }
}

void _start(void)
{
    /* stderr exceeds pipe capacity before stdout starts. Waiting on stdout
     * alone would deadlock. Payloads include NUL and non-ASCII bytes. */
    emit(2, 70003, 239);
    emit(1, 65537, 251);
    efi_wasi_exit(23);
}
