#include "busybox.h"

#include <dirent.h>
#include <fcntl.h>
#include <sys/stat.h>
#include <unistd.h>

static int applet_date(void)
{
    time_t now = time(NULL);
    struct tm *value = gmtime(&now);
    char text[64];
    if (!value || strftime(text, sizeof(text), "%a %b %e %H:%M:%S UTC %Y", value) == 0)
        return 1;
    puts(text);
    return 0;
}

static int applet_sleep(int argc, char **argv)
{
    if (argc != 2)
        return 2;
    char *end;
    unsigned long seconds = strtoul(argv[1], &end, 10);
    if (*end != '\0' || seconds > 86400)
        return 2;
    struct timespec delay = {(time_t)seconds, 0};
    return nanosleep(&delay, NULL);
}

static int applet_sed(int argc, char **argv)
{
    if (argc < 2 || strncmp(argv[1], "s/", 2) != 0)
        return 2;
    const char *separator = strchr(argv[1] + 2, '/');
    if (!separator)
        return 2;
    const char *replacement = separator + 1;
    const char *end = strchr(replacement, '/');
    if (!end)
        return 2;
    size_t needle_size = (size_t)(separator - (argv[1] + 2));
    size_t replacement_size = (size_t)(end - replacement);
    FILE *input = argc > 2 ? fopen(argv[2], "rb") : stdin;
    if (!input)
        return 2;
    char needle[256];
    if (needle_size == 0 || needle_size >= sizeof(needle))
        return 2;
    memcpy(needle, argv[1] + 2, needle_size);
    needle[needle_size] = '\0';
    char *line = NULL;
    size_t capacity = 0;
    while (getline(&line, &capacity, input) >= 0) {
        char *match = strstr(line, needle);
        if (match) {
            fwrite(line, 1, (size_t)(match - line), stdout);
            fwrite(replacement, 1, replacement_size, stdout);
            fputs(match + needle_size, stdout);
        } else {
            fputs(line, stdout);
        }
    }
    free(line);
    if (input != stdin)
        fclose(input);
    return 0;
}

int awk_main(int argc, char **argv);

static int copy_stream(int input, int output, long limit)
{
    char buffer[65536];
    for (;;) {
        ssize_t count = read(input, buffer, sizeof(buffer));
        if (count == 0)
            return 0;
        if (count < 0)
            return 1;
        if (limit >= 0 && count > limit)
            count = limit;
        ssize_t written = 0;
        while (written < count) {
            ssize_t part = write(output, buffer + written, (size_t)(count - written));
            if (part <= 0)
                return 1;
            written += part;
        }
        if (limit >= 0) {
            limit -= count;
            if (limit == 0)
                return 0;
        }
    }
}

static int applet_cat(int argc, char **argv)
{
    if (argc == 1)
        return copy_stream(STDIN_FILENO, STDOUT_FILENO, -1);
    for (int i = 1; i < argc; ++i) {
        int input = strcmp(argv[i], "-") == 0 ? STDIN_FILENO : open(argv[i], O_RDONLY);
        if (input < 0) {
            bb_simple_perror_msg(argv[i]);
            return 1;
        }
        int status = copy_stream(input, STDOUT_FILENO, -1);
        if (input != STDIN_FILENO)
            close(input);
        if (status)
            return status;
    }
    return 0;
}

static int applet_head(int argc, char **argv)
{
    long lines = 10;
    int index = 1;
    if (index + 1 < argc && strcmp(argv[index], "-n") == 0) {
        lines = strtol(argv[index + 1], NULL, 10);
        index += 2;
    }
    if (lines < 0)
        lines = 0;
    if (index == argc)
        return copy_stream(STDIN_FILENO, STDOUT_FILENO, -1);
    for (; index < argc; ++index) {
        int input = open(argv[index], O_RDONLY);
        if (input < 0) {
            bb_simple_perror_msg(argv[index]);
            return 1;
        }
        char buffer[65536];
        long remaining = lines;
        while (remaining > 0) {
            ssize_t count = read(input, buffer, sizeof(buffer));
            if (count <= 0)
                break;
            ssize_t end = count;
            for (ssize_t i = 0; i < count && remaining > 0; ++i) {
                if (buffer[i] == '\n')
                    --remaining;
                if (remaining == 0)
                    end = i + 1;
            }
            if (write(STDOUT_FILENO, buffer, (size_t)end) != end) {
                close(input);
                return 1;
            }
        }
        close(input);
    }
    return 0;
}

