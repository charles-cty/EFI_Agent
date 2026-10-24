"""Exercise non-terminal character devices through unchanged Python and ash."""
from textwrap import dedent


CHARACTER_DEVICES_CHECK = dedent(r'''
    import errno, hashlib, os, stat, subprocess

    def fails(operation, expected):
        try:
            operation()
        except OSError as error:
            assert error.errno == expected, error
        else:
            raise AssertionError('Expected device operation failure')

    inodes = set()
    for name in ['null', 'zero', 'random', 'urandom']:
        path = '/dev/' + name
        info = os.stat(path)
        assert stat.S_ISCHR(info.st_mode) and stat.S_IMODE(info.st_mode) == 0o666, info
        assert (info.st_dev, info.st_size, info.st_blocks) == (2, 0, 0), info
        inodes.add(info.st_ino)
        fd = os.open(path, os.O_RDWR | os.O_CREAT | os.O_TRUNC | os.O_NONBLOCK)
        assert stat.S_ISCHR(os.fstat(fd).st_mode)
        assert not os.isatty(fd)
        with open(path, 'rb') as source:
            assert not source.isatty()
        for whence in [os.SEEK_SET, os.SEEK_CUR, os.SEEK_END]:
            assert os.lseek(fd, -73, whence) == 0
        assert os.write(fd, bytes(8388608)) == 8388608
        assert not os.get_blocking(fd)
        duplicate = os.dup(fd)
        os.set_blocking(duplicate, True)
        assert os.get_blocking(fd)
        os.close(duplicate)
        fails(lambda: os.ftruncate(fd, 0), errno.EINVAL)
        fails(lambda: os.fsync(fd), errno.EINVAL)
        fails(lambda: os.fchmod(fd, 0o600), errno.EROFS)
        os.close(fd)
        fails(lambda: os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL), errno.EEXIST)
        fails(lambda: os.chmod(path, 0o600), errno.EROFS)
        fails(lambda: os.utime(path, ns=(123, 456)), errno.EROFS)
        # Character devices use their containing mount, including through links.
        for candidate in (path, 'device-df-' + name, '/tmp/device-df-' + name):
            if candidate != path:
                os.symlink(path, candidate)
            result = subprocess.run(['df', '-B1', '-P', candidate], capture_output=True)
            assert result.returncode == 0 and not result.stderr, result
            row = result.stdout.splitlines()[1].split()
            capacity = os.statvfs(candidate)
            assert row[0] == b'devfs' and row[-1] == b'/dev', row
            assert int(row[1]) == capacity.f_blocks * capacity.f_frsize == 0, row
            if candidate != path:
                os.unlink(candidate)
    assert len(inodes) == 4
    print('PASS character metadata, permissions, shared flags, df mount selection and 8 MiB discarded writes', flush=True)

    with open('/dev/null', 'r+b', buffering=0) as source:
        assert source.read(65537) == b''
        assert source.write(b'discarded') == 9
        assert source.read(1) == b''
    for size in [0, 1, 65535, 65536, 65537, 524287, 524288, 524289]:
        with open('/dev/zero', 'rb') as source:
            assert source.read(size) == bytes(size), size
    result = subprocess.run('head -c 8388608 /dev/zero > device-zero-large', shell=True, capture_output=True)
    assert (result.returncode, result.stdout, result.stderr) == (0, b'', b''), result
    with open('device-zero-large', 'rb') as source:
        assert source.read() == bytes(8388608)
    result = subprocess.run('test -c /dev/null && test -c /dev/zero && type echo >/dev/null 2>&1', shell=True, capture_output=True)
    assert (result.returncode, result.stdout, result.stderr) == (0, b'', b''), result
    result = subprocess.run('stat -c "%F %a" /dev/null', shell=True, capture_output=True)
    assert (result.returncode, result.stdout, result.stderr) == (0, b'character special file 666\n', b''), result
    print('PASS null EOF, zero boundaries, upstream command redirection and saved 8 MiB zero bytes', flush=True)

    # Neither an inherited device nor an opened device is a terminal.
    result = subprocess.run('python - </dev/null', shell=True, capture_output=True)
    assert (result.returncode, result.stdout, result.stderr) == (0, b'', b''), result
    with open('/dev/zero', 'rb') as input_file:
        result = subprocess.run(['python', '-c', "import sys; assert not sys.stdin.isatty(); assert sys.stdin.buffer.read(17)==bytes(17)"],
            stdin=input_file, capture_output=True)
    assert (result.returncode, result.stdout, result.stderr) == (0, b'', b''), result
    result = subprocess.run(['python', '-c', "import sys; assert sys.stdin.buffer.read()==b''; print('discarded'); print('discarded',file=sys.stderr)"],
        stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    assert result.returncode == 0, result
    print('PASS finite Python stdin, no REPL banner, FileIO.isatty and subprocess.DEVNULL', flush=True)

    for name in ['random', 'urandom']:
        fd = os.open('/dev/' + name, os.O_RDONLY)
        assert os.read(fd, 0) == b''
        sample = os.read(fd, 65537)
        assert len(sample) == 65536, len(sample)
        os.close(fd)
        with open('device-' + name + '-bytes', 'wb') as output:
            output.write(sample)
        with open('device-' + name + '-sha256', 'w') as output:
            output.write(hashlib.sha256(sample).hexdigest() + '\n')
        result = subprocess.run(['sha256sum', 'device-' + name + '-bytes'], capture_output=True)
        assert result.returncode == 0 and not result.stderr, result
        assert result.stdout.split()[0].decode() == hashlib.sha256(sample).hexdigest(), result
    with open('character-devices-result', 'wb') as output:
        output.write(b'CHARACTER_DEVICES_OK\n')
    print('CHARACTER_DEVICES_OK', flush=True)
''')


def verify_character_devices(workspace):
    import hashlib
    assert workspace.read('character-devices-result') == b'CHARACTER_DEVICES_OK\n'
    assert workspace.read('device-zero-large') == bytes(8388608)
    for name in ['random', 'urandom']:
        sample = workspace.read('device-' + name + '-bytes')
        assert len(sample) == 65536
        assert workspace.read('device-' + name + '-sha256') == hashlib.sha256(sample).hexdigest().encode() + b'\n'
    assert not any(path == 'dev' or path.startswith('dev/') for path in workspace.entries)
