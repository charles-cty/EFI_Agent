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
                    'stat -f stat-dir/file', 'stat', 'stat stat-dir/file >&-'):
        result = run(command)
        assert result.returncode != 0 and result.stderr, (command, result)
    with open('stat-error-result', 'wb') as output:
        output.write(b'STAT_ERRORS_OK\n')
    with open('stat-input', 'wb') as output:
        output.write(payload)
    print('UPSTREAM_STAT_OK', flush=True)
''')
