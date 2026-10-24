/* Check standard libc calls against the shared retained-cwd adapter. */
#include <assert.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

static void check_path(const char *expected)
{
    size_t size = strlen(expected) + 1;
    char *buffer = malloc(size);
    assert(buffer);
    assert(getcwd(buffer, size) == buffer && !strcmp(buffer, expected));
    buffer[0] = '!';
    errno = 0;
    assert(!getcwd(buffer, size - 1) && errno == ERANGE && buffer[0] == '!');
    errno = 0;
    assert(!getcwd(buffer, 0) && errno == EINVAL && buffer[0] == '!');
    errno = 0;
    assert(!getcwd(NULL, 1) && errno == ERANGE);
    for (unsigned index = 0; index < 300; ++index) {
        char *allocated = getcwd(NULL, index % 2 ? size + 7 : 0);
        assert(allocated && !strcmp(allocated, expected));
        free(allocated);
    }
    free(buffer);
}

int main(int argc, char **argv)
{
    assert(argc == 3);
    if (!strcmp(argv[2], "wait"))
        assert(getchar() == 'x');
    if (!strcmp(argv[2], "rename"))
        assert(rename("/c-port-cwd/before", "/c-port-cwd/moved") == 0);
    if (!strcmp(argv[1], "deleted")) {
        errno = 0;
        assert(!getcwd(NULL, 0) && errno == ENOENT);
        struct stat info;
        assert(stat(".", &info) == 0 && S_ISDIR(info.st_mode));
        assert(chdir(".") == 0);
        errno = 0;
        assert(!getcwd(NULL, 0) && errno == ENOENT);
        assert(chdir("..") == 0);
        check_path("/c-port-cwd");
    } else {
        check_path(argv[1]);
        if (!strcmp(argv[2], "inherited")) {
            errno = 0;
            assert(chdir(".") == -1 && errno == EACCES);
            check_path(argv[1]);
            puts("C_CWD_OK");
            return 0;
        }
        assert(chdir(".") == 0);
        check_path(argv[1]);
        errno = 0;
        assert(chdir("") == -1 && errno == ENOENT);
        assert(chdir("/c-port-missing") == -1 && errno == ENOENT);
        check_path(argv[1]);
        if (!strcmp(argv[2], "rename") || !strcmp(argv[2], "wait")) {
            static const unsigned char expected[] = "C_PORT_CWD_ORACLE\0\xff";
            unsigned char buffer[sizeof(expected)];
            FILE *input = fopen("input", "rb");
            assert(input);
            assert(fread(buffer, 1, sizeof(buffer), input) == sizeof(expected) - 1);
            assert(feof(input) && !memcmp(buffer, expected, sizeof(expected) - 1));
            assert(fclose(input) == 0);
            errno = 0;
            assert(chdir("input") == -1 && errno == ENOTDIR);
            FILE *output = fopen("probe-written", "wb");
            assert(output && fwrite(expected, 1, sizeof(expected) - 1, output) == sizeof(expected) - 1);
            assert(fclose(output) == 0);
        }
    }
    puts("C_CWD_OK");
    return 0;
}
