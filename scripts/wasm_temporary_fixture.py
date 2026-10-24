"""Exercise shared temporary inodes through standard Python and BusyBox."""
from textwrap import dedent


TEMPORARY_CHECK = dedent(r'''
    import errno, os, stat, subprocess, tempfile

    def run(command):
        result = subprocess.run(command, shell=True, capture_output=True)
        assert (result.returncode, result.stderr) == (0, b''), (command, result)
        return result.stdout

    def fails(operation, expected):
        try:
            operation()
        except OSError as error:
            assert error.errno == expected, error
        else:
            raise AssertionError('Expected operation failure')

    initial = os.statvfs('/tmp')
    assert initial.f_fsid == 3 and initial.f_bavail == 32 * 1024 * 1024, initial
    # Private process requests must only intercept the workspace preopen.
    # An anchored temporary directory can contain the same ordinary filename.
    private_dir = os.open('/tmp', os.O_RDONLY | os.O_DIRECTORY)
    private_file = os.open('.efi-process', os.O_CREAT | os.O_RDWR, 0o600, dir_fd=private_dir)
    assert os.write(private_file, b'ordinary bytes') == 14
    os.lseek(private_file, 0, os.SEEK_SET)
    assert os.read(private_file, 20) == b'ordinary bytes'
    os.close(private_file)
    os.unlink('.efi-process', dir_fd=private_dir)
    os.close(private_dir)
    run('mkdir -p /tmp/tree/sub /tmp/other')
    payload = bytes((index * 37 + 19) % 256 for index in range(65537))
    with open('/tmp/tree/sub/data', 'wb') as output:
        output.write(payload)
    fd = os.open('/tmp/tree/sub', os.O_RDONLY | os.O_DIRECTORY)
    with open('/tmp/tree/sub/data', 'r+b') as held:
        identity = os.fstat(held.fileno())
        assert identity.st_dev == 3 and identity.st_ino != 0 and identity.st_mtime_ns > 0, identity
        os.rename('/tmp/tree', '/tmp/renamed')
        os.link('data', 'alias', src_dir_fd=fd, dst_dir_fd=fd)
        os.symlink('sub/alias', '/tmp/renamed/link')
        assert os.readlink('/tmp/renamed/link') == 'sub/alias'
        assert stat.S_ISLNK(os.lstat('/tmp/renamed/link').st_mode)
        assert os.stat('/tmp/renamed/link').st_ino == identity.st_ino
        assert open('/tmp/renamed/link', 'rb').read() == payload
        assert run('cat /tmp/renamed/link | wc -c').strip() == b'65537'
        os.rename('alias', '/tmp/other/alias', src_dir_fd=fd)
        os.link('/tmp/other/alias', 'returned', dst_dir_fd=fd)
        os.link('returned', '/tmp/root-alias', src_dir_fd=fd)
        assert os.fstat(held.fileno()).st_nlink == 4
        os.chmod('/tmp/renamed/sub/data', 0o600)
        os.utime('/tmp/root-alias', ns=(193000000007, 731000000019))
        info = os.stat('/tmp/other/alias')
        assert stat.S_IMODE(info.st_mode) == 0o600 and info.st_mtime_ns == 731000000019, info
        assert run('stat -c "%d %a %h %s" /tmp/root-alias').strip() == b'3 600 4 65537'
        for path in ['/tmp/root-alias', '/tmp/other/alias', '/tmp/renamed/sub/returned']:
            os.unlink(path)
        os.unlink('data', dir_fd=fd)
        assert not os.path.exists('/tmp/renamed/link') and os.path.lexists('/tmp/renamed/link')
        assert os.fstat(held.fileno()).st_nlink == 0
        assert held.read() == payload
        assert os.statvfs('/tmp').f_bavail == initial.f_bavail - len(payload)
    os.close(fd)
    assert os.statvfs('/tmp').f_bavail == initial.f_bavail
    print('PASS temporary links, metadata and renamed descriptor anchors', flush=True)

    os.symlink('loop-b', '/tmp/loop-a')
    os.symlink('loop-a', '/tmp/loop-b')
    fails(lambda: os.stat('/tmp/loop-a'), errno.ELOOP)
    run('rm /tmp/loop-a /tmp/loop-b')
    os.mkdir('/tmp/denied', 0o000)
    fails(lambda: open('/tmp/denied/child', 'wb'), errno.EACCES)
    os.chmod('/tmp/denied', 0o700)
    os.rmdir('/tmp/denied')
    with tempfile.TemporaryDirectory(dir='/tmp') as directory:
        with open(directory + '/input', 'wb') as output:
            output.write(payload)
        child = subprocess.run('pwd; cat input', shell=True, cwd=directory, capture_output=True)
        assert (child.returncode, child.stdout, child.stderr) == (
            0, directory.encode() + b'\n' + payload, b''), child
    assert not os.path.exists(directory)
    print('PASS temporary permissions, loops, tempfile cleanup and child cwd', flush=True)

    os.mkdir('temporary-saved')
    with open('/tmp/source', 'wb') as output:
        output.write(payload)
    os.chmod('/tmp/source', 0o640)
    os.utime('/tmp/source', ns=(193000000000, 731000000000))
    fails(lambda: os.rename('/tmp/source', 'temporary-saved/data'), errno.EXDEV)
    fails(lambda: os.link('/tmp/source', 'temporary-saved/data'), errno.EXDEV)
    run('mv /tmp/source temporary-saved/data')
    assert not os.path.exists('/tmp/source')
    assert open('temporary-saved/data', 'rb').read() == payload
    run('cp -p temporary-saved/data /tmp/back')
    assert open('/tmp/back', 'rb').read() == payload
    assert stat.S_IMODE(os.stat('/tmp/back').st_mode) == 0o640
    run('tar -cf /tmp/bundle.tar -C temporary-saved data; mkdir /tmp/extracted; '
        'tar -xf /tmp/bundle.tar -C /tmp/extracted')
    assert open('/tmp/extracted/data', 'rb').read() == payload
    run('rm -rf /tmp/renamed /tmp/other /tmp/back /tmp/bundle.tar /tmp/extracted')
    assert os.statvfs('/tmp').f_bavail == initial.f_bavail
    print('PASS cross-device mv fallback and temporary archive extraction', flush=True)

    with open('/tmp/quota', 'w+b', buffering=0) as held:
        held.truncate(initial.f_bavail - 3)
        with open('/tmp/short', 'w+b', buffering=0) as short:
            assert short.write(b'12345') == 3
            fails(lambda: short.write(b'x'), errno.ENOSPC)
            os.unlink('/tmp/quota')
            assert os.statvfs('/tmp').f_bavail == 0
            held.truncate(initial.f_bavail - 5)
            assert short.write(b'yz') == 2
            assert os.statvfs('/tmp').f_bavail == 0
        os.unlink('/tmp/short')
    assert os.statvfs('/tmp').f_bavail == initial.f_bavail
    assert os.statvfs('/tmp').f_ffree == initial.f_ffree
    with open('temporary-result', 'wb') as output:
        output.write(b'TEMPORARY_MOUNT_OK\n')
    print('TEMPORARY_MOUNT_OK', flush=True)
''')


def verify_temporary(workspace):
    assert workspace.read('temporary-result') == b'TEMPORARY_MOUNT_OK\n'
    assert workspace.read('temporary-saved/data') == bytes(
        (index * 37 + 19) % 256 for index in range(65537))
    metadata = workspace.entries['temporary-saved/data']
    assert metadata['mode'] == 0o640 and metadata['modified'] == 731000000000, metadata
    assert not any(name == 'tmp' or name.startswith('tmp/') for name in workspace.entries)
