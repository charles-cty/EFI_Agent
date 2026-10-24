"""Stage upstream unzip with anchored extraction and checked integrity."""


def prepare_unzip(source, output, replace_once):
    text = (source / 'archival/unzip.c').read_text(encoding='utf-8')
    text = replace_once(text, 'enum { zip_fd = 3 };', 'static int zip_fd;')
    text = replace_once(text, '\t\txdup2(STDIN_FILENO, zip_fd);', '''        zip_fd = dup(STDIN_FILENO);
        if (zip_fd < 0)
            bb_simple_perror_msg_and_die("standard input");''')
    text = replace_once(text, '\t\txmove_fd(src_fd, zip_fd);', '\t\tzip_fd = src_fd;')
    text = replace_once(text, '''\t\t\txlseek(zip_fd,
\t\t\t\tSWAP_LE32(cdf.fmt.relative_offset_of_local_header) + 4,
\t\t\t\tSEEK_SET);
\t\t\txread(zip_fd, zip.raw, ZIP_HEADER_LEN);''', '''            uint32_t local_magic;
            xlseek(zip_fd, SWAP_LE32(cdf.fmt.relative_offset_of_local_header), SEEK_SET);
            xread(zip_fd, &local_magic, sizeof(local_magic));
            if (local_magic != ZIP_FILEHEADER_MAGIC)
                bb_simple_error_msg_and_die("invalid local file header");
\t\t\txread(zip_fd, zip.raw, ZIP_HEADER_LEN);''')
    text = replace_once(text, '\ttotal_usize = 0;', '''    if (!(opts & OPT_l) && dst_fd != STDOUT_FILENO)
        efi_archive_begin();
\ttotal_usize = 0;''')
    # At this point all extracted names and symlinks have been processed.
    ending = '\n\treturn 0;\n}\n'
    if not text.endswith(ending):
        raise ValueError('Upstream unzip exit changed')
    text = text[:-len(ending)] + '''
    xclose(zip_fd);
    efi_archive_end();
    fflush_stdout_and_exit_SUCCESS();
}
'''
    text = replace_once(text, '\t\t\t\t\tclose(dst_fd);', '\t\t\t\t\txclose(dst_fd);')
    text = replace_once(text, '\t\t\t\t\txclose(dst_fd);', '''                    efi_archive_file_attributes(dst_fd, file_mode,
                        zip.fmt.moddate, zip.fmt.modtime);
\t\t\t\t\txclose(dst_fd);''')
    text = replace_once(text, 'bb_make_directory(dst_fn, dir_mode, FILEUTILS_IGNORE_CHMOD_ERR)',
                        'bb_make_directory(dst_fn, dir_mode | 0700, 0)')
    text = replace_once(text, '''\t\t\tgoto skip_cmpsize;
\t\t}
 check_file:''', '''            efi_archive_remember_attributes(dst_fn, dir_mode,
                zip.fmt.moddate, zip.fmt.modtime);
\t\t\tgoto skip_cmpsize;
\t\t}
 check_file:''')
    text = replace_once(text, '''\t\t\t\tif (dst_fd != STDOUT_FILENO) /* not -p? */
\t\t\t\t\tunzip_extract_symlink(&symlink_placeholders, &zip, dst_fn);''', '''                if (dst_fd != STDOUT_FILENO) { /* not -p? */
                    unzip_extract_symlink(&symlink_placeholders, &zip, dst_fn);
                    efi_archive_remember_attributes(dst_fn, file_mode,
                        zip.fmt.moddate, zip.fmt.modtime);
                } else {
                    unzip_extract(&zip, STDOUT_FILENO);
                }''')
    text = replace_once(text, '\t\tbb_simple_error_msg("bad length");',
                        '\t\tbb_simple_error_msg_and_die("bad length");')
    text = replace_once(text, '\t\txread(zip_fd, dst_fn, zip.fmt.filename_len);', '''\t\txread(zip_fd, dst_fn, zip.fmt.filename_len);
        if (strlen(dst_fn) != zip.fmt.filename_len)
            bb_simple_error_msg_and_die("invalid member name");''')
    text = replace_once(text, '''\t\toff_t size = zip->fmt.ucmpsize;
\t\tif (size)
\t\t\tbb_copyfd_exact_size(zip_fd, dst_fd, size);
\t\treturn;''', '''        unsigned char buffer[16384];
        uint32_t remaining = zip->fmt.ucmpsize;
        uint32_t crc = ~0u;
        if (remaining != zip->fmt.cmpsize)
            bb_simple_error_msg_and_die("bad length");
        if (!global_crc32_table)
            global_crc32_new_table_le();
        while (remaining) {
            size_t count = MIN(remaining, sizeof(buffer));
            xread(zip_fd, buffer, count);
            crc = crc32_block_endian0(crc, buffer, count, global_crc32_table);
            xwrite(dst_fd, buffer, count);
            remaining -= count;
        }
        if (~crc != zip->fmt.crc32)
            bb_simple_error_msg_and_die("crc error");
        return;''')
    text = replace_once(text, '''\t\txread(zip_fd, target, zip->fmt.ucmpsize);
\t} else {
#if 1
\t\tbb_simple_error_msg_and_die("compressed symlink is not supported");
#else''', '''\t\txread(zip_fd, target, zip->fmt.ucmpsize);
        if (zip->fmt.cmpsize != zip->fmt.ucmpsize)
            bb_simple_error_msg_and_die("bad length");
\t} else {
#if 0''')
    # Replace the disabled example with the normal upstream DEFLATE entry point.
    start = text.index('\n#if 0\n\t\ttransformer_state_t xstate;', text.index('static void unzip_extract_symlink'))
    end = text.index('\n#endif', start)
    text = text[:start] + '''
        transformer_state_t xstate;
        if (zip->fmt.method != 8 || zip->fmt.ucmpsize == 0)
            bb_simple_error_msg_and_die("unsupported symlink compression");
        init_transformer_state(&xstate);
        xstate.src_fd = zip_fd;
        xstate.bytes_in = zip->fmt.cmpsize;
        xstate.mem_output_size_max = zip->fmt.ucmpsize;
        if (inflate_unzip(&xstate) < 0 || xstate.mem_output_size != zip->fmt.ucmpsize)
            bb_simple_error_msg_and_die("inflate error");
        target = xstate.mem_output_buf;''' + text[end + len('\n#endif'):]
    text = replace_once(text, '\tcreate_or_remember_link(symlink_placeholders,', '''    if (strlen(target) != zip->fmt.ucmpsize || !target[0])
        bb_simple_error_msg_and_die("invalid symlink target");
    if (efi_archive_crc(target, zip->fmt.ucmpsize) != zip->fmt.crc32)
        bb_simple_error_msg_and_die("crc error");
\tcreate_or_remember_link(symlink_placeholders,''')
    # Read real bytes only. A full-sized, zero-filled buffer hides truncation.
    text = replace_once(text, '#define PEEK_FROM_END (64*1024)', '#define PEEK_FROM_END (64*1024 + 22)')
    text = replace_once(text, '\tfull_read(zip_fd, buf, PEEK_FROM_END);', '''    ssize_t count = full_read(zip_fd, buf, PEEK_FROM_END);
    if (count < 0)
        bb_simple_perror_msg_and_die(bb_msg_read_error);''')
    text = replace_once(text, '\twhile (p <= buf + PEEK_FROM_END - CDE_LEN - 4) {',
                        '\twhile (count >= 22 && p <= buf + count - 22) {')
    text = replace_once(text, '\t\tif (cde.fmt.cdf_offset < end + (p - buf)) {', '''        unsigned comment_size = p[17] | ((unsigned)p[18] << 8);
        if ((size_t)(p - 3 - buf) + 22 + comment_size == (size_t)count
            && (uint64_t)cde.fmt.cdf_offset + SWAP_LE32(cde.fmt.cdf_size)
                == (uint64_t)end + (p - 3 - buf)) {
            if (cde.fmt.this_disk_no || cde.fmt.disk_with_cdf_no
                || cde.fmt.cdf_entries_on_this_disk != cde.fmt.cdf_entries_total)
                bb_simple_error_msg_and_die("multi-volume ZIP is unsupported");''')
    text = replace_once(text, '\txread(zip_fd, cdf->raw, CDF_HEADER_LEN);', '''    if (magic != ZIP_CDF_MAGIC)
        bb_simple_error_msg_and_die("invalid central directory");
\txread(zip_fd, cdf->raw, CDF_HEADER_LEN);''')
    text = replace_once(text, '''\tcdf_offset += 4 + CDF_HEADER_LEN
\t\t+ cdf->fmt.filename_len
\t\t+ cdf->fmt.extra_len
\t\t+ cdf->fmt.file_comment_length;''', '''    uint64_t next = (uint64_t)cdf_offset + 4 + CDF_HEADER_LEN
        + cdf->fmt.filename_len + cdf->fmt.extra_len + cdf->fmt.file_comment_length;
    if (next >= BAD_CDF_OFFSET)
        bb_simple_error_msg_and_die("central directory offset overflow");
    cdf_offset = next;''')
    (output / 'unzip.c').write_text(text, encoding='utf-8')
