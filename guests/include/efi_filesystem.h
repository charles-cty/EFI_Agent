#ifndef EFI_FILESYSTEM_H
#define EFI_FILESYSTEM_H

#include <sys/stat.h>

/* Preserve path components before libc can normalize the private cwd route. */
int efi_filesystem_fstatat(int fd, const char *path, struct stat *info, int flags);

#endif
