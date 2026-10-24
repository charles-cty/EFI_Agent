# /// script
# requires-python = ">=3.11"
# ///
"""Prepare upstream BusyBox sources for the Windows-native WASI build."""
import argparse
from pathlib import Path
import re
from prepare_text_applets import prepare_text_applets
from prepare_xargs import prepare_xargs
from prepare_compression import prepare_compression
from prepare_unzip import prepare_unzip
from prepare_tar import prepare_tar


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
    "CP", "MV", "LN", "FEATURE_PRESERVE_HARDLINKS", "FEATURE_CP_LONG_OPTIONS", "FEATURE_VERBOSE_CP_MESSAGE",
    "XARGS", "FEATURE_XARGS_SUPPORT_CONFIRMATION", "FEATURE_XARGS_SUPPORT_QUOTES",
    "FEATURE_XARGS_SUPPORT_TERMOPT", "FEATURE_XARGS_SUPPORT_ZERO_TERM",
    "FEATURE_XARGS_SUPPORT_REPL_STR", "FEATURE_XARGS_SUPPORT_PARALLEL",
    "FEATURE_XARGS_SUPPORT_ARGS_FILE", "ECHO",
    "GZIP", "GUNZIP", "ZCAT", "FEATURE_GZIP_DECOMPRESS",
    "FEATURE_GZIP_LONG_OPTIONS", "FEATURE_GUNZIP_LONG_OPTIONS", "FEATURE_GZIP_LEVELS",
    "UNZIP", "FEATURE_UNZIP_CDF",
    "TAR", "FEATURE_TAR_CREATE", "FEATURE_TAR_LONG_OPTIONS", "FEATURE_TAR_FROM",
    "FEATURE_TAR_GNU_EXTENSIONS", "FEATURE_TAR_NOPRESERVE_TIME",
    "FEATURE_SEAMLESS_GZ", "FEATURE_TAR_AUTODETECT",
    "DU", "FEATURE_DU_DEFAULT_BLOCKSIZE_1K", "DF", "FEATURE_DF_FANCY",
    "MONOTONIC_SYSCALL",
    "DATE", "FEATURE_DATE_ISOFMT", "FEATURE_DATE_NANO",
    "SORT", "FEATURE_SORT_BIG", "UNIQ", "CUT", "FEATURE_CUT_REGEX",
    "TR", "FEATURE_TR_CLASSES", "FEATURE_TR_EQUIV",
    "OD", "HEXDUMP", "BASE64", "SHA256SUM", "FEATURE_MD5_SHA1_SUM_CHECK", "REALPATH",
    "STAT", "FEATURE_STAT_FORMAT", "FEATURE_STAT_FILESYSTEM", "WHICH",
    "FIND", "FEATURE_FIND_PRINT0", "FEATURE_FIND_MTIME", "FEATURE_FIND_ATIME",
    "FEATURE_FIND_CTIME", "FEATURE_FIND_MMIN", "FEATURE_FIND_AMIN",
    "FEATURE_FIND_CMIN", "FEATURE_FIND_PERM", "FEATURE_FIND_TYPE",
    "FEATURE_FIND_EXECUTABLE", "FEATURE_FIND_XDEV", "FEATURE_FIND_MAXDEPTH",
    "FEATURE_FIND_NEWER", "FEATURE_FIND_INUM", "FEATURE_FIND_SAMEFILE",
    "FEATURE_FIND_NOT",
    "FEATURE_FIND_DEPTH", "FEATURE_FIND_PAREN", "FEATURE_FIND_SIZE",
    "FEATURE_FIND_PRUNE", "FEATURE_FIND_QUIT", "FEATURE_FIND_DELETE",
    "FEATURE_FIND_EMPTY", "FEATURE_FIND_PATH", "FEATURE_FIND_REGEX",
    "FEATURE_FIND_LINKS",
    "LS", "FEATURE_LS_FILETYPES", "FEATURE_LS_FOLLOWLINKS", "FEATURE_LS_WIDTH",
    "FEATURE_LS_RECURSIVE", "FEATURE_LS_SORTFILES", "FEATURE_LS_TIMESTAMPS",
    "FEATURE_LS_USERNAME", "FEATURE_HUMAN_READABLE",
    "UNICODE_SUPPORT", "UNICODE_COMBINING_WCHARS", "UNICODE_WIDE_WCHARS",
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
    text = (source / "coreutils/ln.c").read_text(encoding="utf-8")
    text = replace_once(text, 'int ln_main(int argc, char **argv) MAIN_EXTERNALLY_VISIBLE;', '''static int efi_same_entry(const char *source, const char *destination)
{
    char *source_path = xstrdup(source);
    char *destination_path = xstrdup(destination);
    struct stat source_parent, destination_parent;
    int same = strcmp(bb_get_last_path_component_strip(source_path),
                      bb_get_last_path_component_strip(destination_path)) == 0
        && stat(dirname(source_path), &source_parent) == 0
        && stat(dirname(destination_path), &destination_parent) == 0
        && source_parent.st_dev == destination_parent.st_dev
        && source_parent.st_ino == destination_parent.st_ino;
    free(source_path);
    free(destination_path);
    return same;
}

int ln_main(int argc, char **argv) MAIN_EXTERNALLY_VISIBLE;''')
    text = replace_once(text, '\t\tif (opts & LN_BACKUP) {', '''        /* Never remove the source when force or backup names the same inode. */
        if (opts & (LN_FORCE | LN_BACKUP)) {
            struct stat source_info, destination_info;
            if (lstat(*argv, &source_info) == 0 && lstat(src, &destination_info) == 0
                && source_info.st_dev == destination_info.st_dev
                && source_info.st_ino == destination_info.st_ino
                && efi_same_entry(*argv, src)) {
                bb_error_msg("'%s' and '%s' are the same file", *argv, src);
                status = EXIT_FAILURE;
                free(src_name);
                continue;
            }
        }
\t\tif (opts & LN_BACKUP) {''')
    text = replace_once(text, '''\t\tif (opts & LN_VERBOSE) {
\t\t\tprintf("'%s' -> '%s'\\n", src, *argv);
\t\t}

''', '')
    text = replace_once(text, '\t\t\tbackup = xasprintf("%s%s", src, suffix);', '''            backup = xasprintf("%s%s", src, suffix);
            if (!suffix[0] || (lstat(*argv, &statbuf) == 0
                              && efi_same_entry(*argv, backup))) {
                bb_error_msg("backup '%s' would replace a source or destination", backup);
                status = EXIT_FAILURE;
                free(backup);
                free(src_name);
                continue;
            }''')
    text = replace_once(text, '\t\t\t\tfree(backup);\n\t\t\t\tcontinue;',
                        '\t\t\t\tfree(backup);\n\t\t\t\tfree(src_name);\n\t\t\t\tcontinue;')
    text = replace_once(text, '\t\t\tstatus = EXIT_FAILURE;\n\t\t}\n\n\t\tfree(src_name);', '''            status = EXIT_FAILURE;
        } else if (opts & LN_VERBOSE) {
            printf("'%s' -> '%s'\\n", src, *argv);
        }

        free(src_name);''')
    text = replace_once(text, '\treturn status;', '\tfflush_stdout_and_exit(status);')
    (output / "ln.c").write_text(text, encoding="utf-8")
    for name in ("cp", "mv"):
        text = (source / f"coreutils/{name}.c").read_text(encoding="utf-8")
        text = replace_once(text, '\treturn status;', '\tfflush_stdout_and_exit(status);')
        if name == "mv":
            text = replace_once(text, '\tint copy_flag = 0;', '\tint copy_flag = 0;\n\tint moved = 0;')
            text = replace_once(text, '\tdo {\n\t\tdest =', '\tdo {\n\t\tmoved = 0;\n\t\tdest =')
            text = replace_once(text, '\t\t\t\t && (remove_file(*argv, FILEUTILS_RECUR | FILEUTILS_FORCE) >= 0)\n\t\t\t\t) {',
                                '\t\t\t\t && (remove_file(*argv, FILEUTILS_RECUR | FILEUTILS_FORCE) >= 0)\n\t\t\t\t) {\n\t\t\t\t\tmoved = 1;')
            text = replace_once(text, '\t\t\tstatus = 1;\n\t\t}',
                                '\t\t\tstatus = 1;\n\t\t} else {\n\t\t\tmoved = 1;\n\t\t}')
            text = replace_once(text, '\t\tif (flags & OPT_VERBOSE) {',
                                '\t\tif (moved && (flags & OPT_VERBOSE)) {')
        (output / f"{name}.c").write_text(text, encoding="utf-8")

    copy = (source / "libbb/copy_file.c").read_text(encoding="utf-8")
    copy = replace_once(copy, '\t\tif (dp == NULL) {\n\t\t\tretval = -1;', '''        if (dp == NULL) {
            bb_perror_msg("can't open '%s'", source);
            retval = -1;''')
    copy = replace_once(copy, '\t\twhile ((d = readdir(dp)) != NULL) {', '''        for (;;) {
            errno = 0;
            d = readdir(dp);
            if (d == NULL) {
                if (errno) {
                    bb_perror_msg("can't read '%s'", source);
                    retval = -1;
                }
                break;
            }''')
    copy = replace_once(copy, '\t\tclosedir(dp);', '''        if (closedir(dp) < 0) {
            bb_perror_msg("can't close '%s'", source);
            retval = -1;
        }''')
    copy = replace_once(copy, '\t\t/* retval = -1; - WRONG! copy *WAS* made */',
                        '\t\tretval = -1;')
    copy = replace_once(copy, '\t\tsrc_fd = open_or_warn(source, O_RDONLY);', '''        /* POSIX truncation follows an existing destination symlink. Compare
         * its target before opening it, so an alias cannot erase the source. */
        if (!ENABLE_FEATURE_NON_POSIX_CP && !(flags & FILEUTILS_INTERACTIVE)
            && stat(dest, &dest_stat) == 0
            && source_stat.st_dev == dest_stat.st_dev
            && source_stat.st_ino == dest_stat.st_ino) {
            bb_error_msg("'%s' and '%s' are the same file", source, dest);
            return -1;
        }
\t\tsrc_fd = open_or_warn(source, O_RDONLY);''')
    copy = replace_once(copy, '''\t\tif (lpath) {
\t\t\tint r = symlink(lpath, dest);''', '''        if (!lpath)
            return -1;
        if (lpath) {
            int r = symlink(lpath, dest);''')
    copy = replace_once(copy, '\tif (flags & FILEUTILS_VERBOSE) {',
                        '\tif (retval == 0 && (flags & FILEUTILS_VERBOSE)) {')
    for attribute in ("times", "ownership", "permissions"):
        marker = f'bb_perror_msg("can\'t preserve %s of \'%s\'", "{attribute}", dest);'
        copy = copy.replace(marker, '{ ' + marker + ' retval = -1; }')
    (output / "copy_file.c").write_text(copy, encoding="utf-8")

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


