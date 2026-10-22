#ifndef EFI_BUSYBOX_UTILITY_SUPPORT_H
#define EFI_BUSYBOX_UTILITY_SUPPORT_H

#include <alloca.h>
#include <getopt.h>

#define FIX_ALIASING
#define BUILD_BUG_ON(condition) _Static_assert(!(condition), "invalid BusyBox buffer size")
#define GETOPT_RESET() (optind = 0)
#define No_argument "\0"
#define Required_argument "\1"
#define Optional_argument "\2"
#define TRUE 1
#define FALSE 0
#define SKIP 2
#define exit_SUCCESS() exit(EXIT_SUCCESS)
#define DOT_OR_DOTDOT(text) ((text)[0] == '.' && \
    ((text)[1] == '\0' || ((text)[1] == '.' && (text)[2] == '\0')))

typedef struct llist_t {
    struct llist_t *link;
    char *data;
} llist_t;
void llist_add_to(llist_t **head, void *data);
void llist_add_to_end(llist_t **head, void *data);
void *llist_pop(llist_t **head);
void llist_free(llist_t *head, void (*free_item)(void *));
extern uint32_t option_mask32;
uint32_t getopt32(char **argv, const char *options, ...);
uint32_t getopt32long(char **argv, const char *options, const char *long_options, ...);
int xatoi_positive(const char *text);
unsigned string_array_len(char **array);
extern int xfunc_error_retval;
extern void (*die_func)(void);
extern const char bb_msg_standard_input[];
void bb_die_memory_exhausted(void) NORETURN;
char *xstrndup(const char *value, size_t length);
FILE *xfopen(const char *path, const char *mode);
FILE *xfopen_stdin(const char *path);
FILE *fopen_for_read(const char *path);
FILE *xfopen_for_write(const char *path);
FILE *xfdopen_for_write(int fd);
FILE *fopen_or_warn(const char *path, const char *mode);
int fclose_if_not_stdin(FILE *stream);
char *bb_get_chunk_from_file(FILE *stream, size_t *end);
char *xmalloc_fgetline(FILE *stream);
int xmkstemp(char *name);
void xrename(const char *source, const char *destination);
void bb_simple_perror_msg_and_die(const char *text) NORETURN;
void bb_perror_msg_and_die(const char *format, ...) NORETURN;

enum {
    ACTION_RECURSE = 1 << 0,
    ACTION_FOLLOWLINKS = 1 << 1,
    ACTION_FOLLOWLINKS_L0 = 1 << 2,
    ACTION_DEPTHFIRST = 1 << 3,
    ACTION_QUIET = 1 << 4,
    ACTION_DANGLING_OK = 1 << 5,
};
typedef struct recursive_state {
    unsigned flags;
    unsigned depth;
    void *userData;
    int (*fileAction)(struct recursive_state *, const char *, struct stat *);
    int (*dirAction)(struct recursive_state *, const char *, struct stat *);
} recursive_state_t;
int recursive_action(const char *path, unsigned flags,
    int (*file_action)(struct recursive_state *, const char *, struct stat *),
    int (*dir_action)(struct recursive_state *, const char *, struct stat *),
    void *data);
char *concat_subpath_file(const char *path, const char *name);
char *last_char_is(const char *text, int value);

#endif
