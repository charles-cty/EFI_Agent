"""Stage upstream text applets with length-aware records and checked I/O."""


def prepare_sort(source, output, replace):
    text = (source / 'coreutils/sort.c').read_text(encoding='utf-8')
    text = replace(text, '#include "libbb.h"', '#include "libbb.h"\n#include "sort_support.h"\n#define HAVE_STRVERSCMP 1')
    text = replace(text, 'len = strlen(str);', 'len = sort_length(str);')
    text = replace(text, '''\t\t\t\t\twhile ((ch = str[end]) != '\\0') {
\t\t\t\t\t\t\tend++;
\t\t\t\t\t\tif (ch == key_separator)
\t\t\t\t\t\t\tbreak;
\t\t\t\t\t}''', '''                    ch = 0;
                    while (end < len) {
                        ch = str[end++];
                        if (ch == key_separator)
                            break;
                        ch = 0;
                    }''')
    text = replace(text, "while (str[end] != '\\0')", 'while (end < len)')
    text = replace(text, '\t\t\tfor (i = 1; i < key->range[2*j] + j; i++) {', '\t\t\tfor (i = 1; i < key->range[2*j] + j && end < len; i++) {')
    text = replace(text, '''\t\tend = key->range[3];
\t\tif (end > len) end = len;''', '''        end = sort_field_start(str, len, key->range[2], key_separator);
        if (flags & FLAG_bb)
            while (end < len && isspace((unsigned char)str[end]))
                ++end;
        unsigned offset = key->range[3];
        end += offset > (unsigned)(len - end) ? len - end : offset;''')
    text = replace(text, '''\t\tstart += key->range[1] - 1;
\t\tif (start > len) start = len;''', '''        unsigned offset = key->range[1] - 1;
        start += offset > (unsigned)(len - start) ? len - start : offset;''')
    text = replace(text, '\tstr = xstrndup(str+start, end-start);', '\tstr = sort_duplicate(str+start, end-start);')
    text = text.replace('for (start = end = 0; str[end]; end++)', 'for (start = end = 0; end < sort_length(str); end++)')
    text = text.replace("\t\tstr[start] = '\\0';", "\t\tstr[start] = '\\0';\n        sort_record(str)->length = start;")
    text = replace(text, 'for (i = 0; str[i]; i++)', 'for (i = 0; i < sort_length(str); i++)')
    start = text.index('#define GET_LINE(fp) \\\n')
    end = text.index('\n#else', start)
    text = text[:start] + '#define GET_LINE(fp) sort_read(fp, (option_mask32 & FLAG_z) ? 0 : \'\\n\')' + text[end:]
    text = replace(text, '\t\t\tretval = strcmp(x, y);', '\t\t\tretval = sort_compare(x, y);')
    text = replace(text, '\t\t\tstruct tm thyme;', '\t\t\tstruct tm thyme = {0};')
    start = text.index('#if ENABLE_FEATURE_SORT_BIG\nstatic int scale_suffix(')
    end = text.index('\n#endif', start) + len('\n#endif')
    text = text[:start] + text[end:]
    text = replace(text, '\t\tcase FLAG_g:\n\t\tcase FLAG_h: {', '''        case FLAG_h:
            retval = bb_compare_human(x, y);
            break;
        case FLAG_g: {''')
    start = text.index('\t\t\t\tif (flags & FLAG_h) {')
    end = text.index('\n\t\t\t\t/* Check for infinity.', start)
    text = text[:start] + text[end:]
    text = replace(text, '''\t\t\tdouble dx = atof(x);
\t\t\tdouble dy = atof(y);
\t\t\tretval = (dx > dy) ? 1 : ((dx < dy) ? -1 : 0);''', '\t\t\tretval = bb_compare_decimal(x, y);')
    text = text.replace('if (x != *(char **)xarg) free(x);', 'if (x != *(char **)xarg) sort_free(x);')
    text = text.replace('if (y != *(char **)yarg) free(y);', 'if (y != *(char **)yarg) sort_free(y);')
    start = text.index('\t\t\tuint32_t *p32;')
    end = text.index('\n\t\t\t/* Here, -r has no effect! */', start)
    text = text[:start] + '''            uint32_t x32 = sort_record(*(char **)xarg)->ordinal;
            uint32_t y32 = sort_record(*(char **)yarg)->ordinal;
            retval = (x32 > y32) - (x32 < y32);''' + text[end:]
    text = replace(text, 'retval = strcmp(*(char **)xarg, *(char **)yarg);', 'retval = sort_compare(*(char **)xarg, *(char **)yarg);')
    text = replace(text, '\tlu = strtoul(*str, str, 10);', '\terrno = 0;\n\tlu = strtoul(*str, str, 10);')
    text = replace(text, '(sizeof(long) > sizeof(int) && lu > INT_MAX) || !lu', 'errno || lu > INT_MAX || !lu')
    text = replace(text, '\t\t\tlines[linecount++] = line;', '\t\t\tsort_record(line)->ordinal = linecount;\n\t\t\tlines[linecount++] = line;')
    text = replace(text, '\t\tfclose_if_not_stdin(fp);', '''        die_if_ferror(fp, *argv);
        if (fclose_if_not_stdin(fp))
            bb_simple_perror_msg_and_die(*argv);''')
    start = text.index('\t/* For stable sort, store original line position beyond terminating NUL */')
    end = text.index('\n\t/* Perform the actual sort */', start)
    text = text[:start] + text[end:]
    text = replace(text, 'free(lines[i]);', 'sort_free(lines[i]);')
    text = replace(text, '''\t\tfor (i = 0; i < linecount; i++)
\t\t\tprintf("%s%c", lines[i], ch);''', '''        for (i = 0; i < linecount; i++) {
            fwrite(lines[i], 1, sort_length(lines[i]), stdout);
            putchar(ch);
        }''')
    (output / 'sort.c').write_text(text, encoding='utf-8')


