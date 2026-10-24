#ifndef EFI_BUSYBOX_STATFS_H
#define EFI_BUSYBOX_STATFS_H

#include <stdint.h>

/* Guest-only layout. All counters retain the private statistics ABI width. */
struct statfs {
    unsigned long f_type;
    unsigned long f_bsize;
    uint64_t f_blocks;
    uint64_t f_bfree;
    uint64_t f_bavail;
    uint64_t f_files;
    uint64_t f_ffree;
    unsigned f_fsid[2];
    unsigned long f_namelen;
    unsigned long f_frsize;
    unsigned long f_flags;
};

int statfs(const char *path, struct statfs *info);

#endif
