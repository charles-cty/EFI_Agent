"""Upstream stat integration with independent file metadata oracles."""
from textwrap import dedent


STAT_CHECK = dedent(r'''
    import os, stat, subprocess

    def run(command):
        return subprocess.run(command, shell=True, capture_output=True)

    os.makedirs('stat-dir')
    payload = b'alpha\x00omega'
    with open('stat-dir/file', 'wb') as output:
        output.write(payload)
    os.chmod('stat-dir/file', 0o640)
    timestamp = 1_700_000_123_000_000_000
    os.utime('stat-dir/file', ns=(timestamp, timestamp))
    os.symlink('file', 'stat-dir/link')

    source = os.stat('stat-dir/file')
    result = run("stat -c '%a|%F|%s|%u|%g|%U|%G|%h|%X|%Y' stat-dir/file")
    expected = '|'.join((format(stat.S_IMODE(source.st_mode), 'o'), 'regular file',
                         str(len(payload)), str(source.st_uid), str(source.st_gid),
                         'agent', 'agent', str(source.st_nlink), str(int(source.st_atime)),
                         str(int(source.st_mtime)))).encode() + b'\n'
    assert (result.returncode, result.stdout, result.stderr) == (0, expected, b''), result
    with open('stat-result', 'wb') as output:
        output.write(result.stdout)
    print('PASS stat file mode, type, size, ownership, links and timestamps', flush=True)

    result = run('stat stat-dir/file')
    assert result.returncode == 0 and not result.stderr, result
    assert b'regular file' in result.stdout and b'Access: (0640/' in result.stdout, result
    assert b'Uid: (    1/   agent)   Gid: (    1/   agent)' in result.stdout, result
    print('PASS stat default human-readable output', flush=True)

    result = run("stat -c '%F|%s|%n' stat-dir/link")
    expected = b'symbolic link|4|stat-dir/link\n'
    assert (result.returncode, result.stdout, result.stderr) == (0, expected, b''), result
    with open('stat-link-result', 'wb') as output:
        output.write(result.stdout)

    result = run('stat stat-dir/link')
    assert result.returncode == 0 and not result.stderr, result
    assert b"File: 'stat-dir/link' -> 'file'" in result.stdout, result
    assert b'symbolic link' in result.stdout, result
    with open('stat-link-human-result', 'wb') as output:
        output.write(b"SYMLINK_TARGET=file\n")

    result = run("stat -L -c '%F|%s|%n' stat-dir/link")
    expected = b'regular file|11|stat-dir/link\n'
    assert (result.returncode, result.stdout, result.stderr) == (0, expected, b''), result
    with open('stat-follow-result', 'wb') as output:
        output.write(result.stdout)

    result = run("stat -c '%F|%n' stat-dir/file stat-dir/link stat-dir")
    expected = (b'regular file|stat-dir/file\n'
                b'symbolic link|stat-dir/link\n'
                b'directory|stat-dir\n')
    assert (result.returncode, result.stdout, result.stderr) == (0, expected, b''), result
    with open('stat-multi-result', 'wb') as output:
        output.write(result.stdout)

    terse = run('stat -t stat-dir/file')
    fields = terse.stdout.split()
    assert terse.returncode == 0 and not terse.stderr and len(fields) == 15, terse
    assert fields[0] == b'stat-dir/file' and fields[1] == str(source.st_size).encode(), terse
    assert fields[3] == format(source.st_mode, 'x').encode(), terse
    assert fields[4:6] == [str(source.st_uid).encode(), str(source.st_gid).encode()], terse
    assert fields[7] == str(source.st_ino).encode() and fields[8] == str(source.st_nlink).encode(), terse
    assert fields[11:14] == [str(int(source.st_atime)).encode(),
                             str(int(source.st_mtime)).encode(),
                             str(int(source.st_ctime)).encode()], terse
    print('PASS stat terse metadata', flush=True)

    with open('stat-time-boundary', 'wb') as output:
        for seconds in (2147483647, 2147483648, 2200000000, 4294967296):
            os.utime('stat-dir/file', ns=(seconds * 10**9, seconds * 10**9))
            result = run("stat -c '%X|%Y' stat-dir/file")
            expected = (str(seconds) + '|' + str(seconds) + '\n').encode()
            assert (result.returncode, result.stdout, result.stderr) == (0, expected, b''), result
            output.write(result.stdout)
    os.utime('stat-dir/file', ns=(timestamp, timestamp))
    print('PASS stat 64-bit timestamp boundaries', flush=True)

    for command in ('stat missing-stat-file', 'stat -Q stat-dir/file',
                    'stat', 'stat stat-dir/file >&-'):
        result = run(command)
        assert result.returncode != 0 and result.stderr, (command, result)
    with open('stat-error-result', 'wb') as output:
        output.write(b'STAT_ERRORS_OK\n')
    with open('stat-input', 'wb') as output:
        output.write(payload)
    print('UPSTREAM_STAT_OK', flush=True)
''')


