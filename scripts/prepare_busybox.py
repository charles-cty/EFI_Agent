# /// script
# requires-python = ">=3.11"
# ///
"""Prepare upstream BusyBox sources for the Windows-native WASI build."""
import argparse
from pathlib import Path
import re
from prepare_text_applets import prepare_text_applets


ENABLED = {
    "ASH", "SHELL_ASH", "ASH_ALIAS", "ASH_CMDCMD", "ASH_GETOPTS",
    "ASH_INTERNAL_GLOB", "ASH_ECHO", "ASH_PRINTF", "ASH_TEST", "FEATURE_TEST_64",
    "FEATURE_SH_MATH", "FEATURE_SH_MATH_64", "FEATURE_SH_MATH_BASE",
    "FEATURE_FANCY_ECHO", "FEATURE_SH_NOFORK",
    "AWK", "FEATURE_AWK_LIBM", "FEATURE_AWK_GNU_EXTENSIONS", "FEATURE_ALLOW_EXEC",
    "SED", "LONG_OPTS",
    "GREP", "FEATURE_GREP_CONTEXT",
    "CHMOD",
    "CAT", "FEATURE_CATN", "FEATURE_CATV", "HEAD", "FEATURE_FANCY_HEAD",
    "TAIL", "FEATURE_FANCY_TAIL", "WC", "FEATURE_WC_LARGE",
    "TEE", "FEATURE_TEE_USE_BLOCK_IO",
    "SLEEP", "FEATURE_FANCY_SLEEP", "FLOAT_DURATION",
    "BASENAME", "DIRNAME", "PWD", "ENV", "PRINTENV", "SEQ", "YES", "TRUE", "FALSE",
    "MKDIR", "RMDIR", "RM", "TOUCH", "FEATURE_TOUCH_SUSV3", "FEATURE_VERBOSE",
    "MONOTONIC_SYSCALL",
    "DATE", "FEATURE_DATE_ISOFMT", "FEATURE_DATE_NANO",
    "SORT", "FEATURE_SORT_BIG", "UNIQ", "CUT", "FEATURE_CUT_REGEX",
    "TR", "FEATURE_TR_CLASSES", "FEATURE_TR_EQUIV",
    "REALPATH",
    "OD",
    "BASE64",
    "SHA256SUM",
    "LN",
    "HEXDUMP",
    "XARGS",
    "FEATURE_XARGS_SUPPORT_QUOTES",
    "FEATURE_XARGS_SUPPORT_TERMOPT",
    "FEATURE_XARGS_SUPPORT_ZERO_TERM",
    "FEATURE_XARGS_SUPPORT_REPL_STR",
}


def replace_once(text, marker, replacement):
    if text.count(marker) != 1:
        raise ValueError(f"Upstream source marker changed: {marker!r}")
    return text.replace(marker, replacement)


def prepare_read(source: Path, output: Path):
    """Keep upstream splitting and escaping; adapt only platform boundaries."""
    common = (source / "shell/shell_common.c").read_text(encoding="utf-8")
    start = common.index("const char* FAST_FUNC\nshell_builtin_read(")
    end = common.index("\n/* ulimit builtin */", start)
    read = common[start:end]
    read = read.replace("\tstruct termios tty, old_tty;\n", "")
    start = read.index("\tif (nchars || (read_flags & BUILTIN_READ_SILENT)) {")
    end = read.index("\n\tretval =", start)
    read = read[:start] + '''    if (read_flags & BUILTIN_READ_SILENT)
        return "read: terminal echo control is unavailable";
''' + read[end:]
    read = read.replace("\tif (read_flags & BUILTIN_READ_SILENT)\n\t\ttcsetattr(fd, TCSANOW, &old_tty);\n", "")
    read = read.replace("\tif (params->opt_t)\n\t\tend_ms += (unsigned)monotonic_ms();", '''    if (params->opt_t) {
        unsigned now;
        if (efi_read_monotonic_ms(&now) < 0)
            return "read: monotonic timeout clock is unavailable";
        end_ms += now;
    }''')
    read = read.replace("\t\t\ttimeout = end_ms - (unsigned)monotonic_ms();", '''            unsigned now;
            if (efi_read_monotonic_ms(&now) < 0) {
                err = errno;
                retval = (const char *)(uintptr_t)1;
                goto ret;
            }
            timeout = end_ms - now;''')
    read = read.replace("return (const char *)(uintptr_t)(r <= 0);",
                        "return (const char *)(uintptr_t)(r <= 0 || (pfd[0].revents & (POLLERR | POLLNVAL)));")
    preamble = '''#include "busybox.h"
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

'''
    license_header = common[:common.index('#include "libbb.h"')]
    (output / "shell_read.c").write_text(license_header + preamble + read, encoding="utf-8")


