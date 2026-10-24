/* Validate the WASI TZif loader independently of localtime and date. */
#include "efi_timezone.h"
#include <assert.h>
#include <stdio.h>
#include <string.h>

int main(int argc, char **argv)
{
    assert(argc >= 3);
    int expected = !strcmp(argv[1], "valid");
    assert(expected || !strcmp(argv[1], "invalid"));
    for (int i = 2; i < argc; ++i) {
        size_t size = 0;
        const unsigned char *data = efi_timezone_map(argv[i], &size);
        if (!!data != expected) {
            fprintf(stderr, "TZif validation mismatch: %s\n", argv[i]);
            return 1;
        }
        if (data) {
            assert(size >= 44);
            assert(efi_timezone_unmap((void *)data, size) == 0);
        }
    }
    printf("TZFILE_VALIDATED %d\n", argc - 2);
    return 0;
}
