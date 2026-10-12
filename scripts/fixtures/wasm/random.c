#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <wasi/api.h>

static void require(int condition, const char *step)
{
    if (!condition) {
        fprintf(stderr, "random %s: errno %d\n", step, errno);
        exit(99);
    }
}

int main(int argc, char **argv)
{
    unsigned char buffer[258];
    memset(buffer, 0xa5, sizeof(buffer));
    require(__wasi_random_get(buffer, 0) == 0, "empty request");
    require(__wasi_random_get((void *)(uintptr_t)UINT32_MAX, 2) == __WASI_ERRNO_FAULT,
        "overflowing range");
    if (argc == 2 && strcmp(argv[1], "unavailable") == 0) {
        require(__wasi_random_get(buffer + 1, 256) == __WASI_ERRNO_NOTSUP, "unavailable source");
        for (size_t index = 0; index < sizeof(buffer); ++index) {
            require(buffer[index] == 0xa5, "failure preserves output");
        }
        require(getentropy(buffer + 1, 256) == -1 && errno == ENOTSUP, "libc unavailable source");
        puts("WASM_RANDOM_UNAVAILABLE_OK");
        return 0;
    }
    require(getentropy(buffer + 1, 257) == -1 && errno == EIO, "libc length limit");
    require(buffer[0] == 0xa5 && buffer[257] == 0xa5, "length rejection guards");
    FILE *file = fopen("wasm-random.bin", "wb");
    require(file != NULL, "open records");
    for (size_t index = 0; index < 3; ++index) {
        require(getentropy(buffer + 1, 256) == 0, "libc entropy");
        require(buffer[0] == 0xa5 && buffer[257] == 0xa5, "entropy guards");
        require(fwrite(buffer + 1, 1, 256, file) == 256, "save entropy record");
    }
    require(fclose(file) == 0, "close records");
    puts("WASM_RANDOM_OK");
    return 0;
}