STATFS_CHECK = dedent(r'''
    import json

    fs_format = '%i|%l|%t|%T|%s|%S|%b|%f|%a|%c|%d|%n'

    def check_filesystem(path, kind, type_name):
        before = os.statvfs(path)
        result = subprocess.run(['stat', '-f', '-c', fs_format, path], capture_output=True)
        after = os.statvfs(path)
        assert result.returncode == 0 and not result.stderr, (path, result)
        fields = result.stdout.decode().rstrip('\n').split('|')
        assert len(fields) == 12, fields
        fsid, name_max, type_hex, name, block, fragment, total, free, available, files, files_free, operand = fields
        assert int(fsid, 16) == before.f_fsid, (path, fields, before)
        assert int(name_max) == before.f_namemax, fields
        assert int(type_hex, 16) == kind and name == type_name, fields
        assert int(block) == before.f_bsize and int(fragment) == before.f_frsize, fields
        assert int(total) == before.f_blocks and int(files) == before.f_files, fields
        # Shared host-volume allocation need not change monotonically.
        block_drift = 4 * 1024 * 1024 // before.f_frsize if before.f_fsid == 0 else 0
        for value, first, last, drift in ((free, before.f_bfree, after.f_bfree, block_drift),
                                          (available, before.f_bavail, after.f_bavail, block_drift),
                                          (files_free, before.f_ffree, after.f_ffree, 0)):
            assert min(first, last) - drift <= int(value) <= max(first, last) + drift, (path, fields, before, after)
        assert operand == path, fields
        print('PASS stat filesystem', repr(path), flush=True)
        return fields

    for path, kind, name in [('/', 0x45464941, 'efi-agent'),
                              ('stat-dir/file', 0x45464941, 'efi-agent'),
                              ('stat-dir/link', 0x45464941, 'efi-agent'),
                              ('/bin/stat', 0x45464941, 'efi-agent'),
                              ('/tmp', 0x01021994, 'tmpfs'),
                              ('/dev/null', 0x1373, 'devfs'),
                              ('/proc/uptime', 0x9fa0, 'proc')]:
        check_filesystem(path, kind, name)
    with open('/tmp/stat-quota', 'wb') as output:
        output.write(b'x' * 65537)
    fields = check_filesystem('/tmp/stat-quota', 0x01021994, 'tmpfs')
    assert list(map(int, fields[6:11])) == [33554432, 33488895, 33488895, 4097, 4095], fields
    os.unlink('/tmp/stat-quota')
    fields = check_filesystem('/tmp', 0x01021994, 'tmpfs')
    assert list(map(int, fields[6:11])) == [33554432, 33554432, 33554432, 4097, 4096], fields
    result = run('stat -f /tmp')
    assert result.returncode == 0 and not result.stderr, result
    for text in (b'Type: tmpfs', b'Block size: 1 ', b'Blocks: Total: 33554432',
                 b'Inodes: Total: 4097'):
        assert text in result.stdout, result
    result = run('stat -ft /tmp')
    expected = b'/tmp 3 1024 1021994 1 33554432 33554432 33554432 4097 4096\n'
    assert (result.returncode, result.stdout, result.stderr) == (0, expected, b''), result
    with open('stat-filesystem-terse', 'wb') as output:
        output.write(result.stdout)
    result = run("stat -f -c '%i:%T' /tmp /dev/null /proc/uptime /")
    assert (result.returncode, result.stdout, result.stderr) == (
        0, b'3:tmpfs\n2:devfs\n4:proc\n0:efi-agent\n', b''), result
    # One process exercises repeated snapshot allocation and release.
    result = subprocess.run(['stat', '-f', '-c', '%i'] + ['/tmp'] * 140, capture_output=True)
    assert (result.returncode, result.stdout, result.stderr) == (0, b'3\n' * 140, b''), result
    result = run("cd stat-dir; stat -f -c '%T' link")
    assert (result.returncode, result.stdout, result.stderr) == (0, b'efi-agent\n', b''), result
    os.symlink('no-such-target', 'stat-dir/dangling')
    os.symlink('loop', 'stat-dir/loop')
    os.mkdir('stat-dir/denied', 0o000)
    for command in ('stat -f stat-dir/absent', 'stat -f stat-dir/dangling',
                    'stat -f stat-dir/loop', 'stat -f stat-dir/file/child',
                    'stat -f stat-dir/denied/child', 'stat -f /tmp >&-'):
        result = run(command)
        assert result.returncode and result.stderr, (command, result)
    os.chmod('stat-dir/denied', 0o700)
    result = run("stat -f -c '%T' stat-dir/absent /tmp")
    assert result.returncode and result.stdout == b'tmpfs\n' and result.stderr, result
    fields = check_filesystem('/', 0x45464941, 'efi-agent')
    with open('stat-filesystem-root', 'w') as output:
        json.dump(fields, output)
    print('UPSTREAM_STATFS_OK', flush=True)
''')

STAT_CHECK += STATFS_CHECK
