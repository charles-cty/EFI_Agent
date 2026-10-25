/* ABI probe only. This program checks the command produced by QuickJS;
 * it does not interpret shell syntax or replace BusyBox ash. */
#include <stdio.h>
#include <string.h>

int main(int argc, char **argv)
{
    if (argc != 3 || strcmp(argv[1], "-c") != 0) {
        return 99;
    }
    if (strcmp(argv[2], "fail") == 0) {
        fputs("probe-error", stderr);
        return 17;
    }
    if (strcmp(argv[2], "inspect $'a\\'b' $'' $'x;$(bad)' $'λ'") != 0) {
        fprintf(stderr, "Unexpected command: %s", argv[2]);
        return 98;
    }
    if (getchar() != EOF) {
        return 97;
    }
    /* Fill stderr before stdout to exercise both-pipe polling in QuickJS. */
    for (int index = 0; index < 70003; ++index) {
        putc('E', stderr);
    }
    fputs("probe-output", stdout);
    return 0;
}