def prepare_du(source: Path, output: Path):
    text = (source / 'coreutils/du.c').read_text(encoding='utf-8')
    # Keep upstream's ceiling for explicit units.
    text = replace_once(text, '# if ENABLE_DESKTOP', '# if 1')
    text = replace_once(text, '\t\twhile ((entry = readdir(dir))) {', '''        for (;;) {
            errno = 0;
            entry = readdir(dir);
            if (!entry) {
                if (errno) {
                    bb_simple_perror_msg(filename);
                    G.status = EXIT_FAILURE;
                }
                break;
            }''')
    text = replace_once(text, '\t\tclosedir(dir);', '''        if (closedir(dir) < 0) {
            bb_simple_perror_msg(filename);
            G.status = EXIT_FAILURE;
        }''')
    (output / 'du.c').write_text(text, encoding='utf-8')


def prepare_date(source: Path, output: Path):
    """Keep upstream date parsing/formatting and reject unavailable clock writes."""
    helpers = (source / 'libbb/time.c').read_text(encoding='utf-8')
    helpers = replace_once(helpers, '''\ttime_t t = mktime(ptm);
\tif (t == (time_t) -1L) {''', '''    errno = 0;
    time_t t = mktime(ptm);
    if (t == (time_t)-1 && errno == EOVERFLOW) {''')
    (output / 'time.c').write_text(helpers, encoding='utf-8')
    text = (source / "coreutils/date.c").read_text(encoding="utf-8")
    text = replace_once(text, '# include <sys/syscall.h>', '# include <locale.h>')
    text = replace_once(text, '\t\tclock_gettime(CLOCK_REALTIME, &ts);', '''        if (clock_gettime(CLOCK_REALTIME, &ts) < 0)
            bb_simple_perror_msg_and_die("clock_gettime");''')
    text = replace_once(text, '\tlocaltime_r(&ts.tv_sec, &tm_time);', '''    if (!localtime_r(&ts.tv_sec, &tm_time))
        bb_simple_perror_msg_and_die("localtime");''')
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


