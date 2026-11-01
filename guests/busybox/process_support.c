#include "libbb.h"

int kill(int pid, int number)
{
    int result = efi_process_kill(pid, number);
    if (result >= 0)
        return result;
    switch (result) {
    case -3: errno = ESRCH; break;
    case -22: errno = EINVAL; break;
    case -95: errno = ENOTSUP; break;
    default: __builtin_trap();
    }
    return -1;
}

pid_t getsid(pid_t pid UNUSED_PARAM)
{
    errno = ENOTSUP;
    return -1;
}

void free_procps_scan(procps_status_t *process)
{
    if (!process)
        return;
    closedir(process->dir);
    free(process->argv0);
    free(process);
}

procps_status_t *procps_scan(procps_status_t *process, int flags)
{
    if (flags & ~(PSSCAN_PID | PSSCAN_PPID | PSSCAN_UIDGID | PSSCAN_COMM
        | PSSCAN_STATE | PSSCAN_ARGV0 | PSSCAN_ARGVN))
        bb_simple_error_msg_and_die("requested process information is unavailable");
    if (!process) {
        process = xzalloc(sizeof(*process));
        process->dir = opendir("/proc");
        if (!process->dir) {
            free(process);
            bb_simple_perror_msg_and_die("/proc");
        }
    }
    struct dirent *entry;
    while ((entry = readdir(process->dir)) != NULL) {
        char *end;
        unsigned long pid = strtoul(entry->d_name, &end, 10);
        if (!*entry->d_name || *end || !pid || pid > UINT_MAX)
            continue;
        char path[64];
        snprintf(path, sizeof(path), "/proc/%lu/status", pid);
        FILE *status = fopen(path, "r");
        if (!status)
            continue;
        free(process->argv0);
        process->argv0 = NULL;
        process->argv_len = 0;
        memset(&process->vsz, 0, sizeof(*process) - offsetof(procps_status_t, vsz));
        process->pid = pid;
        char line[512];
        while (fgets(line, sizeof(line), status)) {
            if (strncmp(line, "Name:\t", 6) == 0) {
                line[strcspn(line, "\n")] = '\0';
                safe_strncpy(process->comm, line + 6, sizeof(process->comm));
            } else if (strncmp(line, "PPid:\t", 6) == 0) {
                process->ppid = strtoul(line + 6, NULL, 10);
            } else if (strncmp(line, "Uid:\t", 5) == 0) {
                process->uid = strtoul(line + 5, NULL, 10);
            } else if (strncmp(line, "Gid:\t", 5) == 0) {
                process->gid = strtoul(line + 5, NULL, 10);
            } else if (strncmp(line, "State:\t", 7) == 0) {
                process->state[0] = line[7];
            }
        }
        fclose(status);
        if (flags & (PSSCAN_ARGV0 | PSSCAN_ARGVN)) {
            snprintf(path, sizeof(path), "/proc/%lu/cmdline", pid);
            FILE *arguments = fopen(path, "r");
            if (arguments) {
                char *bytes = xmalloc(65536);
                size_t count = fread(bytes, 1, 65535, arguments);
                fclose(arguments);
                bytes[count] = '\0';
                process->argv_len = count;
                if (count)
                    process->argv0 = bytes;
                else
                    free(bytes);
            }
        }
        return process;
    }
    free_procps_scan(process);
    return NULL;
}

void read_cmdline(char *buffer, int size, unsigned pid, const char *command)
{
    char path[64];
    snprintf(path, sizeof(path), "/proc/%u/cmdline", pid);
    FILE *stream = fopen(path, "r");
    size_t count = stream ? fread(buffer, 1, size, stream) : 0;
    if (stream)
        fclose(stream);
    while (count && buffer[count - 1] == '\0')
        --count;
    for (size_t index = 0; index < count; ++index)
        if ((unsigned char)buffer[index] < ' ')
            buffer[index] = ' ';
    buffer[count] = '\0';
    if (!count)
        snprintf(buffer, size + 1, "[%s]", command);
}
