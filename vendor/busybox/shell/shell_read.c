/* vi: set sw=4 ts=4: */
/*
 * Adapted from ash applet code
 *
 * This code is derived from software contributed to Berkeley by
 * Kenneth Almquist.
 *
 * Copyright (c) 1989, 1991, 1993, 1994
 *      The Regents of the University of California.  All rights reserved.
 *
 * Copyright (c) 1997-2005 Herbert Xu <herbert@gondor.apana.org.au>
 * was re-ported from NetBSD and debianized.
 *
 * Copyright (c) 2010 Denys Vlasenko
 * Split from ash.c
 *
 * Licensed under GPLv2 or later, see file LICENSE in this source tree.
 */
#include "busybox.h"
#include "shell_common.h"
#include <poll.h>

static int efi_read_monotonic_ms(unsigned *value)
{
    struct timespec now;
    if (clock_gettime(CLOCK_MONOTONIC, &now) < 0)
        return -1;
    *value = (uint64_t)now.tv_sec * 1000 + now.tv_nsec / 1000000;
    return 0;
}

const char* FAST_FUNC
shell_builtin_read(struct builtin_read_params *params)
{
	struct pollfd pfd[1];
#define fd (pfd[0].fd) /* -u FD */
	unsigned err;
	unsigned end_ms; /* -t TIMEOUT */
	int nchars; /* -n NUM */
	char **pp;
	char *buffer;
	char delim;
	const char *retval;
	int bufpos; /* need to be able to hold -1 */
	int startword;
	smallint backslash;
	char **argv;
	const char *ifs;
	int read_flags;

	errno = err = 0;

	argv = params->argv;
	pp = argv;
	while (*pp) {
		if (!*pp[0] || endofname(*pp)[0] != '\0') {
			/* Mimic bash message */
			bb_error_msg("read: '%s': bad variable name", *pp);
			return (const char *)(uintptr_t)1;
		}
		pp++;
	}

	nchars = 0; /* if != 0, -n is in effect */
	if (params->opt_n) {
		nchars = bb_strtou(params->opt_n, NULL, 10);
		if (nchars < 0 || errno)
			return "invalid count";
		/* note: "-n 0": off (bash 3.2 does this too) */
	}

	end_ms = 0;
	if (params->opt_t && !ENABLE_FEATURE_SH_READ_FRAC) {
		end_ms = bb_strtou(params->opt_t, NULL, 10);
		if (errno)
			return "invalid timeout";
		if (end_ms > UINT_MAX / 2048) /* be safely away from overflow */
			end_ms = UINT_MAX / 2048;
		end_ms *= 1000;
	}
	if (params->opt_t && ENABLE_FEATURE_SH_READ_FRAC) {
		/* bash 4.3 (maybe earlier) supports -t N.NNNNNN */
		char *p;
		/* Eat up to three fractional digits */
		int frac_digits = 3 + 1;

		end_ms = bb_strtou(params->opt_t, &p, 10);
		if (end_ms > UINT_MAX / 2048) /* be safely away from overflow */
			end_ms = UINT_MAX / 2048;

		if (errno) {
			/* EINVAL = number is ok, but not NUL terminated */
			if (errno != EINVAL || *p != '.')
				return "invalid timeout";
			/* Do not check the rest: bash allows "0.123456xyz" */
			while (*++p && --frac_digits) {
				end_ms *= 10;
				end_ms += (*p - '0');
				if ((unsigned char)(*p - '0') > 9)
					return "invalid timeout";
			}
		}
		while (--frac_digits > 0) {
			end_ms *= 10;
		}
	}

	fd = STDIN_FILENO;
	if (params->opt_u) {
		fd = bb_strtou(params->opt_u, NULL, 10);
		if (fd < 0 || errno)
			return "invalid file descriptor";
	}

	if (params->opt_t && end_ms == 0) {
		/* "If timeout is 0, read returns immediately, without trying
		 * to read any data. The exit status is 0 if input is available
		 * on the specified file descriptor, non-zero otherwise."
		 * bash seems to ignore -p PROMPT for this use case.
		 */
		int r;
		pfd[0].events = POLLIN;
		r = poll(pfd, 1, /*timeout:*/ 0);
		/* Return 0 only if poll returns 1 ("one fd ready"), else return 1: */
		return (const char *)(uintptr_t)(r <= 0 || (pfd[0].revents & (POLLERR | POLLNVAL)));
	}

	if (params->opt_p && isatty(fd)) {
		fputs(params->opt_p, stderr);
		fflush_all();
	}

	ifs = params->ifs;
	if (ifs == NULL)
		ifs = defifs;

	read_flags = params->read_flags;
    if (read_flags & BUILTIN_READ_SILENT)
        return "read: terminal echo control is unavailable";

	retval = (const char *)(uintptr_t)0;
	startword = 1;
	backslash = 0;
    if (params->opt_t) {
        unsigned now;
        if (efi_read_monotonic_ms(&now) < 0)
            return "read: monotonic timeout clock is unavailable";
        end_ms += now;
    }
	buffer = NULL;
	bufpos = 0;
	delim = params->opt_d ? params->opt_d[0] : '\n';
	do {
		char c;
		int timeout;

		if ((bufpos & 0xff) == 0)
			buffer = xrealloc(buffer, bufpos + 0x101);

		timeout = -1;
		if (params->opt_t) {
            unsigned now;
            if (efi_read_monotonic_ms(&now) < 0) {
                err = errno;
                retval = (const char *)(uintptr_t)1;
                goto ret;
            }
            timeout = end_ms - now;
			/* ^^^^^^^^^^^^^ all values are unsigned,
			 * wrapping math is used here, good even if
			 * 32-bit unix time wrapped (year 2038+).
			 */
			if (timeout <= 0) { /* already late? */
				retval = (const char *)(uintptr_t)1;
				goto ret;
			}
		}

		/* We must poll even if timeout is -1:
		 * we want to be interrupted if signal arrives,
		 * regardless of SA_RESTART-ness of that signal!
		 */
		errno = 0;
		pfd[0].events = POLLIN;
//TODO race with a signal arriving just before the poll!
		if (poll(pfd, 1, timeout) <= 0) {
			/* timed out, or EINTR */
			err = errno;
			retval = (const char *)(uintptr_t)1;
			goto ret;
		}
		if (read(fd, &buffer[bufpos], 1) != 1) {
			err = errno;
			retval = (const char *)(uintptr_t)1;
			break;
		}

		c = buffer[bufpos];
		if (!(read_flags & BUILTIN_READ_RAW)) {
			if (backslash) {
				backslash = 0;
				if (c != '\n')
					goto put;
				continue;
			}
			if (c == '\\') {
				backslash = 1;
				continue;
			}
		}
		if (c == delim) /* '\n' or -d CHAR */
			break;
		if (c == '\0')
			continue;

		/* $IFS splitting. NOT done if we run "read"
		 * without variable names (bash compat).
		 * Thus, "read" and "read REPLY" are not the same.
		 */
		if (argv[0]) {
/* http://www.opengroup.org/onlinepubs/9699919799/utilities/V3_chap02.html#tag_18_06_05 */
			const char *is_ifs = strchr(ifs, c);
			if (startword && is_ifs) {
				if (isspace(c))
					continue;
				/* it is a non-space ifs char */
				startword--;
				if (startword == 1) /* first one? */
					continue; /* yes, it is not next word yet */
			}
			startword = 0;
			if (argv[1] != NULL && is_ifs) {
				buffer[bufpos] = '\0';
				bufpos = 0;
				params->setvar(*argv, buffer);
				argv++;
				/* can we skip one non-space ifs char? (2: yes) */
				startword = isspace(c) ? 2 : 1;
				continue;
			}
		}
 put:
		bufpos++;
	} while (--nchars);

	if (argv[0]) {
		/* Remove trailing space $IFS chars */
		while (--bufpos >= 0
		 && isspace(buffer[bufpos])
		 && strchr(ifs, buffer[bufpos]) != NULL
		) {
			continue;
		}
		buffer[bufpos + 1] = '\0';

		/* Last variable takes the entire remainder with delimiters
		 * (sans trailing whitespace $IFS),
		 * but ***only "if there are fewer vars than fields"(c)***!
		 * The "X:Y:" case below: there are two fields,
		 * and therefore last delimiter (:) is eaten:
		 * IFS=": "
		 * echo "X:Y:Z:"  | (read x y; echo "|$x|$y|") # |X|Y:Z:|
		 * echo "X:Y:Z"   | (read x y; echo "|$x|$y|") # |X|Y:Z|
		 * echo "X:Y:"    | (read x y; echo "|$x|$y|") # |X|Y|, not |X|Y:|
		 * echo "X:Y  : " | (read x y; echo "|$x|$y|") # |X|Y|
		 */
		if (bufpos >= 0
		 && strchr(ifs, buffer[bufpos]) != NULL
		) {
			/* There _is_ a non-whitespace IFS char */
			/* Skip whitespace IFS char before it */
			while (--bufpos >= 0
			 && isspace(buffer[bufpos])
			 && strchr(ifs, buffer[bufpos]) != NULL
			) {
				continue;
			}
			/* Are there $IFS chars? */
			if (strcspn(buffer, ifs) >= ++bufpos) {
				/* No: last var takes one field, not more */
				/* So, drop trailing IFS delims */
				buffer[bufpos] = '\0';
			}
		}

		/* Use the remainder as a value for the next variable */
		params->setvar(*argv, buffer);
		/* Set the rest to "" */
		while (*++argv)
			params->setvar(*argv, "");
	} else {
		/* Note: no $IFS removal */
		buffer[bufpos] = '\0';
		params->setvar("REPLY", buffer);
	}

 ret:
	free(buffer);

	errno = err;
	return retval;
#undef fd
}
