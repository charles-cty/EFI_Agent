#ifndef EFI_PROCESS_H
#define EFI_PROCESS_H

#include <stdint.h>

#define EFI_PROCESS_IMPORT(name) \
    __attribute__((import_module("efi_process"), import_name(name)))

/* Process calls return a nonnegative result or a negative POSIX errno.
 * waitpid writes the direct exit code, not a packed POSIX wait status.
 * Standard spawn inherits the three selected standard descriptors.
 * Ordinary spawn inherits the parent's startup environment. Pass an explicit
 * list to spawn_env_fds for shell exports or changes made by guest libc.
 */
EFI_PROCESS_IMPORT("getpid") int32_t efi_process_getpid(void);
EFI_PROCESS_IMPORT("getppid") int32_t efi_process_getppid(void);
/* Positive PID only. Signal 0 checks existence; INT, KILL and TERM use default
 * termination with status 128 + signal. Handlers and process groups are absent.
 * Termination affects the selected process; its children are adopted by PID 0.
 */
EFI_PROCESS_IMPORT("kill") int32_t efi_process_kill(int32_t pid, int32_t signal);
/* Exact registry name lookup: 1 present, 0 absent, or negative POSIX errno.
 * The UTF-8 bytes exclude a terminating NUL. Lookup creates no process.
 */
EFI_PROCESS_IMPORT("program_exists") int32_t efi_process_program_exists(
    const char *name, uint32_t size);
EFI_PROCESS_IMPORT("pipe") int32_t efi_process_pipe(uint32_t descriptors[2]);
/* Copy up to 512 KiB into an anonymous read-only stream. Reads consume the
 * shared stream across duplicated/inherited descriptors and then return EOF.
 * The descriptor is not seekable. Guest source memory can change afterward.
 */
EFI_PROCESS_IMPORT("input_snapshot") int32_t efi_process_input_snapshot(
    const void *bytes, uint32_t size);
EFI_PROCESS_IMPORT("dup2") int32_t efi_process_dup2(uint32_t source, uint32_t target);
/* Duplicate to the lowest free descriptor at or above minimum. */
EFI_PROCESS_IMPORT("dupfd") int32_t efi_process_dupfd(uint32_t source, uint32_t minimum);
/* Per-descriptor flags: bit 0 is close-on-exec. All descriptor kinds support
 * it. Duplication clears the copy's flag; same-fd dup2 and renumber preserve
 * it. Closing a descriptor discards its flag before that number is reused.
 * get returns 0/1 or -9; set returns 0, -9 (absent) or -22 (unknown flags).
 */
EFI_PROCESS_IMPORT("get_fd_flags") int32_t efi_process_get_fd_flags(uint32_t descriptor);
EFI_PROCESS_IMPORT("set_fd_flags") int32_t efi_process_set_fd_flags(
    uint32_t descriptor, uint32_t flags);
EFI_PROCESS_IMPORT("waitpid") int32_t efi_process_waitpid(uint32_t pid, int32_t *status);
/* Cancel a direct child's subtree. The direct child remains waitable with 130;
 * descendants are reaped. Other callers' children return -ECHILD.
 */
EFI_PROCESS_IMPORT("cancel_child") int32_t efi_process_cancel_child(uint32_t pid);
/* Returns 0 without writing status while the child remains live. */
EFI_PROCESS_IMPORT("waitpid_nohang") int32_t efi_process_waitpid_nohang(
    uint32_t pid, int32_t *status);
EFI_PROCESS_IMPORT("spawn") int32_t efi_process_spawn_raw(const char *arguments, uint32_t size);
EFI_PROCESS_IMPORT("spawn_fds") int32_t efi_process_spawn_fds_raw(
    const char *arguments, uint32_t size, uint32_t input, uint32_t output, uint32_t error);
EFI_PROCESS_IMPORT("spawn_env_fds") int32_t efi_process_spawn_env_fds_raw(
    const char *arguments, uint32_t size, const char *environment, uint32_t environment_size,
    uint32_t input, uint32_t output, uint32_t error);

/* Explicit mappings replace standard descriptor inheritance. Each entry is
 * {child target, parent source}; targets must be unique, below 128, and not 3
 * (reserved for the WASI workspace). Multiple targets may share a source.
 * All sources are read from the unchanged parent table. Explicit duplication
 * clears close-on-exec in the child and leaves the parent's flags unchanged.
 * An empty list leaves only the workspace preopen, when configured.
 */
struct efi_process_fd_mapping {
    uint32_t target;
    uint32_t source;
};
EFI_PROCESS_IMPORT("spawn_mapped_fds") int32_t efi_process_spawn_mapped_fds_raw(
    const char *arguments, uint32_t size,
    const char *environment, uint32_t environment_size,
    const struct efi_process_fd_mapping *mappings, uint32_t count);

/* The caller supplies scratch storage. The helper preserves empty arguments.
 * It accepts at most 64 KiB of UTF-8 argument bytes, including NUL separators.
 * An error can leave a partial pack in scratch; it never starts a process.
 */
int32_t efi_process_pack_argv(
    const char *const arguments[], uint32_t count, char *scratch, uint32_t capacity);
int32_t efi_process_spawn_argv(
    const char *const arguments[], uint32_t count, char *scratch, uint32_t capacity);
int32_t efi_process_spawn_argv_fds(
    const char *const arguments[], uint32_t count, char *scratch, uint32_t capacity,
    uint32_t input, uint32_t output, uint32_t error);
/* Explicit environment replaces inherited entries. Entries are NAME=value;
 * names must be nonempty and unique. An empty list clears the environment.
 * Argument and environment bytes share the 64 KiB limit and scratch buffer.
 */
int32_t efi_process_spawn_argv_env_fds(
    const char *const arguments[], uint32_t count,
    const char *const environment[], uint32_t environment_count,
    char *scratch, uint32_t capacity, uint32_t input, uint32_t output, uint32_t error);

int32_t efi_process_spawn_argv_env_mapped(
    const char *const arguments[], uint32_t count,
    const char *const environment[], uint32_t environment_count,
    char *scratch, uint32_t capacity,
    const struct efi_process_fd_mapping *mappings, uint32_t mapping_count);

#endif
