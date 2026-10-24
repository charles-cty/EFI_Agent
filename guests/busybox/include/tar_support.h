#ifndef EFI_BUSYBOX_TAR_SUPPORT_H
#define EFI_BUSYBOX_TAR_SUPPORT_H

void efi_tar_transform(int fd, bool compress);
pid_t efi_tar_wait(int *status);
int setup_unzip_on_fd(int fd, int fail_if_not_compressed);

#endif
