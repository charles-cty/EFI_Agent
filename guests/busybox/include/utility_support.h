#ifndef EFI_BUSYBOX_UTILITY_SUPPORT_H
#define EFI_BUSYBOX_UTILITY_SUPPORT_H

#include <alloca.h>
#include <getopt.h>
#include <sys/time.h>

#define ALIGN4 __attribute__((aligned(4)))
#define PACKED __attribute__((packed))
#define USE_FOR_NOMMU(...)
#define SEAMLESS_COMPRESSION 0
#define BUILD_BUG_ON(condition) _Static_assert(!(condition), "invalid BusyBox buffer size")
#define GETOPT_RESET() (optind = 0)
#define No_argument "\0"
#define Required_argument "\1"
#define Optional_argument "\2"
#define TRUE 1
#define FALSE 0
#define SKIP 2
#define exit_SUCCESS() exit(EXIT_SUCCESS)
#define MAXINT(T) (T)(((T)-1) > 0 ? (T)-1 : (T)~((T)1 << (sizeof(T)*8-1)))
#define MIN(left, right) ((left) < (right) ? (left) : (right))
#define MAX(left, right) ((left) > (right) ? (left) : (right))
#define SET_PTR_TO_GLOBALS(value) (ptr_to_globals = (void *)(value))
int gunzip_main(int argc, char **argv);
extern int efi_archive_root_fd;
void efi_archive_begin(void);
void efi_archive_end(void);
void efi_archive_file_attributes(int fd, mode_t mode, unsigned date, unsigned time);
void efi_archive_remember_attributes(const char *path, mode_t mode,
    unsigned date, unsigned time);
uint32_t efi_archive_crc(const void *data, size_t size);
int xopen3(const char *path, int flags, int mode);
void xread(int fd, void *data, size_t size);
void xchdir(const char *path);
void xdup2(int source, int destination);
void bb_copyfd_exact_size(int input, int output, off_t size);
void chomp(char *text);
void reset_ino_dev_hashtable(void);
typedef uint64_t uoff_t;
#define OFF_FMT "ll"
#define HUMAN_READABLE_MAX_WIDTH_STR "7"
const char *make_human_readable_str(unsigned long long size,
    unsigned long block_size, unsigned long display_unit);
char *auto_string(char *text);
const char *printable_string2(uni_stat_t *stats, const char *text);
const char *get_cached_username(uid_t uid);
const char *get_cached_groupname(gid_t gid);
enum {	/* cp.c, mv.c, install.c depend on these values. CAREFUL when changing them! */
	FILEUTILS_PRESERVE_STATUS = 1 << 0, /* -p */
	FILEUTILS_DEREFERENCE     = 1 << 1, /* !-d */
	FILEUTILS_RECUR           = 1 << 2, /* -R */
	FILEUTILS_FORCE           = 1 << 3, /* -f */
	FILEUTILS_INTERACTIVE     = 1 << 4, /* -i */
	FILEUTILS_NO_OVERWRITE    = 1 << 5, /* -n */
	FILEUTILS_MAKE_HARDLINK   = 1 << 6, /* -l */
	FILEUTILS_MAKE_SOFTLINK   = 1 << 7, /* -s */
	FILEUTILS_DEREF_SOFTLINK  = 1 << 8, /* -L */
	FILEUTILS_DEREFERENCE_L0  = 1 << 9, /* -H */
	/* -a = -pdR (mapped in cp.c) */
	/* -r = -dR  (mapped in cp.c) */
	/* -P = -d   (mapped in cp.c) */
	FILEUTILS_VERBOSE         = (1 << 13) * ENABLE_FEATURE_VERBOSE,	/* -v */
	FILEUTILS_UPDATE          = 1 << 14, /* -u */
	FILEUTILS_NO_TARGET_DIR	  = 1 << 15, /* -T */
	FILEUTILS_TARGET_DIR	  = 1 << 16, /* -t DIR */
#if ENABLE_SELINUX
	FILEUTILS_PRESERVE_SECURITY_CONTEXT = 1 << 17, /* -c */
#endif
#define FILEUTILS_CP_OPTSTR "pdRfinlsLHarPvuTt:" IF_SELINUX("c")
/* How many bits in FILEUTILS_CP_OPTSTR? */
	FILEUTILS_CP_OPTBITS      = 18 - !ENABLE_SELINUX,

