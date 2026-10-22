#ifndef EFI_BUSYBOX_COMMON_BUFSIZ_H
#define EFI_BUSYBOX_COMMON_BUFSIZ_H

#define COMMON_BUFSIZE 1024
extern char bb_common_bufsiz1[COMMON_BUFSIZE] __attribute__((aligned(16)));
void setup_common_bufsiz(void);

#endif