def prepare_dump(source: Path, output: Path):
    dump = (source / "libbb/dump.c").read_text(encoding="utf-8")
    dump = replace_once(dump, '#include "libbb.h"', '''#include "libbb.h"
#define move_from_unaligned16(value, pointer) memcpy(&(value), (pointer), 2)
#define move_from_unaligned32(value, pointer) memcpy(&(value), (pointer), 4)
#define move_from_unaligned64(value, pointer) memcpy(&(value), (pointer), 8)
void FAST_FUNC xfstat(int fd, struct stat *stat_buf, const char *errmsg);
char *FAST_FUNC strcpy_and_process_escape_sequences(char *dst, const char *src);''')
    dump = replace_once(dump, '\t\t\t\tbb_simple_perror_msg(dumper->argv[-1]);', '''                bb_simple_perror_msg("read");
                dumper->exitval = 1;''')
    dump = replace_once(dump, '\treturn dumper->exitval;', '''    int output_error = fflush(stdout) != 0;
    if (ferror(stdout) || output_error) {
        bb_simple_perror_msg("standard output");
        dumper->exitval = 1;
    }
    return dumper->exitval;''')
    dump += '''\nvoid FAST_FUNC xfstat(int fd, struct stat *stat_buf, const char *errmsg)
{
    if (fstat(fd, stat_buf))
        bb_simple_perror_msg_and_die(errmsg);
}
'''
    (output / "dump.c").write_text(dump, encoding="utf-8")