def prepare_streams(source: Path, output: Path):
    """Retain upstream applets and fix byte preservation and error propagation."""
    cat = (source / "coreutils/cat.c").read_text(encoding="utf-8")
    cat = replace_once(cat, '\t\tns.start = 1;', '\t\tns.start = 1;\n        ns.continued = 0;')
    (output / "cat.c").write_text(cat, encoding="utf-8")

    numbered = (source / "libbb/print_numbered_lines.c").read_text(encoding="utf-8")
    numbered = replace_once(numbered, '\tchar *line;', '''    char *line = NULL;
    size_t capacity = 0;
    ssize_t length;
    int status = EXIT_SUCCESS;''')
    start = numbered.index('\twhile ((line = xmalloc_fgetline(fp)) != NULL) {')
    end = numbered.index('\n\tns->start = N;', start)
    numbered = numbered[:start] + '''    while ((length = getline(&line, &capacity, fp)) >= 0) {
        if (!ns->continued) {
            if (ns->all || (ns->nonempty && line[0] != '\\n')) {
                printf("%*u%s", ns->width, N, ns->sep);
                N += ns->inc;
            } else if (ns->empty_str) {
                fputs_stdout(ns->empty_str);
            }
        }
        fwrite(line, 1, length, stdout);
        ns->continued = line[length - 1] != '\\n';
    }
    free(line);
    if (ferror(fp)) {
        bb_simple_perror_msg(filename);
        status = EXIT_FAILURE;
    }
''' + numbered[end:]
    numbered = replace_once(numbered, '\tfclose(fp);', '''    if (fclose_if_not_stdin(fp)) {
        bb_simple_perror_msg(filename);
        return EXIT_FAILURE;
    }''')
    numbered = replace_once(numbered, '\treturn EXIT_SUCCESS;', '\treturn status;')
    (output / "print_numbered_lines.c").write_text(numbered, encoding="utf-8")

    head = (source / "coreutils/head.c").read_text(encoding="utf-8")
    marker = 'static void\nprint_except_N_last_lines(FILE *fp, unsigned count)'
    head = replace_once(head, marker, '''static char *head_line(FILE *fp, size_t *length)
{
    char *line = NULL;
    size_t capacity = 0;
    ssize_t size = getline(&line, &capacity, fp);
    if (size < 0) {
        free(line);
        return NULL;
    }
    *length = size;
    return line;
}

''' + marker)
    start = head.index(marker)
    end = head.index('\n#else', start)
    ring = head[start:end]
    ring = replace_once(ring, '\tunsigned head = 0;', '''    size_t *lengths = xzalloc(count * sizeof(*lengths));
    size_t length;
    unsigned head = 0;''')
    ring = ring.replace('xmalloc_fgets(fp)', 'head_line(fp, &length)')
    ring = replace_once(ring, '\t\tcircle[head++] = c;\n\t\tif (head == count)', '\t\tlengths[head] = length;\n\t\tcircle[head++] = c;\n\t\tif (head == count)')
    ring = replace_once(ring, '\t\tfputs_stdout(circle[head]);', '\t\tfwrite(circle[head], 1, lengths[head], stdout);')
    ring = replace_once(ring, '\t\tfree(circle[head]);', '\t\tlengths[head] = length;\n\t\tfree(circle[head]);')
    ring = replace_once(ring, '\tfree(circle);', '\tfree(circle);\n\tfree(lengths);')
    head = head[:start] + ring + head[end:]
    head = replace_once(head, 'bb_error_msg("count is too big: %lu", count);', 'bb_error_msg_and_die("count is too big: %lu", count);')
    head = replace_once(head, '\t\t\tdie_if_ferror_stdout();', '''            die_if_ferror_stdout();
            if (ferror(fp)) {
                bb_simple_perror_msg(*argv);
                retval = EXIT_FAILURE;
            }''')
    (output / "head.c").write_text(head, encoding="utf-8")

    tail = (source / "coreutils/tail.c").read_text(encoding="utf-8")
    tail = replace_once(tail, '\t\tsleep(sleep_period);', '''        if (sleep(sleep_period) != 0)
            bb_simple_perror_msg_and_die("sleep");''')
    tail = replace_once(tail, '\t\t\t\t\tbb_copyfd_size(fd, STDOUT_FILENO, count);', '''                    if (bb_copyfd_size(fd, STDOUT_FILENO, count) < 0)
                        G.exitcode = EXIT_FAILURE;''')
    start = tail.index('#if 1 /* This is technically incorrect for *LONG* strings')
    end = tail.index('#endif', start) + len('#endif')
    tail = tail[:start] + '                xlseek(fd, 0, SEEK_SET);' + tail[end:]
    tail = replace_once(tail, '\t\t\t\tunsigned off;\n', '')
    start = tail.index('\tif (!G.from_top && COUNT_BYTES) {\n\t\tif (tailbufsize < count + BUFSIZ)')
    end = tail.index('\n\t/* tail -c1024m', start)
    tail = tail[:start] + tail[end:]
    tail = replace_once(tail, 'taillen > (int)count', '(unsigned)taillen > count')
    tail = replace_once(tail, 'newlines_seen + newlines_in_buf < (int)count', '(unsigned)(newlines_seen + newlines_in_buf) < count')
    tail = replace_once(tail, '\t\t\t\t\t\tnwrite -= (count - seen);\n\t\t\t\t\t\tseen += nread;', '''                        unsigned skip = count - seen;
                        if (skip > (unsigned)nread)
                            skip = nread;
                        nwrite -= skip;
                        seen += skip;''')
    growth = '''\t\t\t\t\tif (tailbufsize < (size_t)taillen + BUFSIZ) {
\t\t\t\t\t\ttailbufsize = taillen + BUFSIZ;
\t\t\t\t\t\ttailbuf = xrealloc(tailbuf, tailbufsize);
\t\t\t\t\t}'''
    tail = replace_once(tail, growth, '')
    tail = replace_once(tail, '\t\t\t\tbuf = tailbuf + taillen;', growth + '\n\t\t\t\tbuf = tailbuf + taillen;')
    (output / "tail.c").write_text(tail, encoding="utf-8")

    tee = (source / "coreutils/tee.c").read_text(encoding="utf-8")
    tee = replace_once(tee, '\t\tsignal(SIGINT, SIG_IGN);', '''        if (signal(SIGINT, SIG_IGN) == SIG_ERR)
            bb_simple_perror_msg_and_die("SIGINT");''')
    tee = replace_once(tee, '\tfiles[0] = stdin;', '\tdie_if_ferror_stdout();\n\tfiles[0] = stdin;')
    (output / "tee.c").write_text(tee, encoding="utf-8")

    flush = (source / "libbb/fflush_stdout_and_exit.c").read_text(encoding="utf-8")
    flush = replace_once(flush, 'if (fflush(stdout))', 'if (fflush(stdout) || ferror(stdout))')
    (output / "fflush_stdout_and_exit.c").write_text(flush, encoding="utf-8")


