#ifndef EFI_TIMEZONE_H
#define EFI_TIMEZONE_H

#include <stddef.h>

/* The upstream rules engine owns the returned, validated TZif file buffer. */
const unsigned char *efi_timezone_map(const char *path, size_t *size);
int efi_timezone_unmap(void *data, size_t size);
const char *efi_timezone_setting(const char *name);

#endif
