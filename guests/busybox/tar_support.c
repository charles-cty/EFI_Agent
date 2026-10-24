#include "libbb.h"
#include "bb_archive.h"
#include "tar_support.h"

static pid_t transformer_pid;

static void replace_mapping(struct efi_process_fd_mapping mappings[127],
    unsigned *count, unsigned target, unsigned source)
{
    for (unsigned index = 0; index < *count; ++index) {
        if (mappings[index].target == target) {
            mappings[index].source = source;
            return;
        }
    }
    if (*count == 127)
        __builtin_trap();
    mappings[(*count)++] = (struct efi_process_fd_mapping){target, source};
}

void efi_tar_transform(int fd, bool compress)
{
    if (transformer_pid)
        bb_simple_error_msg_and_die("nested tar compression");
    uint32_t ends[2];
    int result = efi_process_pipe(ends);
    if (result < 0)
        bb_simple_error_msg_and_die("tar compression pipe");
    close_on_exec_on(ends[0]);
    close_on_exec_on(ends[1]);
    close_on_exec_on(fd);
    struct efi_process_fd_mapping mappings[127];
    unsigned count = efi_ash_inherit_fds(mappings);
    replace_mapping(mappings, &count, STDIN_FILENO, compress ? ends[0] : (unsigned)fd);
    replace_mapping(mappings, &count, STDOUT_FILENO, compress ? (unsigned)fd : ends[1]);
    const char *arguments[] = {compress ? "gzip" : "gunzip", "-c"};
    unsigned env_count = 0;
    while (environ && environ[env_count])
        ++env_count;
    char *scratch = xmalloc(65536);
    int32_t pid = efi_process_spawn_argv_env_mapped(arguments, 2,
        (const char *const *)environ, env_count, scratch, 65536, mappings, count);
    free(scratch);
    if (pid < 0)
        bb_error_msg_and_die("tar compression launch failed: %d", (int)pid);
    transformer_pid = pid;
    xdup2(compress ? ends[1] : ends[0], fd);
    xclose(ends[0]);
    xclose(ends[1]);
}

pid_t efi_tar_wait(int *status)
{
    if (!transformer_pid) {
        *status = 0;
        return 0;
    }
    pid_t pid = transformer_pid;
    int32_t code;
    if (efi_process_waitpid(pid, &code) != pid)
        bb_simple_error_msg_and_die("tar compression wait failed");
    transformer_pid = 0;
    *status = code == 0 ? 0 : EXIT_FAILURE << 8;
    return pid;
}

void check_errors_in_children(int signal_number UNUSED_PARAM)
{
    int status;
    efi_tar_wait(&status);
    if (status)
        bb_got_signal = EXIT_FAILURE;
}

void fork_transformer(int fd, int signature_skipped,
    int (*transformer)(transformer_state_t *))
{
    if (signature_skipped || transformer != unpack_gz_stream)
        bb_simple_error_msg_and_die("unsupported tar decompressor");
    efi_tar_transform(fd, false);
}

int setup_unzip_on_fd(int fd, int fail_if_not_compressed UNUSED_PARAM)
{
    unsigned char magic[2];
    ssize_t size = full_read(fd, magic, sizeof(magic));
    if (size < 0)
        bb_simple_perror_msg_and_die(bb_msg_read_error);
    if (lseek(fd, 0, SEEK_SET) < 0)
        bb_simple_perror_msg_and_die("compressed tar seek");
    if (size != 2 || magic[0] != 0x1f || magic[1] != 0x8b)
        return -1;
    efi_tar_transform(fd, false);
    return 0;
}