def prepare_mutations(source: Path, output: Path):
    """Use upstream traversal, parsing and prompts with explicit I/O failures."""
    for name in ("mkdir", "rmdir", "rm"):
        text = (source / f"coreutils/{name}.c").read_text(encoding="utf-8")
        text = replace_once(text, '\treturn status;', '\tfflush_stdout_and_exit(status);')
        if name == "mkdir":
            text = replace_once(text, '\t\tif (bb_make_directory(*argv, mode, flags)) {', '''        if (!(flags & FILEUTILS_RECUR)
            && (LONE_CHAR(*argv, '/') || LONE_CHAR(*argv, '.'))) {
            errno = EEXIST;
            bb_simple_perror_msg(*argv);
            status = EXIT_FAILURE;
        } else if (bb_make_directory(*argv, mode, flags)) {''')
        (output / f"{name}.c").write_text(text, encoding="utf-8")

    remove = (source / "libbb/remove_file.c").read_text(encoding="utf-8")
    remove = replace_once(remove, '\t\tif (dp == NULL) {\n\t\t\treturn -1;', '''        if (dp == NULL) {
            bb_perror_msg("can't open '%s'", path);
            return -1;''')
    remove = replace_once(remove, '\t\twhile ((d = readdir(dp)) != NULL) {', '''        for (;;) {
            errno = 0;
            d = readdir(dp);
            if (d == NULL) {
                if (errno) {
                    bb_perror_msg("can't read '%s'", path);
                    status = -1;
                }
                break;
            }''')
    remove = replace_once(remove, '\t\tclosedir(dp);', '''        if (closedir(dp) < 0) {
            bb_perror_msg("can't close '%s'", path);
            status = -1;
        }''')
    remove = replace_once(remove, '\t\tif (flags & FILEUTILS_VERBOSE) {', '\t\tif (status == 0 && (flags & FILEUTILS_VERBOSE)) {')
    (output / "remove_file.c").write_text(remove, encoding="utf-8")

    touch = (source / "coreutils/touch.c").read_text(encoding="utf-8")
    touch = replace_once(touch, '''\t\t\t\t\tif (opts & (OPT_r|OPT_d|OPT_t))
\t\t\t\t\t\tfutimens(fd, timebuf);''', '''                    if ((opts & (OPT_r|OPT_d|OPT_t)) && futimens(fd, timebuf) < 0) {
                        status = EXIT_FAILURE;
                        bb_simple_perror_msg(*argv);
                    }''')
    (output / "touch.c").write_text(touch, encoding="utf-8")