def prepare_uniq(source, output, replace):
    text = (source / 'coreutils/uniq.c').read_text(encoding='utf-8')
    text = replace(text, '\tconst char *cur_compare;', '\tconst char *cur_compare;\n    size_t cur_length = 0, compare_length = 0;')
    text = replace(text, '\t\tconst char *old_compare;', '\t\tconst char *old_compare;\n        size_t old_length = cur_length, old_compare_length = compare_length;')
    text = replace(text, 'while ((cur_line = xmalloc_fgetline(stdin)) != NULL)', 'while ((cur_line = bb_read_record(stdin, eol, &cur_length)) != NULL)')
    text = replace(text, '''\t\t\t\tcur_compare = skip_whitespace(cur_compare);
\t\t\t\tcur_compare = skip_non_whitespace(cur_compare);''', '''                while (cur_compare < cur_line + cur_length && isspace((unsigned char)*cur_compare))
                    ++cur_compare;
                while (cur_compare < cur_line + cur_length && !isspace((unsigned char)*cur_compare))
                    ++cur_compare;''')
    text = replace(text, 'for (i = skip_fields; i; i--)', 'for (i = skip_fields; i && cur_compare < cur_line + cur_length; i--)')
    text = replace(text, 'for (i = skip_chars; *cur_compare && i; i--)', 'for (i = skip_chars; cur_compare < cur_line + cur_length && i; i--)')
    text = replace(text, '\t\t\tif (!old_line)', '''            compare_length = cur_length - (cur_compare - cur_line);
            if (compare_length > max_chars)
                compare_length = max_chars;
\t\t\tif (!old_line)''')
    text = replace(text, '''\t\t\tif ((opt & OPT_i)
\t\t\t\t? strncasecmp(old_compare, cur_compare, max_chars)
\t\t\t\t: strncmp(old_compare, cur_compare, max_chars)
\t\t\t) {''', '''            if (old_compare_length != compare_length
                || bb_compare_record(old_compare, cur_compare, compare_length, opt & OPT_i)) {''')
    text = replace(text, '\t\t\t\tprintf("%s%c", old_line, eol);', '\t\t\t\tfwrite(old_line, 1, old_length, stdout);\n                putchar(eol);')
    (output / 'uniq.c').write_text(text, encoding='utf-8')