	FILEUTILS_RMDEST          = 1 << FILEUTILS_CP_OPTBITS, /* cp --remove-destination */
	/* bit 18 skipped for "cp --parents" */
	FILEUTILS_REFLINK         = 1 << (20 - !ENABLE_SELINUX), /* cp --reflink=auto */
	FILEUTILS_REFLINK_ALWAYS  = 1 << (21 - !ENABLE_SELINUX), /* cp --reflink[=always] */
	/*
	 * Hole. cp may have some bits set here,
	 * they should not affect remove_file()/copy_file()
	 */
#if ENABLE_SELINUX
	FILEUTILS_SET_SECURITY_CONTEXT = 1 << 30,
#endif
	FILEUTILS_IGNORE_CHMOD_ERR = 1 << 31,
};
int bb_make_directory(char *path, long mode, int flags);
int remove_file(const char *path, int flags);
int copy_file(const char *source, const char *destination, int flags);
char *is_in_ino_dev_hashtable(const struct stat *info);
void add_to_ino_dev_hashtable(const struct stat *info, const char *name);
int open3_or_warn(const char *path, int flags, int mode);
int chown(const char *path, uid_t owner, gid_t group);
int lchown(const char *path, uid_t owner, gid_t group);
int mknod(const char *path, mode_t mode, dev_t device);
int bb_ask_y_confirmation(void);
int bb_ask_y_confirmation_FILE(FILE *stream);
void xstat(const char *path, struct stat *metadata);
void xclose(int fd);
void xunlink(const char *path);
uint32_t *crc32_new_table_le(void);
uint32_t *crc32_filltable(uint32_t *table, int endian);
uint32_t *global_crc32_new_table_le(void);
extern uint32_t *global_crc32_table;
uint32_t crc32_block_endian0(uint32_t value, const void *buffer, unsigned length,
    uint32_t *table);
int parse_datestr(const char *text, struct tm *value);
int index_in_substrings(const char *strings, const char *key);
time_t validate_tm_time(const char *text, struct tm *value);
extern const char bb_msg_invalid_date[];
extern const char bb_msg_invalid_arg_to[];
typedef smalluint exitcode_t;
char *bb_read_record(FILE *stream, int delimiter, size_t *length);
int bb_compare_decimal(const char *left, const char *right);
int bb_compare_human(const char *left, const char *right);
int bb_compare_record(const char *left, const char *right, size_t length, int fold);
#define xrealloc_vector(vector, shift, index) \
    xrealloc_vector_helper((vector), (sizeof((vector)[0]) << 8) + (shift), (index))
void *xrealloc_vector_helper(void *vector, unsigned size_and_shift, int index);
typedef double duration_t;
duration_t parse_duration_str(char *text);
void sleep_for_duration(duration_t duration);
#define isprint_asciionly(value) ((unsigned)((value) - 0x20) <= 0x7e - 0x20)
#define DOT_OR_DOTDOT(text) ((text)[0] == '.' && \
    ((text)[1] == '\0' || ((text)[1] == '.' && (text)[2] == '\0')))

#define FIX_ALIASING __attribute__((may_alias))
typedef long bb__aliased_long FIX_ALIASING;
typedef uint16_t bb__aliased_uint16_t FIX_ALIASING;
typedef uint32_t bb__aliased_uint32_t FIX_ALIASING;
typedef uint64_t bb__aliased_uint64_t FIX_ALIASING;
#define BB_BIG_ENDIAN 0
#define BB_LITTLE_ENDIAN 1
#define BB_UNALIGNED_MEMACCESS_OK 1
#define ALIGN8 __attribute__((aligned(8)))
#define bb_bswap_64(value) __builtin_bswap64(value)
#define SWAP_LE32(value) (value)
#define SWAP_LE16(value) (value)
#define SWAP_BE16(value) __builtin_bswap16(value)
#define SWAP_BE32(value) __builtin_bswap32(value)
#define SWAP_LE64(value) (value)
#define SWAP_BE64(value) __builtin_bswap64(value)
#define move_from_unaligned16(value, pointer) memcpy(&(value), (pointer), 2)
#define move_from_unaligned32(value, pointer) memcpy(&(value), (pointer), 4)
#define move_from_unaligned64(value, pointer) memcpy(&(value), (pointer), 8)
#define move_from_unaligned_long(value, pointer) memcpy(&(value), (pointer), sizeof(long))
#define move_to_unaligned32(pointer, value) memcpy((pointer), &(uint32_t){value}, 4)
#define move_to_unaligned64(pointer, value) memcpy((pointer), &(uint64_t){value}, 8)

