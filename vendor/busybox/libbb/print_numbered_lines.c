/* vi: set sw=4 ts=4: */
/*
 * Copyright (C) 2017 Denys Vlasenko <vda.linux@googlemail.com>
 *
 * Licensed under GPLv2, see file LICENSE in this source tree.
 */
//kbuild:lib-y += print_numbered_lines.o

#include "libbb.h"

int FAST_FUNC print_numbered_lines(struct number_state *ns, const char *filename)
{
	FILE *fp = fopen_or_warn_stdin(filename);
	unsigned N;
    char *line = NULL;
    size_t capacity = 0;
    ssize_t length;
    int status = EXIT_SUCCESS;

	if (!fp)
		return EXIT_FAILURE;

	N = ns->start;
    while ((length = getline(&line, &capacity, fp)) >= 0) {
        if (!ns->continued) {
            if (ns->all || (ns->nonempty && line[0] != '\n')) {
                printf("%*u%s", ns->width, N, ns->sep);
                N += ns->inc;
            } else if (ns->empty_str) {
                fputs_stdout(ns->empty_str);
            }
        }
        fwrite(line, 1, length, stdout);
        ns->continued = line[length - 1] != '\n';
    }
    free(line);
    if (ferror(fp)) {
        bb_simple_perror_msg(filename);
        status = EXIT_FAILURE;
    }

	ns->start = N;

    if (fclose_if_not_stdin(fp)) {
        bb_simple_perror_msg(filename);
        return EXIT_FAILURE;
    }

	return status;
}