def prepare_cut(source, output, replace):
    text = (source / 'coreutils/cut.c').read_text(encoding='utf-8')
    text = replace(text, '\tchar *line;\n', '\tchar *line;\n    size_t length;\n')
    text = replace(text, '\t\tunsigned cl_pos = 0;', '''        unsigned cl_pos = 0;
        if ((option_mask32 & (CUT_OPT_FIELDS_FLGS | CUT_OPT_REGEX_FLGS))
            && (option_mask32 & (CUT_OPT_SUPPRESS_FLGS | CUT_OPT_NOSORT_FLGS))
            && *delim != '\\n') {
            if (!shoe) {
                if (!memchr(line, (unsigned char)*delim, length))
                    goto next_line;
            } else {
                int match = regexec(&reg, line, 0, NULL, REG_NOTBOL | REG_NOTEOL);
                if (match == REG_NOMATCH)
                    goto next_line;
                if (match)
                    bb_simple_error_msg_and_die("regex match failed");
            }
        }''')
    text = replace(text, 'while ((line = xmalloc_fgetline(file)) != NULL)', 'while ((line = bb_read_record(file, \'\\n\', &length)) != NULL)')
    text = replace(text, 'int linelen = strlen(line);', '''int linelen = length;
        if (shoe && memchr(line, 0, length))
            bb_simple_error_msg_and_die("regex fields cannot contain NUL");''')
    text = replace(text, '\t\t\t\t\t\t\tend = uu + rr.rm_so;', '''                            if (rr.rm_eo == 0)
                                bb_simple_error_msg_and_die("regex delimiter matched empty string");
\t\t\t\t\t\t\tend = uu + rr.rm_so;''')
    text = replace(text, '\t\t\t\t\t\tif (!regexec(&reg, line+uu, 1, &rr, REG_NOTBOL|REG_NOTEOL)) {', '''                        int match = regexec(&reg, line+uu, 1, &rr, REG_NOTBOL|REG_NOTEOL);
                        if (match && match != REG_NOMATCH)
                            bb_simple_error_msg_and_die("regex match failed");
                        if (!match) {''')
    text = replace(text, '\tif (!odelim) odelim = (opt & CUT_OPT_REGEX_FLGS) ? " " : delim;', '''    if (!odelim) odelim = (opt & CUT_OPT_REGEX_FLGS) ? " " : delim;
    if ((opt & CUT_OPT_FIELDS_FLGS) && strlen(delim) != 1)
        bb_simple_error_msg_and_die("field delimiter must be one byte");''')
    text = replace(text, '\t\t\tputs(line);', '\t\t\tfwrite(line, 1, length, stdout);\n            putchar(\'\\n\');')
    text = replace(text, '''\t\t\t\tif (end != start || !shoe)
\t\t\t\t\tprintf("%s%.*s", out++ ? odelim : "", end-start, line + start);''', '''                if (end != start || !shoe) {
                    if (out++) fputs(odelim, stdout);
                    fwrite(line + start, 1, end - start, stdout);
                }''')
    text = replace(text, '\t\t\ts = xatoi_positive(ntok);', '''            s = xatoi_positive(ntok);
            if (s == 0)
                bb_simple_error_msg_and_die("positions start at 1");''')
    text = replace(text, '\t\t\te = xatoi_positive(ltok);', '''            e = xatoi_positive(ltok);
            if (e == 0)
                bb_simple_error_msg_and_die("positions start at 1");''')
    text = replace(text, 'else if (e < s)', 'else if (e <= s)')
    text = replace(text, '\t\t\tfclose_if_not_stdin(file);', '''            if (ferror(file)) {
                bb_simple_perror_msg(*argv);
                retval = EXIT_FAILURE;
            }
            if (fclose_if_not_stdin(file)) {
                bb_simple_perror_msg(*argv);
                retval = EXIT_FAILURE;
            }''')
    text = replace(text, '\t\tfree(orig_line);\n\t}\n}', '\t\tfree(orig_line);\n\t}\n    if (shoe) regfree(&reg);\n}')
    (output / 'cut.c').write_text(text, encoding='utf-8')


def prepare_text_applets(source, output, replace):
    prepare_sort(source, output, replace)
    prepare_uniq(source, output, replace)
    prepare_cut(source, output, replace)
