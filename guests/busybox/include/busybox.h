#ifndef EFI_BUSYBOX_H
#define EFI_BUSYBOX_H

/* WASI declarations for the upstream ash translation unit. The guest port
 * implements these utilities; it does not include Linux applet infrastructure.
 */
#include "ash_config.h"
#include "efi_process.h"
#include <ctype.h>
#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <inttypes.h>
#include <limits.h>
#include <libgen.h>
#include <setjmp.h>
#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>
#include <stdbool.h>
#include <signal.h>
#include <sys/wait.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <time.h>
#include <unistd.h>

#define FAST_FUNC
#define MAIN_EXTERNALLY_VISIBLE
#define UNUSED_PARAM __attribute__((unused))
#define NORETURN __attribute__((noreturn))
#define NOINLINE __attribute__((noinline))
#define ALWAYS_INLINE inline __attribute__((always_inline))
#define ALIGN1 __attribute__((aligned(1)))
#define ALIGN2 __attribute__((aligned(2)))
#define ALIGN_PTR __attribute__((aligned(sizeof(void *))))
#define PUSH_AND_SET_FUNCTION_VISIBILITY_TO_HIDDEN
#define POP_SAVED_FUNCTION_VISIBILITY
#define BB_MMU 1
#define BB_GLOBAL_CONST
#define MAX_HISTORY 0
#define ARRAY_SIZE(array) (sizeof(array) / sizeof((array)[0]))
#define ASSIGN_CONST_PTR(pointer, value) (*(pointer) = (value))
#define XZALLOC_CONST_PTR(pointer, size) (*(pointer) = xzalloc(size))
#define barrier() __asm__ volatile("" ::: "memory")
#define fdprintf dprintf
#define bb_unreachable(alternative) __builtin_unreachable()
#define LONE_DASH(text) ((text)[0] == '-' && (text)[1] == '\0')
#define NOT_LONE_DASH(text) (!(text) || !LONE_DASH(text))
#define LONE_CHAR(text, character) ((text)[0] == (character) && (text)[1] == '\0')
#define NOT_LONE_CHAR(text, character) (!(text) || !LONE_CHAR(text, character))
#define STRERROR_FMT "%s"
#define STRERROR_ERRNO , strerror(errno)
#define _exit_SUCCESS() _exit(0)
#define likely(value) __builtin_expect(!!(value), 1)
#define unlikely(value) __builtin_expect(!!(value), 0)

typedef signed char smallint;
typedef unsigned char smalluint;

extern const char bb_PATH_root_path[];
extern const char bb_msg_memory_exhausted[];
extern const char bb_msg_requires_arg[];
extern const char bb_msg_write_error[];
int test_main(int argc, char **argv);
int index_in_strings(const char *strings, const char *value);
gid_t *bb_getgroups(int *count, gid_t *groups);
void bb_simple_perror_msg(const char *text);
NORETURN void bb_simple_perror_msg_and_die(const char *text);
extern const char *applet_name;
extern const char *bb_busybox_exec_path;
extern int bb_got_signal;
extern char **environ;
extern const char bb_dev_null[];
int efi_ash_dupfd(int source, int minimum);
int efi_ash_dupfd_cloexec(int source, int minimum);
unsigned efi_ash_inherit_fds(struct efi_process_fd_mapping mappings[127]);
int efi_ash_input_snapshot(const void *bytes, size_t size);
union node;
/* Spawn a fresh ash instance from a bounded serialized node and shell state. */
int efi_ash_spawn_node(union node *node, int mode);
enum {
    EFI_TASK_CLEAR_ERREXIT = 1,
    EFI_TASK_TESTED = 2,
    EFI_TASK_REDIR_EXPANDED = 4,
};
int efi_ash_spawn_task(union node *node, uint32_t input, uint32_t output, unsigned flags);
int efi_ash_exec(const char *path, char *const arguments[], char *const environment[]);
int efi_ash_run(char *const arguments[], char *const environment[]);
struct passwd {
    char *pw_name;
    uid_t pw_uid;
    gid_t pw_gid;
    char *pw_dir;
};
struct group {
    char *gr_name;
    gid_t gr_gid;
    char **gr_mem;
};
struct passwd *getpwnam(const char *name);
struct passwd *getpwuid(uid_t uid);
struct group *getgrgid(gid_t gid);
mode_t umask(mode_t mask);
uid_t geteuid(void);
uid_t getuid(void);
gid_t getegid(void);
gid_t getgid(void);

void *xzalloc(size_t size);
void *xmalloc(size_t size);
void *xrealloc(void *pointer, size_t size);
void *xmemdup(const void *pointer, size_t size);
char *xstrdup(const char *text);
char *xasprintf(const char *format, ...) __attribute__((format(printf, 1, 2)));
void bb_error_msg(const char *format, ...) __attribute__((format(printf, 1, 2)));
NORETURN void bb_error_msg_and_die(const char *format, ...) __attribute__((format(printf, 1, 2)));
void bb_simple_error_msg(const char *text);
NORETURN void bb_simple_error_msg_and_die(const char *text);
ssize_t safe_read(int fd, void *bytes, size_t size);
ssize_t safe_write(int fd, const void *bytes, size_t size);
ssize_t full_write(int fd, const void *bytes, size_t size);
void xwrite(int fd, const void *bytes, size_t size);
void close_on_exec_on(int fd);
NORETURN void xfunc_die(void);
ssize_t nonblock_immune_read(int fd, void *bytes, size_t size);
char *itoa(int value);
struct suffix_mult {
    char suffix[4];
    uint32_t mult;
};
#define ALIGN_SUFFIX __attribute__((aligned(4)))
extern const struct suffix_mult bkm_suffixes[];
#include "xatonum.h"
int printf_main(int argc, char **argv);
int bb_putchar(int value);
char *skip_whitespace(const char *text);
char **skip_dash_dash(char **arguments);
void overlapping_strcpy(char *target, const char *source);
NORETURN void bb_show_usage(void);
unsigned bb_clk_tck(void);
int bb_parse_mode(const char *text, unsigned initial);
const char *get_signame(int number);
int get_signum(const char *name);
int fflush_all(void);
char *is_prefixed_with(const char *text, const char *prefix);
char *safe_strncpy(char *target, const char *source, size_t size);
const char *bb_basename(const char *path);
const char *endofname(const char *text);
char *concat_path_file(const char *path, const char *name);
char *xmalloc_realpath(const char *path);
char *xrealloc_getcwd_or_warn(char *buffer);
void sigprocmask_allsigs(int how);
void sigprocmask2(int how, sigset_t *mask);
void sigaction_set(int signal_number, const struct sigaction *action);
extern const char bb_banner[];
#define bb_default_path "/bin"
#define major(device) ((unsigned)(((device) >> 8) & 0xfff))
#define minor(device) ((unsigned)(((device) & 0xff) | (((device) >> 12) & 0xffffff00)))
#define DEV_FD_PREFIX "/dev/fd/"
#define getpid efi_process_getpid
#define getppid efi_process_getppid
char *utoa(unsigned value);
int echo_main(int argc, char **argv);
int efi_busybox_applet(int argc, char **argv);
char bb_process_escape_sequence(const char **text);

#endif
