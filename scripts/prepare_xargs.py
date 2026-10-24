"""Adapt upstream xargs to cooperative process hooks and finite guest input."""


def prepare_xargs(source, output, replace_once):
    text = (source / 'findutils/xargs.c').read_text(encoding='utf-8')
    text = replace_once(text, '#include "libbb.h"',
                        '#include "libbb.h"\n#include "xargs_support.h"')
    text = replace_once(text, '\tsmalluint process_stdin__state;',
                        '\tsmalluint process_stdin__state;\n\tsmalluint process_stdin__word;')
    # Preserve empty quoted words and diagnose an unfinished quote/escape at EOF.
    text = replace_once(text, '''\t\tif (c == EOF) {
\t\t\tif (p != s)
\t\t\t\tgoto close_word;''', '''        if (c == EOF) {
            if (state == QUOTE)
                bb_simple_error_msg_and_die("unmatched quote");
            if (state == BACKSLASH)
                bb_simple_error_msg_and_die("unfinished escape");
            if (p != s || G.process_stdin__word)
                goto close_word;''')
    text = replace_once(text, '\t\t\t\tif (p != s) {',
                        '\t\t\t\tif (p != s || G.process_stdin__word) {')
    text = replace_once(text, '\t\t\t\t\tstate = BACKSLASH;',
                        '\t\t\t\t\tstate = BACKSLASH;\n\t\t\t\t\tG.process_stdin__word = 1;')
    text = replace_once(text, '\t\t\t\t\tq = c;',
                        '\t\t\t\t\tq = c;\n\t\t\t\t\tG.process_stdin__word = 1;')
    text = replace_once(text, '\t\t\tstate = NORM;\n\t\t\tif (q)',
                        '\t\t\tstate = NORM;\n\t\t\tG.process_stdin__word = 0;\n\t\t\tif (q)')
    text = replace_once(text, '\t\trem = read_args(n_max_chars, n_max_arg, buf);',
                        '''        rem = read_args(n_max_chars, n_max_arg, buf);
        die_if_ferror(stdin, bb_msg_read_error);
        if (opt & OPT_TERMINATE) {
            if (*rem)
                bb_simple_error_msg_and_die("argument line too long");
            if (!(opt & (OPT_REPLSTR | OPT_REPLSTR1))
                && G.idx - initial_idx < n_max_arg) {
                size_t used = 0;
                for (int index = initial_idx; index < G.idx; ++index)
                    used += strlen(G.args[index]) + 1;
                if (used == (size_t)n_max_chars) {
                    int next;
                    do {
                        next = getchar();
                    } while (!(opt & OPT_ZEROTERM) && next != EOF && ISSPACE(next));
                    die_if_ferror(stdin, bb_msg_read_error);
                    if (next != EOF)
                        bb_simple_error_msg_and_die("argument line too long");
                }
            }
        }''')
    text = replace_once(text, '\t\tbb_simple_perror_msg(G.args[0]);\n\t\tstatus = (errno == ENOENT) ? 127 : 126;',
                        '\t\tint error = errno;\n\t\tbb_simple_perror_msg(G.args[0]);\n\t\tstatus = (error == ENOENT) ? 127 : 126;')
    text = replace_once(text, '\t\tG.args = NULL;\n\t\tG.argv = argv;', '''        if (!G.repl_str[0])
            bb_simple_error_msg_and_die("empty replacement string");
\t\tG.args = NULL;
\t\tG.argv = argv;''')
    text = replace_once(text, '\t\t\tif (!(opt & OPT_INTERACTIVE))\n\t\t\t\tbb_putchar_stderr(\'\\n\');', '''            if (!(opt & OPT_INTERACTIVE))
                bb_putchar_stderr('\\n');
            die_if_ferror(stderr, "standard error");''')
    # A 255 exit must not leave other parallel children running or lose its
    # abort status when a later child has an ordinary nonzero exit.
    marker = 'G.xargs_exitcode = 123;'
    if text.count(marker) != 2:
        raise ValueError('Upstream xargs child-status branches changed')
    text = text.replace(marker, 'if (G.xargs_exitcode < 123) G.xargs_exitcode = 123;')
    text = replace_once(text, '\txargs_exec(); /* final waitpid() loop */',
                        '\tdo { xargs_exec(); } while (G.running_procs != 0);')
    text = replace_once(text, '\treturn G.xargs_exitcode;',
                        '\tfflush_stdout_and_exit(G.xargs_exitcode);')
    (output / 'xargs.c').write_text(text, encoding='utf-8')
