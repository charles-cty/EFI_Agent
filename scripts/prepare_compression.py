"""Stage upstream gzip interfaces for cooperative WASI streams."""


def prepare_compression(source, output, replace_once):
    text = (source / 'archival/gzip.c').read_text(encoding='utf-8')
    text = replace_once(text, '#include "bb_archive.h"', '''#include "bb_archive.h"
struct globals;
static struct globals *ptr_to_globals;''')
    text = replace_once(text, '''\tif (len == (unsigned)(-1) || len == 0)
\t\treturn len;''', '''    if (len == (unsigned)-1)
        bb_simple_perror_msg_and_die(bb_msg_read_error);
    if (len == 0)
        return 0;''')
    (output / 'gzip.c').write_text(text, encoding='utf-8')
    # These common transformer helpers perform direct I/O. The following
    # upstream fork-transformer infrastructure is not used by gzip streams.
    text = (source / 'archival/libarchive/open_transformer.c').read_text(encoding='utf-8')
    marker = '\nvoid check_errors_in_children(int signo)'
    if text.count(marker) != 1:
        raise ValueError('Upstream transformer helper boundary changed')
    (output / 'transformer.c').write_text(text.split(marker)[0], encoding='utf-8')

    text = (source / 'archival/bbunzip.c').read_text(encoding='utf-8')
    # Gzip-only zcat decompresses directly. Multi-format auto-detection needs
    # separate cooperative children and is not part of this configuration.
    text = replace_once(text, '''\t\t\t} else {
\t\t\t\t/* "clever zcat" with FILE */''', '''\t\t\t}
#if SEAMLESS_COMPRESSION
            else {
\t\t\t\t/* "clever zcat" with FILE */''')
    text = replace_once(text, '''\t\t} else
\t\tif (option_mask32 & BBUNPK_SEAMLESS_MAGIC)''', '''#endif
        }
#if SEAMLESS_COMPRESSION
        else
\t\tif (option_mask32 & BBUNPK_SEAMLESS_MAGIC)''')
    text = replace_once(text, '\n\t\t/* Special cases: test, stdout */',
                        '\n#endif\n\t\t/* Special cases: test, stdout */')
    # Reject directories before creating a destination, or a later read error
    # could leave a misleading compressed-file entry behind.
    text = replace_once(text, '\t\t\t\tif (open_to_or_warn(STDIN_FILENO, filename, O_RDONLY, 0))', '''                if (!S_ISREG(stat_buf.st_mode)) {
                    errno = S_ISDIR(stat_buf.st_mode) ? EISDIR : EINVAL;
                    goto err_name;
                }
\t\t\t\tif (open_to_or_warn(STDIN_FILENO, filename, O_RDONLY, 0))''')
    text = replace_once(text, '\t\t\t\t\tutimes(new_name, times); /* ignoring errors */', '''                    if (utimes(new_name, times) < 0) {
                        bb_simple_perror_msg(new_name);
                        exitcode = 1;
                        goto free_name;
                    }''')
    (output / 'bbunzip.c').write_text(text, encoding='utf-8')

    text = (source / 'archival/libarchive/decompress_gunzip.c').read_text(encoding='utf-8')
    # Asyncify longjmp must return through its helper epilogue while unwinding.
    text = text.replace('NORETURN', 'NOINLINE')
    text = replace_once(text, '\tunsigned bytebuffer_size;       /* how much data is there (size <= max) */',
                        '\tunsigned bytebuffer_size;       /* how much data is there (size <= max) */\n\tsmallint input_error;')
    text = replace_once(text, '#define bytebuffer_size     (S()bytebuffer_size    )',
                        '#define bytebuffer_size     (S()bytebuffer_size    )\n#define input_error (S()input_error)')
    text = replace_once(text, '''\t\tif ((int)bytebuffer_size < 0) {
\t\t\tbb_simple_error_msg(bb_msg_read_error);''', '''\t\tif ((int)bytebuffer_size < 0) {
            input_error = 1;
\t\t\tbb_simple_error_msg(bb_msg_read_error);''')
    text = replace_once(text, '''\t} header;

\tBUILD_BUG_ON(sizeof(header) != 8);''', '''\t} header;
    uint32_t header_table[256];
    uint32_t header_crc;

\tBUILD_BUG_ON(sizeof(header) != 8);''')
    text = replace_once(text, '''\t/* Check the compression method */
\tif (header.formatted.gz_method != 8)''', '''    crc32_filltable(header_table, 0);
    header_crc = crc32_block_endian0(~0u, "\\x1f\\x8b", 2, header_table);
    header_crc = crc32_block_endian0(header_crc, header.raw, 8, header_table);
    /* Reserved flags and unknown methods are invalid RFC 1952 members. */
\tif (header.formatted.gz_method != 8 || (header.formatted.flags & 0xe0))''')
    text = replace_once(text, '''\t\textra_short = buffer_read_le_u16(PASS_STATE_ONLY);
\t\tif (!top_up(PASS_STATE extra_short))
\t\t\treturn 0;
\t\t/* Ignore extra field */
\t\tbytebuffer_offset += extra_short;''', '''        header_crc = crc32_block_endian0(header_crc,
            &bytebuffer[bytebuffer_offset], 2, header_table);
\t\textra_short = buffer_read_le_u16(PASS_STATE_ONLY);
        while (extra_short) {
            unsigned count = MIN(extra_short, bytebuffer_max);
            if (!top_up(PASS_STATE count))
                return 0;
            header_crc = crc32_block_endian0(header_crc,
                &bytebuffer[bytebuffer_offset], count, header_table);
            bytebuffer_offset += count;
            extra_short -= count;
        }''')
    text = replace_once(text, '''\t\t\t\tif (!top_up(PASS_STATE 1))
\t\t\t\t\treturn 0;
\t\t\t} while (bytebuffer[bytebuffer_offset++] != 0);''', '''\t\t\t\tif (!top_up(PASS_STATE 1))
\t\t\t\t\treturn 0;
                header_crc = crc32_block_endian0(header_crc,
                    &bytebuffer[bytebuffer_offset], 1, header_table);
\t\t\t} while (bytebuffer[bytebuffer_offset++] != 0);''')
    text = replace_once(text, '''\t\tif (!top_up(PASS_STATE 2))
\t\t\treturn 0;
\t\tbytebuffer_offset += 2;''', '''\t\tif (!top_up(PASS_STATE 2))
\t\t\treturn 0;
        if (buffer_read_le_u16(PASS_STATE_ONLY) != (uint16_t)~header_crc)
            return 0;''')
    text = replace_once(text, '''\tdefault:
\t\tabort_unzip(PASS_STATE_ONLY);
\t}
}''', '''\tdefault:
\t\tabort_unzip(PASS_STATE_ONLY);
        return 0;
\t}
}''')
    # A bad member must not become successful when another valid member follows.
    text = replace_once(text, '''\t\tbb_simple_error_msg("incorrect length");
\t\ttotal = -1;
\t}''', '''\t\tbb_simple_error_msg("incorrect length");
\t\ttotal = -1;
        goto ret;
\t}''')
    text = replace_once(text, ''' ret:
\tfree(bytebuffer);
\tDEALLOC_STATE;
\treturn total;''', ''' ret:
    if (input_error)
        total = -1;
\tfree(bytebuffer);
\tDEALLOC_STATE;
\treturn total;''')
    (output / 'decompress_gunzip.c').write_text(text, encoding='utf-8')
