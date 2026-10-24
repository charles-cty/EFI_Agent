"""Stage upstream tar with anchored extraction and checked metadata."""


def prepare_tar(source, output, replace_once):
    text = (source / 'archival/tar.c').read_text(encoding='utf-8')
    text = replace_once(text, '#include "bb_archive.h"', '''#include "bb_archive.h"
#include "tar_support.h"
#undef SEAMLESS_COMPRESSION
#define SEAMLESS_COMPRESSION 1''')
    start = text.index('static void NOINLINE vfork_compressor(')
    end = text.index('\n# endif /* SEAMLESS_COMPRESSION */', start)
    text = text[:start] + '''static void NOINLINE vfork_compressor(int fd, const char *program)
{
    if (strcmp(program, "gzip") != 0)
        bb_simple_error_msg_and_die("unsupported tar compression");
    efi_tar_transform(fd, true);
}
''' + text[end:]
    text = replace_once(text, 'safe_waitpid(-1, &status, 0)', 'efi_tar_wait(&status)')
    text = replace_once(text, '\t                     | ARCHIVE_UNLINK_OLD;',
                        '\t                     | ARCHIVE_UNLINK_OLD | ARCHIVE_DONT_RESTORE_OWNER;')
    text = replace_once(text, '\t/* Zero processed headers (== empty file) is not a valid tarball.',
                        '''    tar_handle->seek = seek_by_read;
    if ((opt & OPT_EXTRACT) && !(opt & OPT_2STDOUT))
        efi_archive_begin();

\t/* Zero processed headers (== empty file) is not a valid tarball.''')
    text = replace_once(text, '\treturn bb_got_signal;', '''    xclose(tar_handle->src_fd);
    efi_archive_end();
    fflush_stdout_and_exit(bb_got_signal);''')
    text = replace_once(text, '\t\treturn writeTarFile(tbInfo,',
                        '\t\tint result = writeTarFile(tbInfo,')
    text = replace_once(text, '\t\t\t\tzipMode);',
                        '\t\t\t\tzipMode);\n        fflush_stdout_and_exit(result);')
    text = replace_once(text, '\tclose(tbInfo->tarFd);', '\txclose(tbInfo->tarFd);')
    text = replace_once(text, '\t\tclose(inputFileFd);', '\t\txclose(inputFileFd);')
    text = replace_once(text, '\t\tfclose(src_stream);', '''        if (ferror(src_stream))
            bb_simple_perror_msg_and_die("file list");
        if (fclose(src_stream) != 0)
            bb_simple_perror_msg_and_die("file list");''')
    (output / 'tar.c').write_text(text, encoding='utf-8')

    text = (source / 'archival/libarchive/data_extract_all.c').read_text(encoding='utf-8')
    text = replace_once(text, '#include "bb_archive.h"',
                        '#include "bb_archive.h"\nvoid efi_tar_attributes(const char *, const file_header_t *, unsigned);')
    text = replace_once(text, '\t\t\tbb_make_directory(dst_name, -1, FILEUTILS_RECUR);', '''            if (bb_make_directory(dst_name, -1, FILEUTILS_RECUR) != 0)
                bb_simple_error_msg_and_die("archive parent directory");''')
    text = replace_once(text, '\t\tclose(dst_fd);', '\t\txclose(dst_fd);')
    text = replace_once(text, 'mkdir(dst_name, file_header->mode)',
                        'mkdir(dst_name, file_header->mode | 0700)')
    text = replace_once(text, '\t\t\tbb_perror_msg("can\'t make dir %s", dst_name);',
                        '\t\t\tbb_perror_msg_and_die("can\'t make dir %s", dst_name);')
    text = replace_once(text, '\t\t\tbb_perror_msg("can\'t create node %s", dst_name);',
                        '\t\t\tbb_perror_msg_and_die("can\'t create node %s", dst_name);')
    start = text.index('\n\tif (!S_ISLNK(file_header->mode)) {')
    end = text.index('\n ret: ;', start)
    text = text[:start] + '''
    efi_tar_attributes(dst_name, file_header, archive_handle->ah_flags);
''' + text[end:]
    (output / 'data_extract_all.c').write_text(text, encoding='utf-8')

    text = (source / 'archival/libarchive/get_header_tar.c').read_text(encoding='utf-8')
    text = replace_once(text, '#include "bb_archive.h"',
                        '#include "bb_archive.h"\n#include "tar_support.h"')
    # Keep short/read failures observable even after valid members or zero blocks.
    text = replace_once(text, '''\t\t\twhile (full_read(archive_handle->src_fd, &tar, 512) == 512)
\t\t\t\tcontinue;''', '''            while ((i = full_read(archive_handle->src_fd, &tar, 512)) == 512)
                continue;
            if (i < 0)
                bb_simple_perror_msg_and_die(bb_msg_read_error);''')
    text = replace_once(text, '\tfile_header->size = GET_OCTAL(tar.size);', '''\tfile_header->size = GET_OCTAL(tar.size);
    if (file_header->size < 0 || file_header->size > INT64_MAX - 511)
        bb_simple_error_msg_and_die("invalid tar member size");''')
    start = text.index('\t\tp += len;')
    end = text.index('\n\t\tp[-1] =', start)
    text = text[:start] + '''        if (len < 2 || len > sz || errno != EINVAL || *end != ' '
            || end >= p + len - 1 || p[len - 1] != '\\n'
            || memchr(p, '\\0', len))
            bb_simple_error_msg_and_die("malformed extended header");
        p += len;
        sz -= len;
''' + text[end:]
    (output / 'get_header_tar.c').write_text(text, encoding='utf-8')
