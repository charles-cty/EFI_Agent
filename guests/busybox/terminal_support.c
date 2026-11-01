#include "libbb.h"

int efi_bb_system(const char *command);
int system(const char *command)
{
    return efi_bb_system(command);
}

static int terminal_error(int result)
{
    if (result >= 0)
        return result;
    switch (result) {
    case -9: errno = EBADF; break;
    case -14: errno = EFAULT; break;
    case -22: errno = EINVAL; break;
    case -25: errno = ENOTTY; break;
    default: __builtin_trap();
    }
    return -1;
}

int tcgetattr(int fd, struct termios *settings)
{
    uint32_t state[3];
    if (terminal_error(efi_process_terminal_get(fd, state)) < 0)
        return -1;
    memset(settings, 0, sizeof(*settings));
    settings->c_lflag = state[0] & 3;
    settings->c_oflag = state[0] & 4;
    settings->c_cc[VMIN] = 1;
    settings->c_cc[VERASE] = 127;
    return 0;
}

int tcsetattr(int fd, int action, const struct termios *settings)
{
    if (action != TCSANOW && action != TCSADRAIN) {
        errno = ENOTSUP;
        return -1;
    }
    if (settings->c_iflag || settings->c_cflag
        || (settings->c_lflag & ~(ICANON | ECHO))
        || (settings->c_oflag & ~ONLCR)
        || settings->c_cc[VMIN] != 1 || settings->c_cc[VTIME] != 0) {
        errno = ENOTSUP;
        return -1;
    }
    return terminal_error(efi_process_terminal_set(fd, settings->c_lflag | settings->c_oflag));
}

int tcsetattr_stdin_TCSANOW(const struct termios *settings)
{
    return tcsetattr(STDIN_FILENO, TCSANOW, settings);
}

int get_termios_and_make_raw(int fd, struct termios *new_settings,
    struct termios *old_settings, int flags)
{
    if (tcgetattr(fd, old_settings) < 0)
        return -1;
    *new_settings = *old_settings;
    new_settings->c_lflag &= ~(ICANON | ECHO);
    if (flags & TERMIOS_RAW_CRNL_OUTPUT)
        new_settings->c_oflag = 0;
    return 0;
}

int set_termios_to_raw(int fd, struct termios *old_settings, int flags)
{
    struct termios raw;
    if (get_termios_and_make_raw(fd, &raw, old_settings, flags) < 0)
        bb_simple_perror_msg_and_die("terminal attributes");
    if (tcsetattr(fd, TCSANOW, &raw) < 0)
        bb_simple_perror_msg_and_die("terminal raw mode");
    return 0;
}

int get_terminal_width_height(int fd, unsigned *width, unsigned *height)
{
    uint32_t state[3];
    int result = efi_process_terminal_get(fd, state);
    if (width) *width = result == 0 ? state[1] : 80;
    if (height) *height = result == 0 ? state[2] : 24;
    return result == 0 ? 0 : -1;
}

char *xmalloc_ttyname(int fd)
{
    return isatty(fd) ? xstrdup("/dev/tty") : NULL;
}

int ndelay_on(int fd)
{
    int flags = fcntl(fd, F_GETFL);
    if (flags < 0 || fcntl(fd, F_SETFL, flags | O_NONBLOCK) < 0)
        bb_simple_perror_msg_and_die("terminal nonblocking mode");
    return flags;
}

int ndelay_off(int fd)
{
    int flags = fcntl(fd, F_GETFL);
    if (flags < 0 || fcntl(fd, F_SETFL, flags & ~O_NONBLOCK) < 0)
        bb_simple_perror_msg_and_die("terminal blocking mode");
    return flags;
}

void bb_putchar_stderr(char byte)
{
    fputc(byte, stderr);
}

int safe_poll(struct pollfd *descriptors, nfds_t count, int timeout)
{
    int result;
    do { result = poll(descriptors, count, timeout); } while (result < 0 && errno == EINTR);
    return result;
}
