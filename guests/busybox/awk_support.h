#ifndef EFI_AWK_SUPPORT_H
#define EFI_AWK_SUPPORT_H

#include <locale.h>
#include <math.h>

#define ALIGN4 __attribute__((aligned(4)))
#define TRUE 1
#define FALSE 0
#define SET_PTR_TO_GLOBALS(value) (ptr_to_globals = (void *)(value))
#define popen efi_bb_popen
#define pclose efi_bb_pclose
#define system efi_bb_system

char *xstrndup(const char *value, size_t length);
FILE *xfopen(const char *path, const char *mode);
FILE *xfopen_stdin(const char *path);
int xopen_stdin(const char *path);
char *xmalloc_read(int fd, size_t *size);
FILE *fopen_for_read(const char *path);
FILE *efi_bb_popen(const char *command, const char *mode);
int efi_bb_pclose(FILE *stream);
int efi_bb_system(const char *command);
void bb_die_memory_exhausted(void) NORETURN;
void bb_simple_perror_msg_and_die(const char *text) NORETURN;
void bb_perror_msg_and_die(const char *format, ...) NORETURN;

#endif