def prepare_date(source: Path, output: Path):
    """Keep upstream date parsing/formatting and reject unavailable clock writes."""
    text = (source / "coreutils/date.c").read_text(encoding="utf-8")
    text = replace_once(text, '# include <sys/syscall.h>', '# include <locale.h>')
    text = replace_once(text, '\t\tclock_gettime(CLOCK_REALTIME, &ts);', '''        if (clock_gettime(CLOCK_REALTIME, &ts) < 0)
            bb_simple_perror_msg_and_die("clock_gettime");''')
    text = replace_once(text, '\tlocaltime_r(&ts.tv_sec, &tm_time);', '''    if (!localtime_r(&ts.tv_sec, &tm_time))
        bb_simple_perror_msg_and_die("localtime");''')
    text = replace_once(text, '\tif (ENABLE_FEATURE_DATE_ISOFMT && (opt & OPT_ISO8601)) {', '''    const char *timezone = getenv("TZ");
    if (timezone && *timezone && strcmp(timezone, "UTC") && strcmp(timezone, "UTC0"))
        bb_simple_error_msg_and_die("non-UTC timezones are unavailable; use -u");

\tif (ENABLE_FEATURE_DATE_ISOFMT && (opt & OPT_ISO8601)) {''')
    text = replace_once(text, '\t\t\tif (strptime(date_str, fmt_str2dt, &tm_time) == NULL)', '''            char *end = strptime(date_str, fmt_str2dt, &tm_time);
            if (end == NULL || *end != '\\0')''')
    text = replace_once(text, '''\t\tif ((opt & OPT_SET) && clock_settime(CLOCK_REALTIME, &ts) < 0) {
\t\t\tbb_simple_perror_msg("can't set date");
\t\t}''', '''        if (opt & OPT_SET) {
            errno = ENOTSUP;
            bb_simple_perror_msg_and_die("can't set date");
        }''')
    text = replace_once(text, '\t\tstrftime(date_buf, COMMON_BUFSIZE, fmt_dt2str, &tm_time);', '''        if (!strftime(date_buf, COMMON_BUFSIZE, fmt_dt2str, &tm_time))
            bb_simple_error_msg_and_die("formatted date exceeds output buffer");''')
    text = replace_once(text, '\treturn EXIT_SUCCESS;', '\tfflush_stdout_and_exit_SUCCESS();')
    (output / "date.c").write_text(text, encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    files = [args.source / "shell/ash.c", args.source / "shell/shell_common.h",
             args.source / "shell/shell_common.c",
             args.source / "coreutils/printf.c",
             args.source / "coreutils/test.c", args.source / "shell/math.c",
             args.source / "include/unicode.h", args.source / "editors/awk.c",
             args.source / "editors/sed.c", args.source / "libbb/getopt32.c",
             args.source / "findutils/grep.c", args.source / "coreutils/chmod.c",
             *(args.source / f"coreutils/{name}.c" for name in (
                 "cat", "head", "tail", "wc", "tee", "sleep", "basename",
                 "dirname", "pwd", "env", "printenv", "seq", "yes", "true", "false",
                 "mkdir", "rmdir", "rm", "touch", "od", "uudecode", "ln")),
             args.source / "coreutils/date.c", args.source / "libbb/compare_string_array.c",
             *(args.source / f"coreutils/{name}.c" for name in ("sort", "uniq", "cut", "tr", "realpath")),
             *(args.source / f"libbb/{name}.c" for name in (
                 "copyfd", "duration", "make_directory", "remove_file", "time"))]
    names = set(ENABLED)
    for path in files:
        text = path.read_text(encoding="utf-8")
        names.update(re.findall(r"\bENABLE_([A-Z0-9_]+)", text))
        names.update(re.findall(r"\bIF_(?:NOT_)?([A-Z0-9_]+)", text))
    # BASH_* helper macros are defined by upstream ash itself.
    names = {name for name in names if not name.startswith("BASH_") or name == "BASH_IS_ASH"}
    lines = ["#ifndef EFI_ASH_CONFIG_H", "#define EFI_ASH_CONFIG_H"]
    for name in sorted(names):
        enabled = name in ENABLED
        lines.append(f"#define ENABLE_{name} {int(enabled)}")
        lines.append(f"#define IF_{name}(...) {'__VA_ARGS__' if enabled else ''}")
        lines.append(f"#define IF_NOT_{name}(...) {'' if enabled else '__VA_ARGS__'}")
    lines.extend(["#define HAVE_DEV_FD 0", "#define CONFIG_FEATURE_EDITING_MAX_LEN 1024",
                  "#define CONFIG_FEATURE_COPYBUF_KB 4", "#endif", ""])
    (args.output / "ash_config.h").write_text("\n".join(lines), encoding="utf-8")
    (args.output / "NUM_APPLETS.h").write_text("#define NUM_APPLETS 1\n", encoding="utf-8")
    prepare_streams(args.source, args.output)
    prepare_mutations(args.source, args.output)
    prepare_date(args.source, args.output)
    for name in ("realpath",):
        realpath = (args.source / f"coreutils/{name}.c").read_text(encoding="utf-8")
        realpath = realpath.replace(
            '\t\tchar *resolved_path = xmalloc_realpath_coreutils(*argv);',
            '\t\tstruct stat metadata;\n\t\tif (stat(*argv, &metadata) < 0) {\n\t\t\tbb_simple_perror_msg(*argv);\n\t\t\tretval = EXIT_FAILURE;\n\t\t\tcontinue;\n\t\t}\n\t\tchar *resolved_path = xmalloc_realpath_coreutils(*argv);',
        )
        (args.output / f"{name}.c").write_text(realpath, encoding="utf-8")
    xreadlink = (args.source / "libbb/xreadlink.c").read_text(encoding="utf-8")
    xreadlink = '#ifndef KERNEL_VERSION\n#define KERNEL_VERSION(a, b, c) 0\n#endif\n' + xreadlink
    (args.output / "xreadlink.c").write_text(xreadlink, encoding="utf-8")
    (args.output / "od.c").write_text(
        (args.source / "coreutils/od.c").read_text(encoding="utf-8"),
        encoding="utf-8",
    )
    (args.output / "uudecode.c").write_text(
        (args.source / "coreutils/uudecode.c").read_text(encoding="utf-8"),
        encoding="utf-8",
    )
    (args.output / "md5_sha1_sum.c").write_text(
        (args.source / "coreutils/md5_sha1_sum.c").read_text(encoding="utf-8"),
        encoding="utf-8",
    )
    (args.output / "ln.c").write_text(
        (args.source / "coreutils/ln.c").read_text(encoding="utf-8"),
        encoding="utf-8",
    )
    (args.output / "hexdump.c").write_text(
        (args.source / "util-linux/hexdump.c").read_text(encoding="utf-8"),
        encoding="utf-8",
    )
    (args.output / "xargs.c").write_text(
        (args.source / "findutils/xargs.c").read_text(encoding="utf-8"),
        encoding="utf-8",
    )
    prepare_text_applets(args.source, args.output, replace_once)
    sleep = (args.source / "coreutils/sleep.c").read_text(encoding="utf-8")
    sleep = replace_once(sleep, '\t\t\tsleep(INT_MAX);', '''            if (sleep(INT_MAX) != 0)
                bb_simple_perror_msg_and_die("sleep");''')
    (args.output / "sleep.c").write_text(sleep, encoding="utf-8")
    duration = (args.source / "libbb/duration.c").read_text(encoding="utf-8")
    duration = replace_once(duration, '\n\tnanosleep(&ts, &ts);', '''
    if (nanosleep(&ts, &ts) < 0)
        bb_simple_perror_msg_and_die("nanosleep");''')
    (args.output / "duration.c").write_text(duration, encoding="utf-8")
    pwd = (args.source / "coreutils/pwd.c").read_text(encoding="utf-8")
    # Enable pwd's logical/physical option parser without changing the global
    # DESKTOP feature selection of other translation units.
    pwd = replace_once(pwd, '\tif (ENABLE_DESKTOP) {', '\t{')
    (args.output / "pwd.c").write_text(pwd, encoding="utf-8")
    yes = (args.source / "coreutils/yes.c").read_text(encoding="utf-8")
    yes = replace_once(yes, "} while (putchar('\\n') != EOF);",
                       "} while (putchar('\\n') != EOF && !ferror(stdout));")
    (args.output / "yes.c").write_text(yes, encoding="utf-8")
    awk = (args.source / "editors/awk.c").read_text(encoding="utf-8")
    awk = replace_once(awk, '#include "libbb.h"', '#include "libbb.h"\n#include "awk_support.h"\nstruct globals;\nstatic struct globals *ptr_to_globals;')
    (args.output / "awk.c").write_text(awk, encoding="utf-8")
    sed = (args.source / "editors/sed.c").read_text(encoding="utf-8")
    # WASI has no ownership-changing interface. Namespace ownership is fixed
    # for all files, so replacing a file does not transfer its owner.
    sed = replace_once(sed, "\t\t\tfchown(nonstdoutfd, statbuf.st_uid, statbuf.st_gid);", "")
    (args.output / "sed.c").write_text(sed, encoding="utf-8")
    grep = (args.source / "findutils/grep.c").read_text(encoding="utf-8")
    # Upstream counts the first suppressed match for -m 0, incorrectly
    # returning success. Check the limit before consuming input instead.
    grep = replace_once(grep, "\tmatched = 0;\n", "\tmatched = 0;\n\tif ((option_mask32 & OPT_m) && max_matches == 0)\n\t\treturn 1;\n")
    grep = replace_once(grep, "\t} /* while (read line) */", '''\t} /* while (read line) */
    if (ferror(file)) {
        if (!SUPPRESS_ERR_MSGS)
            bb_simple_perror_msg(cur_file);
        open_errors = 1;
    }''')
    grep = replace_once(grep, "\tif (open_errors)\n\t\treturn 2;", '''    if (fflush(stdout) != 0 || ferror(stdout)) {
        bb_simple_perror_msg("standard output");
        return 2;
    }
\tif (open_errors)
\t\treturn 2;''')
    (args.output / "grep.c").write_text(grep, encoding="utf-8")
    ash = (args.source / "shell/ash.c").read_text(encoding="utf-8")
    printf = (args.source / "coreutils/printf.c").read_text(encoding="utf-8")
    # Successful stdio output can change errno. Keep numeric conversion
    # failures separate from libc's internal descriptor probes and writes.
    printf = replace_once(printf, "\tchar *have_prec, *have_width;",
                          "\tchar *have_prec, *have_width;\n\tint conversion_error = 0;")
    for conversion in ["llv = my_xstrtoll(skip_whitespace(argument));",
                       "llv = my_xstrtoull(skip_whitespace(argument));",
                       "dv = my_xstrtod(argument);"]:
        printf = replace_once(printf, conversion, conversion + "\n\t\tconversion_error = errno;")
    printf = replace_once(printf, "\tformat[fmt_length] = saved;",
                          "\tformat[fmt_length] = saved;\n\terrno = conversion_error;")
    (args.output / "printf.c").write_text(printf, encoding="utf-8")
    test = (args.source / "coreutils/test.c").read_text(encoding="utf-8")
    # Asyncify longjmp first returns while unwinding saved frames. Keep the
    # error helper epilogue, as for the ash translation unit below.
    test = test.replace("NORETURN", "NOINLINE")
    test = replace_once(test, 'bb_error_msg("%s: %s"+4, msg);', 'bb_error_msg("%s", msg);')
    (args.output / "test.c").write_text(test, encoding="utf-8")
    math = (args.source / "shell/math.c").read_text(encoding="utf-8")
    # Upstream suppresses ternary branches, but evaluates both sides of &&/||.
    # Each logical operator retains the enclosing suppression state until its
    # right operand has been parsed; restore it before combining the values.
    math = replace_once(math, "operator *opstack, *opstackptr;",
                        "operator *opstack, *opstackptr;\n    unsigned *logical_disabled;")
    marker = "opstackptr = opstack = alloca(expr_len * sizeof(opstack[0]));"
    math = replace_once(math, marker, marker + "\n        logical_disabled = alloca(expr_len * sizeof(logical_disabled[0]));")
    marker = "errmsg = arith_apply(math_state, prev_op, numstack, &numstackptr);"
    math = replace_once(math, marker, '''if (prev_op == TOK_AND || prev_op == TOK_OR)
                    math_state->evaluation_disabled = logical_disabled[opstackptr - opstack];
                ''' + marker)
    marker = "*opstackptr++ = lasttok = op;"
    math = replace_once(math, marker, '''if (op == TOK_AND || op == TOK_OR) {
            logical_disabled[opstackptr - opstack] = math_state->evaluation_disabled;
            if (!math_state->evaluation_disabled &&
                ((op == TOK_AND && numstackptr[-1].val == 0) ||
                 (op == TOK_OR && numstackptr[-1].val != 0)))
                math_state->evaluation_disabled = UINT_MAX;
        }
        ''' + marker)
    (args.output / "math.c").write_text(math, encoding="utf-8")
    # Error helpers return during an Asyncify unwind. They must not carry
    # C noreturn promises, which would let LLVM discard the unwind epilogue.
    ash = ash.replace("NORETURN", "NOINLINE")
    ash = ash.replace("execve(cmd, argv, envp);", "efi_ash_exec(cmd, argv, envp);")
    # Expanded here-documents already live in the current shell's stack.
    # Snapshot them as a read-only stream; no pipe writer or fork is needed.
    start = ash.index("static int\nopenhere(union node *redir)\n{")
    end = ash.index("\nstatic int\nopenredirect(", start)
    ash = ash[:start] + '''static int
openhere(union node *redir)
{
    char *text = redir->nhere.doc->narg.text;
    if (redir->type == NXHERE) {
        expandhere(redir->nhere.doc);
        text = stackblock();
    }
    int descriptor = efi_ash_input_snapshot(text, strlen(text));
    if (descriptor < 0)
        ash_msg_and_raise_perror("cannot create here-document input");
    return descriptor;
}
''' + ash[end:]
    # Saved parser/redirection descriptors must not enter fresh tasks.
    ash, duplicates = re.subn(r"fcntl\(([^,\n]+), F_DUPFD(_CLOEXEC)?, ([^)\n]+)\)",
                             lambda match: f"efi_ash_dupfd{'_cloexec' if match[2] else ''}({match[1]}, {match[3]})", ash)
    if duplicates != 5:
        raise ValueError("BusyBox descriptor duplication call sites changed")
    marker = "#if BASH_READ_D\n\t\tcase 'd':\n\t\t\tparams.opt_d = optionarg;\n\t\t\tbreak;\n#endif"
    if ash.count(marker) != 1:
        raise ValueError("Upstream ash read delimiter marker changed")
    ash = ash.replace(marker, "\t\tcase 'd':\n\t\t\tparams.opt_d = optionarg;\n\t\t\tbreak;")
    # Registered programs use the cooperative process ABI directly. This keeps
    # ordinary command execution in the parent and avoids cloning shell memory.
    start = ash.index("\tdefault: {\n", ash.index("\nevalcommand(union node *cmd, int flags)"))
    end = ash.index("\tcase CMDBUILTIN:", start)
    ash = ash[:start] + '''\tdefault: {
        char **environment = listvars(VEXPORT, VUNSET, varlist.list, NULL);
        int command_status = efi_ash_run(argv, environment);
        if (command_status < 0) {
            command_status = errno == ENOENT ? 127 : 126;
            ash_msg("%s: %s", argv[0], strerror(errno));
        }
        exitstatus = command_status;
        break;
    }
''' + ash[end:]
    # Builtins and functions retain upstream precedence. The registry query is
    # live and uncached, so loading a program after a shell starts is visible.
    marker = "\t/* We have to search path. */"
    if ash.count(marker) != 1:
        raise ValueError("Upstream ash command lookup marker changed")
    ash = ash.replace(marker, '''    if (efi_process_program_exists(name, strlen(name)) == 1) {
        entry->cmdtype = CMDNORMAL;
        entry->u.index = -1;
        return;
    }
''' + marker)
    # The process ABI requires unique environment names. Compare names rather
    # than NAME=value strings, and discard earlier command-local assignments.
    marker = "if (strcmp(lp1->text, vp->var_text) == 0)"
    if ash.count(marker) != 1:
        raise ValueError("Upstream ash environment overlay marker changed")
    ash = ash.replace(marker, "if (varcmp(lp1->text, vp->var_text) == 0)")
    marker = "\twhile (lp) {\n\t\tif (ep == stackstrend())"
    if ash.count(marker) != 1:
        raise ValueError("Upstream ash temporary environment marker changed")
    ash = ash.replace(marker, '''\twhile (lp) {
        struct strlist *later = lp->next;
        while (later && varcmp(lp->text, later->text) != 0)
            later = later->next;
        if (later) {
            lp = lp->next;
            continue;
        }
\t\tif (ep == stackstrend())''')
    # These values are used only while unwinding. Asyncify skips the caller's
    # remaining work until its driver regains control.
    returns = {
        'ash_msg_and_raise_error(err_msg, name);': 'return NULL;',
        'ash_msg_and_raise_error("can\'t open %s: %s", fname, errmsg(errno, "no such file"));': 'return -1;',
        'ash_msg_and_raise_error("%s: not found", basename);': 'return NULL;',
        'raise_exception(EXEXIT);': 'return 0;',
        'TRACE(("End of main reached\\n"));\n\texitshell();': 'return 0;',
    }
    for marker, returned in returns.items():
        if ash.count(marker) != 1:
            raise ValueError(f"Upstream ash return marker changed: {marker}")
        ash = ash.replace(marker, marker + "\n\t" + returned)
    start = ash.index("static int\nforkshell(struct job *jp, union node *n, int mode)\n{")
    end = ash.index("\n}\n", start) + 3
    ash = ash[:start] + '''static int
forkshell(struct job *jp, union node *n, int mode)
{
    int pid = efi_ash_spawn_node(n, mode);
    if (pid < 0) {
        if (jp)
            freejob(jp);
        ash_msg_and_raise_error("cannot start shell task: %d", pid);
    }
    forkparent(jp, n, mode, pid);
    return pid;
}
''' + ash[end:]
    replacements = {
        'static void\nsetsignal(int signo)\n{': '''static void
setsignal(int signo)
{
    if (trap[signo] != NULL)
        ash_msg_and_raise_error("asynchronous signals are unavailable");
}
''',
        'static void\nraise_interrupt(void)\n{': '''static void
raise_interrupt(void)
{
    pending_int = 0;
    exitstatus = 130;
    raise_exception(EXINT);
}
''',
        'static int\nwaitproc(int block, int *status)\n{': '''static int
waitproc(int block, int *status)
{
    for (;;) {
        int active = 0;
        for (unsigned index = 0; index < njobs; ++index) {
            struct job *job = &jobtab[index];
            if (!job->used)
                continue;
            for (unsigned process = 0; process < job->nprocs; ++process) {
                struct procstat *entry = &job->ps[process];
                if (entry->ps_status != -1)
                    continue;
                active = 1;
                int32_t code;
                int32_t result = efi_process_waitpid_nohang(entry->ps_pid, &code);
                if (result < 0)
                    __builtin_trap();
                if (result > 0) {
                    *status = ((uint32_t)code & 255) << 8;
                    return result;
                }
            }
        }
        if (!active) {
            errno = ECHILD;
            return -1;
        }
        if (!block)
            return 0;
    }
}
''',
        'static int FAST_FUNC\ntimescmd(int argc UNUSED_PARAM, char **argv UNUSED_PARAM)\n{': '''static int FAST_FUNC
timescmd(int argc UNUSED_PARAM, char **argv UNUSED_PARAM)
{
    ash_msg_and_raise_error("process CPU clocks are unavailable");
    return 2;
}
''',
    }
    for marker, replacement in replacements.items():
        start = ash.index(marker)
        end = ash.index("\n}\n", start) + 3
        ash = ash[:start] + replacement + ash[end:]
    start = ash.index("static int\nevalpipe(union node *n, int flags)\n{")
    end = ash.index("\n}\n", start) + 3
    ash = ash[:start] + '''static int
evalpipe(union node *n, int flags)
{
    int count = 0;
    for (struct nodelist *entry = n->npipe.cmdlist; entry; entry = entry->next)
        ++count;
    INT_OFF;
    struct job *job = makejob(count);
    int input = 0;
    for (struct nodelist *entry = n->npipe.cmdlist; entry; entry = entry->next) {
        int descriptors[2] = {-1, -1};
        if (entry->next && pipe(descriptors) < 0)
            ash_msg_and_raise_perror("cannot create shell pipe");
        int output = entry->next ? descriptors[1] : 1;
        int pid = efi_ash_spawn_task(entry->n, input, output,
            (flags & EV_TESTED) ? EFI_TASK_TESTED : 0);
        if (input != 0)
            close(input);
        if (entry->next)
            close(output);
        if (pid < 0) {
            if (entry->next)
                close(descriptors[0]);
            ash_msg_and_raise_error("cannot start pipeline task: %d", pid);
        }
        forkparent(job, entry->n, n->npipe.pipe_backgnd, pid);
        input = descriptors[0];
    }
    int status = n->npipe.pipe_backgnd ? 0 : waitforjob(job);
    INT_ON;
    return status;
}
''' + ash[end:]
    start = ash.index("static void FAST_FUNC\nevalbackcmd(union node *n, struct backcmd *result")
    end = ash.index("\n}\n", start) + 3
    ash = ash[:start] + '''static void FAST_FUNC
evalbackcmd(union node *node, struct backcmd *result)
{
    result->fd = -1;
    result->buf = NULL;
    result->nleft = 0;
    result->jp = NULL;
    if (!node)
        return;
    int descriptors[2];
    if (pipe(descriptors) < 0)
        ash_msg_and_raise_perror("cannot create command substitution pipe");
    struct job *job = makejob(1);
    int pid = efi_ash_spawn_task(node, 0, descriptors[1], EFI_TASK_CLEAR_ERREXIT);
    close(descriptors[1]);
    if (pid < 0) {
        close(descriptors[0]);
        freejob(job);
        ash_msg_and_raise_error("cannot start command substitution: %d", pid);
    }
    forkparent(job, node, FORK_NOJOB, pid);
    result->fd = descriptors[0];
    result->jp = job;
}
''' + ash[end:]
    start = ash.index("static int\nevalsubshell(union node *n, int flags)\n{")
    end = ash.index("\n}\n", start) + 3
    ash = ash[:start] + '''static int
evalsubshell(union node *node, int flags)
{
    int background = node->type == NBACKGND;
    errlinno = lineno = node->nredir.linno;
    expredir(node->nredir.redirect);
    if (!background && (flags & EV_EXIT) && !may_have_traps) {
        redirect(node->nredir.redirect, 0);
        evaltreenr(node->nredir.n, flags);
    }
    INT_OFF;
    struct job *job = makejob(1);
    unsigned task_flags = EFI_TASK_REDIR_EXPANDED;
    if (!background && (flags & EV_TESTED))
        task_flags |= EFI_TASK_TESTED;
    int pid = efi_ash_spawn_task(node, 0, 1, task_flags);
    if (pid < 0) {
        freejob(job);
        ash_msg_and_raise_error("cannot start subshell task: %d", pid);
    }
    forkparent(job, node, background, pid);
    int status = background ? 0 : waitforjob(job);
    INT_ON;
    return status;
}
''' + ash[end:]
    ash = ash.replace("\tif (iflag)\n\t\tsignal(SIGHUP, SIG_DFL);",
                      "\t/* No asynchronous signal state exists in the guest. */")
    marker = "/*\n * Main routine.  We initialize things"
    if ash.count(marker) != 1:
        raise ValueError("Upstream ash task include marker changed")
    ash = ash.replace(marker, '#include "tasks.inc"\n\n' + marker)
    marker = "\tlogin_sh = procargs(argv);"
    if ash.count(marker) != 1:
        raise ValueError("Upstream ash startup marker changed")
    ash = ash.replace(marker, '''    if (argv[1] && strcmp(argv[1], "--efi-task") == 0) {
        arg0 = argv[0];
        if (!argv[2] || !argv[3] || argv[4])
            ash_msg_and_raise_error("shell task requires size and descriptor arguments");
        efi_ash_task_entry(argv[2], argv[3]);
    }
''' + marker)
    (args.output / "ash.c").write_text(ash, encoding="utf-8")
    prepare_read(args.source, args.output)
    (args.output / "LICENSE").write_bytes((args.source / "LICENSE").read_bytes())


if __name__ == "__main__":
    main()