typedef struct md5_ctx_t {
    uint8_t wbuffer[64];
    void (*process_block)(struct md5_ctx_t *);
    uint64_t total64;
    uint32_t hash[8];
} md5_ctx_t;
typedef md5_ctx_t sha1_ctx_t;
typedef md5_ctx_t sha256_ctx_t;
typedef struct sha512_ctx_t {
    uint64_t total64[2];
    uint64_t hash[8];
    uint8_t wbuffer[128];
} sha512_ctx_t;
typedef struct sha3_ctx_t {
    uint64_t state[25];
    unsigned bytes_queued;
    unsigned input_block_bytes;
} sha3_ctx_t;
void sha256_begin(sha256_ctx_t *context);
void md5_begin(md5_ctx_t *context);
void md5_hash(md5_ctx_t *context, const void *buffer, size_t length);
unsigned md5_end(md5_ctx_t *context, void *result);
void sha1_begin(sha1_ctx_t *context);
#define sha1_hash md5_hash
unsigned sha1_end(sha1_ctx_t *context, void *result);
#define sha256_hash md5_hash
#define sha256_end sha1_end
void sha512_begin(sha512_ctx_t *context);
void sha512_hash(sha512_ctx_t *context, const void *buffer, size_t length);
unsigned sha512_end(sha512_ctx_t *context, void *result);
char *bin2hex(char *destination, const char *source, int count);
void die_if_ferror(FILE *stream, const char *name);
char *xmalloc_realpath_coreutils(char *path);
char *xmalloc_readlink_or_warn(const char *path);

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
const char *printable_string(const char *text);
char *bb_get_last_path_component_strip(char *path);
char *bb_get_last_path_component_nostrip(const char *path);
char *single_argv(char **arguments);
void BB_EXECVP_or_die(char **arguments) NORETURN;
char *bb_mode_string(char buffer[11], mode_t mode);
extern const char bb_msg_standard_output[];
extern const char bb_msg_read_error[];
extern const char *const bb_argv_dash[];
ssize_t full_read(int fd, void *buffer, size_t size);
int open_or_warn(const char *path, int flags);
int xopen(const char *path, int flags);
int open_or_warn_stdin(const char *path);
FILE *fopen_or_warn_stdin(const char *path);
off_t xlseek(int fd, off_t offset, int whence);
void xmove_fd(int source, int target);
void bb_perror_nomsg_and_die(void) NORETURN;
void die_if_ferror(FILE *stream, const char *name);
void die_if_ferror_stdout(void);
int fputs_stdout(const char *text);
extern const char bb_uuenc_tbl_base64[];
extern const char bb_uuenc_tbl_std[];
void bb_uuencode(char *store, const void *source, int length, const char *table);
enum {
    BASE64_FLAG_UU_STOP = 0x100,
    BASE64_32 = 0x200,
    BASE64_FLAG_NO_STOP_CHAR = 0x80,
};
char *decode_base64(char *destination, const char **source);
void read_base64(FILE *source, FILE *destination, int flags);
void bb_perror_msg(const char *format, ...) __attribute__((format(printf, 1, 2)));
void fflush_stdout_and_exit(int status) NORETURN;
int is_directory(const char *name, int follow_links);
void fflush_stdout_and_exit_SUCCESS(void) NORETURN;
off_t bb_copyfd_size(int input, int output, off_t size);
off_t bb_copyfd_eof(int input, int output);
int bb_cat(char **argv);
char *xmalloc_fgets(FILE *stream);
struct number_state {
    unsigned width, start, inc;
    const char *sep, *empty_str;
    smallint all, nonempty, continued;
};
int print_numbered_lines(struct number_state *state, const char *path);
enum { PRINTABLE_META = 0x100, VISIBLE_ENDLINE = 1 << 0, VISIBLE_SHOW_TABS = 1 << 1 };
void visible(unsigned value, char *buffer, int flags);

#endif
