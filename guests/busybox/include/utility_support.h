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

typedef struct llist_t {
    struct llist_t *link;
    char *data;
} llist_t;
void llist_add_to(llist_t **head, void *data);
void llist_add_to_end(llist_t **head, void *data);
void *llist_pop(llist_t **head);
void llist_free(llist_t *head, void (*free_item)(void *));
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

#endif
