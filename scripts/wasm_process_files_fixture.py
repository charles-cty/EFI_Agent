"""Exercise process files and descriptor links through real Python and ash."""
from textwrap import dedent


PROCESS_FILES_CHECK = dedent(r'''
    import errno, os, stat, subprocess, time

    def fails(operation, expected):
        try:
            operation()
        except OSError as error:
            assert error.errno == expected, error
        else:
            raise AssertionError('Expected operation failure')

    pid = os.getpid()
    assert os.readlink('/proc/self') == str(pid)
    assert os.path.isdir('/proc/self') and stat.S_ISLNK(os.lstat('/proc/self').st_mode)
    status = open('/proc/self/status').read()
    assert 'Pid:\t' + str(pid) + '\n' in status, status
    assert 'PPid:\t' + str(os.getppid()) + '\n' in status, status
    assert 'Uid:\t1\t1\t1\t1\n' in status, status
    arguments = open('/proc/self/cmdline', 'rb').read().split(b'\0')
    assert arguments[0] == b'python' and arguments[1] == b'-c' and arguments[-1] == b'', arguments[:2]
    for entry in open('/proc/self/environ', 'rb').read().split(b'\0'):
        if entry:
            key, value = entry.decode().split('=', 1)
            assert os.environ[key] == value, entry
    assert os.readlink('/proc/self/cwd') == os.getcwd()
    assert os.readlink('/proc/self/exe') == '/python.wasm'
    mounts = open('/proc/mounts', 'rb').read()
    assert mounts == open('/proc/self/mounts', 'rb').read()
    assert b'devfs /dev devfs ro 0 0\n' in mounts and b'tmpfs /tmp tmpfs rw 0 0\n' in mounts, mounts
    assert {'0', '1', '2', '3'} <= set(os.listdir('/dev/fd'))
    for descriptor in (0, 1, 2):
        target = os.stat('/proc/self/fd/' + str(descriptor))
        opened = os.fstat(descriptor)
        assert (target.st_mode, target.st_dev, target.st_ino, target.st_size) == (opened.st_mode, opened.st_dev, opened.st_ino, opened.st_size), (target, opened)
    control = os.open('.efi-process', os.O_RDWR)
    assert str(control) not in os.listdir('/dev/fd')
    assert not os.path.lexists('/proc/self/fd/' + str(control))
    os.close(control)
    fails(lambda: open('/proc/self/status', 'w'), errno.EROFS)
    fails(lambda: os.chmod('/proc/self/status', 0o600), errno.EROFS)
    with open('/proc/self/status', 'rb') as snapshot:
        fails(lambda: os.fsync(snapshot.fileno()), errno.EINVAL)
    print('PASS actual process identity, state, arguments, environment, cwd and mounts', flush=True)

    with open('/dev/stdout', 'w') as output:
        output.write('DESCRIPTOR_STDOUT_OK\n')
    fails(lambda: os.open('/dev/stdin', os.O_WRONLY), errno.EACCES)
    read_fd, write_fd = os.pipe()
    alias = os.open('/dev/fd/' + str(write_fd), os.O_WRONLY)
    os.close(write_fd)
    assert os.write(alias, b'pipe-alias') == 10
    os.close(alias)
    assert os.read(read_fd, 32) == b'pipe-alias'
    assert os.read(read_fd, 1) == b''
    os.close(read_fd)
    read_fd, write_fd = os.pipe()
    os.close(read_fd)
    fails(lambda: os.write(write_fd, b'x'), errno.EPIPE)
    os.close(write_fd)
    os.mkdir('proc-work')
    with open('proc-work/shared', 'wb') as output:
        output.write(b'abcdef')
    fd = os.open('proc-work/shared', os.O_RDWR)
    path = '/proc/self/fd/' + str(fd)
    assert os.path.samefile(path, 'proc-work/shared')
    fails(lambda: os.open(path, os.O_RDONLY | os.O_NOFOLLOW), errno.ELOOP)
    fails(lambda: os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL), errno.EEXIST)
    read_only = os.open('proc-work/shared', os.O_RDONLY)
    fails(lambda: os.open('/dev/fd/' + str(read_only), os.O_WRONLY | os.O_TRUNC), errno.EACCES)
    os.close(read_only)
    assert open('proc-work/shared', 'rb').read() == b'abcdef'
    os.lseek(fd, 2, os.SEEK_SET)
    alias = os.open(path, os.O_RDWR | os.O_TRUNC)
    assert os.read(alias, 2) == b'cd'
    assert os.lseek(fd, 0, os.SEEK_CUR) == 4
    assert os.write(alias, b'Z') == 1
    os.unlink('proc-work/shared')
    assert os.stat(path).st_nlink == 0
    os.close(fd)
    assert not os.path.lexists(path)
    os.lseek(alias, 0, os.SEEK_SET)
    assert os.read(alias, 16) == b'abcdZf'
    os.close(alias)
    print('PASS descriptor aliases share cursors and lifetime without retaining pipe endpoints', flush=True)

    os.mkdir('proc-work/directory')
    fd = os.open('proc-work/directory', os.O_RDONLY)
    path = '/dev/fd/' + str(fd)
    os.rename('proc-work/directory', 'proc-work/renamed')
    os.mkdir(path + '/child')
    with open(path + '/child/data', 'wb') as output:
        output.write(b'directory-fd-bytes')
    os.symlink('data', path + '/child/link')
    assert os.readlink(path + '/child/link') == 'data'
    assert open(path + '/child/link', 'rb').read() == b'directory-fd-bytes'
    os.rename(path + '/child/data', 'proc-work/moved')
    os.link('proc-work/moved', path + '/child/hard')
    assert os.path.samefile('proc-work/moved', path + '/child/hard')
    assert set(os.listdir(path + '/child')) == {'link', 'hard'}
    os.unlink(path + '/child/link')
    fails(lambda: open(path + '/../escape', 'wb'), errno.ENOTCAPABLE)
    os.close(fd)
    os.symlink('/proc/self/fd/3/proc-work/loop', 'proc-work/loop')
    fails(lambda: os.stat('proc-work/loop'), errno.ELOOP)
    for index in range(39):
        os.symlink('moved' if index == 0 else 'route-link' + str(index - 1), 'proc-work/route-link' + str(index))
    assert open('proc-work/route-link38', 'rb').read() == b'directory-fd-bytes'
    # self + fd + 38 ordinary links succeeds; one additional link fails.
    assert open('/proc/self/fd/3/proc-work/route-link37', 'rb').read() == b'directory-fd-bytes'
    fails(lambda: os.stat('/proc/self/fd/3/proc-work/route-link38'), errno.ELOOP)
    print('PASS renamed directory descriptor routing, mutations, links and shared loop limits', flush=True)

    first = list(map(float, open('/proc/uptime').read().split()))
    time.sleep(0.04)
    second = list(map(float, open('/proc/uptime').read().split()))
    assert len(first) == len(second) == 2 and second[0] - first[0] >= 0.03, (first, second)
    assert 0 <= first[1] <= first[0] and first[1] <= second[1] <= second[0], (first, second)
    child_code = "import os; assert b'PROC_MARKER=child-value' in open('/proc/self/environ','rb').read().split(b'\\0'); print(os.getpid(), os.getppid(), os.readlink('/proc/self'), flush=True); os.read(0,1); print('CHILD_RELEASED',flush=True)"
    child = subprocess.Popen(['python', '-c', child_code], env=dict(os.environ, PROC_MARKER='child-value'), stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    values = child.stdout.readline().split()
    assert list(map(int, values)) == [child.pid, pid, child.pid], values
    child_status = open('/proc/' + str(child.pid) + '/status').read()
    assert 'PPid:\t' + str(pid) + '\n' in child_status, child_status
    assert open('/proc/' + str(child.pid) + '/cmdline', 'rb').read().split(b'\0')[:2] == [b'python', b'-c']
    stdout, stderr = child.communicate(b'x')
    assert (child.returncode, stdout, stderr) == (0, b'CHILD_RELEASED\n', b''), (child.returncode, stdout, stderr)
    assert not os.path.exists('/proc/' + str(child.pid))
    result = subprocess.run('printf shell-fd-ok > /dev/stdout; printf append-ok >> /dev/stdout', shell=True, capture_output=True)
    assert (result.returncode, result.stdout, result.stderr) == (0, b'shell-fd-okappend-ok', b''), result
    with open('process-files-result', 'wb') as output:
        output.write(b'PROCESS_FILES_OK\n')
    print('PROCESS_FILES_OK', flush=True)
''')


def verify_process_files(workspace):
    assert workspace.read('process-files-result') == b'PROCESS_FILES_OK\n'
    assert workspace.read('proc-work/moved') == b'directory-fd-bytes'
    assert workspace.read('proc-work/renamed/child/hard') == b'directory-fd-bytes'
    assert workspace.entries['proc-work/moved']['inode'] == workspace.entries['proc-work/renamed/child/hard']['inode']
    assert not any(path in ('dev', 'proc') or path.startswith(('dev/', 'proc/')) for path in workspace.entries)