static int applet_ls(int argc, char **argv)
{
    const char *path = argc > 1 ? argv[argc - 1] : ".";
    DIR *directory = opendir(path);
    if (!directory) {
        bb_simple_perror_msg(path);
        return 1;
    }
    struct dirent *entry;
    while ((entry = readdir(directory)) != NULL) {
        if (entry->d_name[0] == '.' && entry->d_name[1] != '\0')
            continue;
        puts(entry->d_name);
    }
    closedir(directory);
    return 0;
}

static int applet_env(void)
{
    for (char **entry = environ; entry && *entry; ++entry)
        puts(*entry);
    return 0;
}

static int applet_yes(int argc, char **argv)
{
    const char *text = argc > 1 ? argv[1] : "y";
    for (;;) {
        if (puts(text) == EOF)
            return 1;
    }
}

static int applet_grep(int argc, char **argv)
{
    if (argc < 2) {
        fputs("grep: missing pattern\n", stderr);
        return 2;
    }
    const char *pattern = argv[1];
    FILE *input = stdin;
    if (argc > 2) {
        input = fopen(argv[2], "rb");
        if (!input) {
            bb_simple_perror_msg(argv[2]);
            return 2;
        }
    }
    char *line = NULL;
    size_t capacity = 0;
    int found = 1;
    while (getline(&line, &capacity, input) >= 0) {
        if (strstr(line, pattern)) {
            fputs(line, stdout);
            found = 0;
        }
    }
    free(line);
    if (input != stdin)
        fclose(input);
    return found;
}

static int applet_mkdir(int argc, char **argv)
{
    int parents = argc > 1 && strcmp(argv[1], "-p") == 0;
    int first = parents ? 2 : 1;
    int status = 0;
    for (int i = first; i < argc; ++i) {
        if (parents) {
            char path[4096];
            size_t length = strlen(argv[i]);
            if (length >= sizeof(path)) {
                status = 1;
                continue;
            }
            memcpy(path, argv[i], length + 1);
            for (char *cursor = path + 1; *cursor; ++cursor) {
                if (*cursor != '/')
                    continue;
                *cursor = '\0';
                if (*path && mkdir(path, 0777) < 0 && errno != EEXIST)
                    status = 1;
                *cursor = '/';
            }
            if (mkdir(path, 0777) < 0 && errno != EEXIST)
                status = 1;
        } else if (mkdir(argv[i], 0777) < 0) {
            bb_simple_perror_msg(argv[i]);
            status = 1;
        }
    }
    return status;
}

static int applet_rm(int argc, char **argv)
{
    int status = 0;
    for (int i = 1; i < argc; ++i) {
        if (unlink(argv[i]) < 0) {
            bb_simple_perror_msg(argv[i]);
            status = 1;
        }
    }
    return status;
}

static int applet_cp_mv(int argc, char **argv, int move)
{
    if (argc != 3) {
        fputs(move ? "mv: expected source and destination\n" : "cp: expected source and destination\n", stderr);
        return 2;
    }
    if (move)
        return rename(argv[1], argv[2]) == 0 ? 0 : (bb_simple_perror_msg(argv[1]), 1);
    int input = open(argv[1], O_RDONLY);
    int output = open(argv[2], O_WRONLY | O_CREAT | O_TRUNC, 0666);
    if (input < 0 || output < 0) {
        bb_simple_perror_msg(input < 0 ? argv[1] : argv[2]);
        if (input >= 0) close(input);
        if (output >= 0) close(output);
        return 1;
    }
    int status = copy_stream(input, output, -1);
    close(input);
    close(output);
    return status;
}

int efi_busybox_applet(int argc, char **argv)
{
    const char *name = bb_basename(argv[0]);
    applet_name = name;
    if (strcmp(name, "cat") == 0) return applet_cat(argc, argv);
    if (strcmp(name, "head") == 0) return applet_head(argc, argv);
    if (strcmp(name, "ls") == 0) return applet_ls(argc, argv);
    if (strcmp(name, "env") == 0) return applet_env();
    if (strcmp(name, "yes") == 0) return applet_yes(argc, argv);
    if (strcmp(name, "grep") == 0) return applet_grep(argc, argv);
    if (strcmp(name, "mkdir") == 0) return applet_mkdir(argc, argv);
    if (strcmp(name, "rm") == 0) return applet_rm(argc, argv);
    if (strcmp(name, "cp") == 0) return applet_cp_mv(argc, argv, 0);
    if (strcmp(name, "mv") == 0) return applet_cp_mv(argc, argv, 1);
    if (strcmp(name, "date") == 0) return applet_date();
    if (strcmp(name, "sleep") == 0) return applet_sleep(argc, argv);
    if (strcmp(name, "sed") == 0) return applet_sed(argc, argv);
    if (strcmp(name, "awk") == 0) return awk_main(argc, argv);
    fputs("busybox: applet is unavailable\n", stderr);
    return 127;
}
