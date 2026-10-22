#include "busybox.h"
#include "utility_support.h"

void xfstat(int fd, struct stat *metadata, const char *message)
{
    if (fstat(fd, metadata) < 0)
        bb_simple_perror_msg_and_die(message);
}

