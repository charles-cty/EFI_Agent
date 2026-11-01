#ifndef EFI_BUSYBOX_TERMIOS_H
#define EFI_BUSYBOX_TERMIOS_H

#include <stdint.h>
typedef uint32_t tcflag_t;
typedef unsigned char cc_t;
struct termios {
    tcflag_t c_iflag, c_oflag, c_cflag, c_lflag;
    cc_t c_cc[32];
};
#define ICANON 1
#define ECHO 2
#define ISIG 4
#define OPOST 4
#define ONLCR 4
#define TCSANOW 0
#define TCSADRAIN 1
#define TCSAFLUSH 2
#define VMIN 0
#define VTIME 1
#define VERASE 2
int tcgetattr(int fd, struct termios *settings);
int tcsetattr(int fd, int action, const struct termios *settings);

#endif
