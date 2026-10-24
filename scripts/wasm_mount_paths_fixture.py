"""Verify global mount/link resolution and bounded directory capabilities."""
from textwrap import dedent


MOUNT_PATHS_CHECK = dedent(r'''
    import errno, os, stat, subprocess

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

    os.mkdir('mount-work')
    os.mkdir('mount-work/dir')
    os.mkdir('/tmp/mount-dir')
    with open('mount-work/data', 'wb') as output:
        output.write(b'workspace\x00\xff\n')
    with open('/tmp/data', 'wb') as output:
        output.write(b'temporary\x00\xff\n')
    os.chmod('mount-work/data', 0o600)
    os.chmod('/tmp/data', 0o640)
    os.symlink('/tmp/mount-dir', 'mount-work/absolute')
    os.symlink('tmp/mount-dir', 'mount-relative')
    os.symlink('/mount-work', '/tmp/absolute')
    os.symlink('../mount-work', '/tmp/relative')
    os.symlink('/', '/tmp/root')
    for path in ['mount-work/absolute/../data', '/mount-work/absolute/../data', 'mount-relative/../data']:
        assert open(path, 'rb').read() == b'temporary\x00\xff\n'
        assert stat.S_IMODE(os.stat(path).st_mode) == 0o640
        assert os.statvfs(path).f_fsid == 3
        assert run('cat ' + path) == b'temporary\x00\xff\n'
        assert run('stat -c "%d %a" ' + path).strip() == b'3 640'
    for path in ['/tmp/absolute/data', '/tmp/relative/data', '/tmp/root/mount-work/data']:
        assert open(path, 'rb').read() == b'workspace\x00\xff\n'
        assert stat.S_IMODE(os.stat(path).st_mode) == 0o600
        assert os.statvfs(path).f_fsid == 0
    assert os.stat('/tmp/..').st_ino == os.stat('/').st_ino
    assert os.statvfs('/tmp/..').f_fsid == 0
    assert run('realpath mount-work/absolute/../data').strip() == b'/tmp/data'
    assert b' ..\n' in run('ls -la /tmp')
    assert run('cat /tmp/root/mount-work/data') == b'workspace\x00\xff\n'
    print('PASS global absolute/relative links, parent order and mount statistics', flush=True)

    # Python metadata adaptation must retain link/.. rather than fold it first.
    os.chmod('mount-work/absolute/../data', 0o604)
    assert stat.S_IMODE(os.stat('/tmp/data').st_mode) == 0o604
    assert stat.S_IMODE(os.stat('mount-work/data').st_mode) == 0o600
    with os.scandir('mount-work/absolute/..') as entries:
        entry = next(item for item in entries if item.name == 'data')
        assert stat.S_IMODE(entry.stat().st_mode) == 0o604
    child = subprocess.run(['python', '-c',
        "import os,stat;assert open('data','rb').read()==b'temporary\\x00\\xff\\n';"
        "assert stat.S_IMODE(os.stat('data').st_mode)==0o604"],
        cwd='mount-work/absolute/..', capture_output=True)
    assert (child.returncode, child.stdout, child.stderr) == (0, b'', b''), child
    os.symlink('/tmp/new', 'mount-work/dangling')
    with open('mount-work/dangling', 'wb') as output:
        output.write(b'created through dangling link')
    assert open('/tmp/new', 'rb').read() == b'created through dangling link'
    fails(lambda: os.open('mount-work/dangling', os.O_CREAT | os.O_EXCL | os.O_WRONLY), errno.EEXIST)
    os.rename('mount-work/dangling', 'mount-work/renamed-link')
    assert os.readlink('mount-work/renamed-link') == '/tmp/new'
    os.unlink('mount-work/renamed-link')
    assert os.path.exists('/tmp/new')
    fails(lambda: os.rename('/tmp/data', 'mount-work/cross'), errno.EXDEV)
    fails(lambda: os.link('/tmp/data', 'mount-work/cross'), errno.EXDEV)
    print('PASS Python metadata, scandir, subprocess cwd and mutation link policy', flush=True)

    os.symlink('/mount-work/data', '/tmp/mount-dir/escape')
    os.symlink('../../mount-work/data', '/tmp/mount-dir/relative-escape')
    fd = os.open('/tmp/mount-dir', os.O_RDONLY | os.O_DIRECTORY)
    os.rename('/tmp/mount-dir', '/tmp/renamed-dir')
    for path in ['escape', 'relative-escape', '../data']:
        fails(lambda path=path: os.open(path, os.O_RDONLY, dir_fd=fd), errno.ENOTCAPABLE)
    os.close(fd)
    # Actual root descriptors keep the mounted namespace.
    root_fd = os.open('/', os.O_RDONLY | os.O_DIRECTORY)
    data_fd = os.open('tmp/data', os.O_RDONLY, dir_fd=root_fd)
    assert os.read(data_fd, 32) == b'temporary\x00\xff\n'
    os.close(data_fd)
    os.close(root_fd)
    assert run('python -c "print(731)"').strip() == b'731'
    os.symlink('/bin/ash', '/tmp/ash')
    assert run('/tmp/ash -c "echo installed"') == b'installed\n'
    print('PASS retained directory bounds, root views and installed command links', flush=True)

    name = lambda index: ('mount-work/' if index % 2 == 0 else '/tmp/') + 'chain-' + str(index)
    for index in reversed(range(41)):
        target = '/mount-work/data' if index == 40 else '/' + name(index + 1).lstrip('/')
        os.symlink(target, name(index))
    assert open(name(1), 'rb').read() == b'workspace\x00\xff\n'
    fails(lambda: os.stat(name(0)), errno.ELOOP)
    assert stat.S_ISLNK(os.lstat(name(0)).st_mode)
    fails(lambda: os.stat('mount-work/data/../data'), errno.ENOTDIR)
    fails(lambda: os.stat('mount-work/missing/../data'), errno.ENOENT)
    os.mkdir('mount-work/denied')
    os.chmod('mount-work/denied', 0o600)
    fails(lambda: os.stat('mount-work/denied/../data'), errno.EACCES)
    os.chmod('mount-work/denied', 0o700)
    run('cp /tmp/data mount-work/copied')
    run('rm -rf /tmp/renamed-dir /tmp/absolute /tmp/relative /tmp/root /tmp/new /tmp/data /tmp/ash')
    for index in range(1, 41, 2):
        os.unlink(name(index))
    with open('mount-paths-result', 'wb') as output:
        output.write(b'MOUNT_PATHS_OK\n')
    print('MOUNT_PATHS_OK', flush=True)
''')


def verify_mount_paths(workspace):
    assert workspace.read('mount-paths-result') == b'MOUNT_PATHS_OK\n'
    assert workspace.read('mount-work/data') == b'workspace\x00\xff\n'
    assert workspace.read('mount-work/copied') == b'temporary\x00\xff\n'
    assert workspace.entries['mount-work/data']['mode'] == 0o600
    assert workspace.entries['mount-work/absolute']['target'] == '/tmp/mount-dir'
    assert not any(name == 'tmp' or name.startswith('tmp/') for name in workspace.entries)