def prepare_base64(source: Path, output: Path):
    text = (source / "coreutils/uudecode.c").read_text(encoding="utf-8")
    text = replace_once(text, '\tfflush_stdout_and_exit_SUCCESS();', '''    die_if_ferror(src_stream, bb_msg_read_error);
    if (fclose_if_not_stdin(src_stream) != 0)
        bb_simple_perror_msg_and_die(bb_msg_read_error);
    fflush_stdout_and_exit_SUCCESS();''')
    text = replace_once(text, '\t\t\t/* Encode the buffer we just read in */', '''            die_if_ferror_stdout();
			/* Encode the buffer we just read in */''')
    (output / "base64.c").write_text(text, encoding="utf-8")
    text = (source / "libbb/uuencode.c").read_text(encoding="utf-8")
    text = replace_once(text, '\t\t\tint ch = fgetc(src_stream);', '''            int ch = fgetc(src_stream);
            if (ch == EOF)
                die_if_ferror(src_stream, bb_msg_read_error);''')
    text = replace_once(text, '\t\tfwrite(buf, (out_tail - buf), 1, dst_stream);', '''        size_t count = out_tail - buf;
        if (fwrite(buf, 1, count, dst_stream) != count)
            bb_simple_perror_msg_and_die(bb_msg_standard_output);''')
    (output / "uuencode.c").write_text(text, encoding="utf-8")


