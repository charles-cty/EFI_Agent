#ifndef EFI_WASI_H
#define EFI_WASI_H

#include <stdint.h>

#define EFI_WASI_IMPORT(name) \
    __attribute__((import_module("wasi_snapshot_preview1"), import_name(name)))

struct efi_wasi_iovec {
    void *buffer;
    uint32_t size;
};

/* Explicit padding follows the WASI Preview 1 wire layout. */
struct efi_wasi_subscription {
    uint64_t userdata;
    uint8_t type;
    uint8_t padding[7];
    union {
        uint32_t fd;
        struct {
            uint32_t id;
            uint32_t padding;
            uint64_t timeout;
            uint64_t precision;
            uint16_t flags;
            uint8_t tail[6];
        } clock;
    } value;
};

struct efi_wasi_event {
    uint64_t userdata;
    uint16_t error;
    uint8_t type;
    uint8_t padding[5];
    uint64_t bytes;
    uint16_t flags;
    uint8_t tail[6];
};

_Static_assert(sizeof(struct efi_wasi_subscription) == 48, "WASI subscription layout");
_Static_assert(sizeof(struct efi_wasi_event) == 32, "WASI event layout");

EFI_WASI_IMPORT("fd_read") int32_t efi_wasi_read(uint32_t fd,
    const struct efi_wasi_iovec *vectors, uint32_t count, uint32_t *size);
EFI_WASI_IMPORT("fd_write") int32_t efi_wasi_write(uint32_t fd,
    const struct efi_wasi_iovec *vectors, uint32_t count, uint32_t *size);
EFI_WASI_IMPORT("fd_close") int32_t efi_wasi_close(uint32_t fd);
EFI_WASI_IMPORT("poll_oneoff") int32_t efi_wasi_poll(
    const struct efi_wasi_subscription *subscriptions, struct efi_wasi_event *events,
    uint32_t count, uint32_t *received);
EFI_WASI_IMPORT("sched_yield") int32_t efi_wasi_yield(void);
EFI_WASI_IMPORT("proc_exit") __attribute__((noreturn)) void efi_wasi_exit(int32_t status);

#endif
