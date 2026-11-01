#ifndef EFI_BUSYBOX_TERMINAL_SUPPORT_H
#define EFI_BUSYBOX_TERMINAL_SUPPORT_H
#include <termios.h>
#include <poll.h>
#define CURRENT_TTY "/dev/tty"
#define TERMIOS_RAW_CRNL_INPUT 2
#define TERMIOS_RAW_CRNL_OUTPUT 4
#define TERMIOS_RAW_CRNL 6
enum {
	KEYCODE_UP        =  -2,
	KEYCODE_DOWN      =  -3,
	KEYCODE_RIGHT     =  -4,
	KEYCODE_LEFT      =  -5,
	KEYCODE_HOME      =  -6,
	KEYCODE_END       =  -7,
	KEYCODE_INSERT    =  -8,
	KEYCODE_DELETE    =  -9,
	KEYCODE_PAGEUP    = -10,
	KEYCODE_PAGEDOWN  = -11,
	KEYCODE_BACKSPACE = -12, /* Used only if Alt/Ctrl/Shifted */
	KEYCODE_D         = -13, /* Used only if Alted */
#if 0
	KEYCODE_FUN1      = ,
	KEYCODE_FUN2      = ,
	KEYCODE_FUN3      = ,
	KEYCODE_FUN4      = ,
	KEYCODE_FUN5      = ,
	KEYCODE_FUN6      = ,
	KEYCODE_FUN7      = ,
	KEYCODE_FUN8      = ,
	KEYCODE_FUN9      = ,
	KEYCODE_FUN10     = ,
	KEYCODE_FUN11     = ,
	KEYCODE_FUN12     = ,
#endif
	/* ^^^^^ Be sure that last defined value is small enough.
	 * Current read_key() code allows going up to -32 (0xfff..fffe0).
	 * This gives three upper bits in LSB to play with:
	 * KEYCODE_foo values are 0xfff..fffXX, lowest XX bits are: scavvvvv,
	 * s=0 if SHIFT, c=0 if CTRL, a=0 if ALT,
	 * vvvvv bits are the same for same key regardless of "shift bits".
	 */
	//KEYCODE_SHIFT_...   = KEYCODE_...   & ~0x80,
	KEYCODE_CTRL_RIGHT    = KEYCODE_RIGHT & ~0x40,
	KEYCODE_CTRL_LEFT     = KEYCODE_LEFT  & ~0x40,
	KEYCODE_ALT_RIGHT     = KEYCODE_RIGHT & ~0x20,
	KEYCODE_ALT_LEFT      = KEYCODE_LEFT  & ~0x20,
	KEYCODE_ALT_BACKSPACE = KEYCODE_BACKSPACE & ~0x20,
	KEYCODE_ALT_D         = KEYCODE_D     & ~0x20,

	KEYCODE_CURSOR_POS = -0x100, /* 0xfff..fff00 */
	/* How long is the longest ESC sequence we know?
	 * We want it big enough to be able to contain
	 * cursor position sequence "ESC [ 9999 ; 9999 R"
	 */
	KEYCODE_BUFFER_SIZE = 16
};
int64_t read_key(int fd, char *buffer, int timeout);
int64_t safe_read_key(int fd, char *buffer, int timeout);
void read_key_ungets(char *buffer, const char *text, unsigned length);
int safe_poll(struct pollfd *descriptors, nfds_t count, int timeout);
int tcsetattr_stdin_TCSANOW(const struct termios *settings);
int get_termios_and_make_raw(int fd, struct termios *new_settings, struct termios *old_settings, int flags);
int set_termios_to_raw(int fd, struct termios *old_settings, int flags);
int get_terminal_width_height(int fd, unsigned *width, unsigned *height);
char *xmalloc_ttyname(int fd);
int ndelay_on(int fd);
int ndelay_off(int fd);
void bb_putchar_stderr(char byte);
FILE *fopen_for_write(const char *path);
void *xmalloc_open_read_close(const char *path, size_t *size);
char *xmalloc_read(int fd, size_t *size);
#define _exit_FAILURE() _exit(EXIT_FAILURE)
#endif