def prepare_sha256sum(source: Path, output: Path):
    text = (source / "coreutils/md5_sha1_sum.c").read_text(encoding="utf-8")
    text = replace_once(text, '''\tif (src_fd != STDIN_FILENO) {
\t\tclose(src_fd);
\t}''', '''    if (src_fd != STDIN_FILENO && close(src_fd) < 0) {
        bb_perror_msg("can't close '%s'", filename);
        free(hash_value);
        hash_value = NULL;
    }''')
    text = replace_once(text, '\treturn return_value;\n}', '''    if (fflush(stdout) != 0 || ferror(stdout)) {
        bb_simple_perror_msg("standard output");
        return_value = EXIT_FAILURE;
    }
    return return_value;
}''')
    (output / "md5_sha1_sum.c").write_text(text, encoding="utf-8")


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
                 "mkdir", "rmdir", "rm", "touch", "cp", "mv", "du", "ln", "df")),
             args.source / "libbb/copy_file.c", args.source / "libbb/inode_hash.c",
             args.source / "findutils/xargs.c",
             args.source / "archival/gzip.c", args.source / "archival/bbunzip.c",
             args.source / "archival/libarchive/decompress_gunzip.c",
             args.source / "include/bb_archive.h",
             args.source / "archival/unzip.c",
             args.source / "archival/tar.c",
             args.source / "archival/libarchive/get_header_tar.c",
             args.source / "archival/libarchive/data_extract_all.c",
             args.source / "archival/libarchive/header_verbose_list.c",
             args.source / "coreutils/od.c", args.source / "util-linux/hexdump.c",
             args.source / "libbb/dump.c", args.source / "libbb/xatonum.c",
             args.source / "coreutils/uudecode.c", args.source / "libbb/uuencode.c",
             args.source / "coreutils/md5_sha1_sum.c", args.source / "libbb/hash_md5_sha.c",
             args.source / "coreutils/realpath.c",
             args.source / "coreutils/stat.c",
             args.source / "findutils/find.c",
             args.source / "libbb/recursive_action.c",
             args.source / "libbb/replace.c",
             args.source / "coreutils/ls.c", args.source / "libbb/unicode.c",
             args.source / "libbb/printable_string.c",
             args.source / "coreutils/date.c", args.source / "libbb/compare_string_array.c",
             *(args.source / f"coreutils/{name}.c" for name in ("sort", "uniq", "cut", "tr")),
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
                  "#define CONFIG_GZIP_FAST 0",
                  "#define CONFIG_SHA3_SMALL 0", "#define CONFIG_MD5_SMALL 0",
                  "#define CONFIG_SHA1_SMALL 0",
                  "#define CONFIG_LAST_SUPPORTED_WCHAR 0x2ffff",
                  "#define CONFIG_SUBST_WCHAR 63",
                  "#define CONFIG_FEATURE_COPYBUF_KB 4", "#endif", ""])
    (args.output / "ash_config.h").write_text("\n".join(lines), encoding="utf-8")
    (args.output / "NUM_APPLETS.h").write_text("#define NUM_APPLETS 1\n", encoding="utf-8")
    prepare_streams(args.source, args.output)
    prepare_mutations(args.source, args.output)
    prepare_xargs(args.source, args.output, replace_once)
    prepare_compression(args.source, args.output, replace_once)
    prepare_unzip(args.source, args.output, replace_once)
    prepare_tar(args.source, args.output, replace_once)
    prepare_du(args.source, args.output)
    df = (args.source / "coreutils/df.c").read_text(encoding="utf-8")
    df = replace_once(df, '\treturn status;', '\tfflush_stdout_and_exit(status);')
    (args.output / "df.c").write_text(df, encoding="utf-8")
    mount = (args.source / "libbb/find_mount_point.c").read_text(encoding="utf-8")
    # WASI has no UBI volumes or st_rdev. Character devices belong to devfs;
    # use their containing filesystem identity as for other ordinary paths.
    mount = replace_once(mount,
        '\t/* Why S_ISCHR? - UBI volumes use char devices, not block */\n'
        '\tif (S_ISBLK(s.st_mode) || S_ISCHR(s.st_mode)) {',
        '\tif (S_ISBLK(s.st_mode)) {')
    (args.output / "find_mount_point.c").write_text(mount, encoding="utf-8")
    prepare_date(args.source, args.output)
    prepare_text_applets(args.source, args.output, replace_once)
    prepare_dump(args.source, args.output)
    prepare_base64(args.source, args.output)
    prepare_sha256sum(args.source, args.output)
    stat = (args.source / "coreutils/stat.c").read_text(encoding="utf-8")
    stat = replace_once(stat, '#define FS_TYPE_LIST \\\n',
                        '#define FS_TYPE_LIST \\\nFS_TYPE(0x45464941, "efi-agent") \\\nFS_TYPE(0x1373, "devfs") \\\n')
    # WASI time_t is 64-bit while long is 32-bit.
    stat = stat.replace('TYPE_SIGNED(time_t) ? "ld" : "lu"',
                        'TYPE_SIGNED(time_t) ? "lld" : "llu"')
    for field in ('atime', 'mtime', 'ctime'):
        stat = replace_once(stat, f'(long) statbuf->st_{field}',
                            f'(long long) statbuf->st_{field}')
    stat = replace_once(stat, "\treturn (ok ? EXIT_SUCCESS : EXIT_FAILURE);", '''    if (fflush(stdout) != 0 || ferror(stdout)) {
        bb_simple_perror_msg("standard output");
        ok = 0;
    }
    return (ok ? EXIT_SUCCESS : EXIT_FAILURE);''')
    (args.output / "stat.c").write_text(stat, encoding="utf-8")
    ls = (args.source / "coreutils/ls.c").read_text(encoding="utf-8")
    ls = replace_once(ls, '#include "common_bufsiz.h"',
                      '#include "common_bufsiz.h"\n#define HAVE_STRVERSCMP 1')
    (args.output / "ls.c").write_text(ls, encoding="utf-8")
    hexdump = (args.source / "util-linux/hexdump.c").read_text(encoding="utf-8")
    hexdump = replace_once(hexdump, '#include "libbb.h"', '''#include "libbb.h"
#define OFF_T_MAX ((off_t)~((off_t)1 << (sizeof(off_t) * 8 - 1)))
''')
    hexdump = replace_once(hexdump, 'fp = xfopen_for_read(name);', 'fp = xfopen(name, "r");')
    hexdump = replace_once(hexdump, '\tfclose(fp);', '''    int read_error = ferror(fp);
    int close_error = fclose(fp);
    if (read_error || close_error)
        bb_simple_perror_msg_and_die(name);''')
    (args.output / "hexdump.c").write_text(hexdump, encoding="utf-8")
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
    executable = (args.source / "libbb/executable.c").read_text(encoding="utf-8")
    executable = executable[:executable.index("#if ENABLE_FEATURE_PREFER_APPLETS")]
    # WASI access(X_OK) has no execute-right bit. Use the enforced guest owner mode.
    executable = replace_once(executable,
        "return (!access(name, X_OK) && !stat(name, &s) && S_ISREG(s.st_mode));",
        "return (!stat(name, &s) && (s.st_mode & S_IXUSR) && S_ISREG(s.st_mode));")
    (args.output / "executable.c").write_text(executable, encoding="utf-8")
    which = (args.source / "debianutils/which.c").read_text(encoding="utf-8")
    which = replace_once(which, "return status;", "fflush_stdout_and_exit(status);")
    (args.output / "which.c").write_text(which, encoding="utf-8")
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
        char *original_name = argv[0];
        char *resolved_name = NULL;
        if (!strchr(original_name, '/')) {
            const char *search_path = path;
            int index = cmdentry.u.index;
            while (padvance(&search_path, original_name) >= 0) {
                if (index-- == 0 && pathopt == NULL) {
                    resolved_name = strchr(stackblock(), '/')
                        ? xstrdup(stackblock()) : xasprintf("./%s", (char *)stackblock());
                    argv[0] = resolved_name;
                    break;
                }
            }
        }
        int command_status = efi_ash_run(argv, environment);
        argv[0] = original_name;
        free(resolved_name);
        if (command_status < 0) {
            command_status = errno == ENOENT ? 127 : 126;
            ash_msg("%s: %s", argv[0], strerror(errno));
        }
        exitstatus = command_status;
        break;
    }
''' + ash[end:]
    # Keep upstream PATH lookup and hashing. The filesystem exposes installed
    # command entries, and execution uses the selected PATH component above.
    ash = replace_once(ash, "if (!S_ISREG(statb.st_mode))",
                       "if (!S_ISREG(statb.st_mode) || !(statb.st_mode & S_IXUSR))")
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
    ash = ash.replace(marker, '#include "tasks.inc"\n#include "session.inc"\n\n' + marker)
    marker = "\tlogin_sh = procargs(argv);"
    if ash.count(marker) != 1:
        raise ValueError("Upstream ash startup marker changed")
    ash = ash.replace(marker, '''    efi_session = argv[1] && strcmp(argv[1], "--efi-session") == 0;
    if (efi_session) {
        if (argv[2])
            ash_msg_and_raise_error("code session does not accept arguments");
        argv[1] = NULL;
    }
    if (argv[1] && strcmp(argv[1], "--efi-task") == 0) {
        arg0 = argv[0];
        if (!argv[2] || !argv[3] || argv[4])
            ash_msg_and_raise_error("shell task requires size and descriptor arguments");
        efi_ash_task_entry(argv[2], argv[3]);
    }
''' + marker)
    ash = replace_once(ash, "\tstate = 4;\n\tif (minusc) {",
                       "\tstate = 4;\n\tif (efi_session) efi_ash_session_loop();\n\tif (minusc) {")
    (args.output / "ash.c").write_text(ash, encoding="utf-8")
    prepare_read(args.source, args.output)
    (args.output / "LICENSE").write_bytes((args.source / "LICENSE").read_bytes())


if __name__ == "__main__":
    main()
