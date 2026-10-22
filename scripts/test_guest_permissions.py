# /// script
# requires-python = ">=3.11"
# ///
"""Check actual CPython/BusyBox permission effects and persisted namespace modes."""
import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile
from wasm_namespace_fixture import DirectoryWorkspace


CHECK = r'''
import errno, os, pathlib, stat, subprocess

def mode(path):
    return stat.S_IMODE(os.stat(path).st_mode)

def denied(function):
    try:
        function()
    except PermissionError as error:
        assert error.errno == errno.EACCES, error
    else:
        raise AssertionError('Permission operation succeeded')

assert os.getuid() == os.geteuid() == os.getgid() == os.getegid() == 1
with open('data', 'w') as output:
    output.write('old old\n')
fd = os.open('data', os.O_RDWR)
os.chmod('data', 0o400)
assert mode('data') == 0o400 and os.fstat(fd).st_mode & 0o7777 == 0o400
assert os.access('data', os.R_OK) and not os.access('data', os.W_OK)
denied(lambda: open('data', 'wb'))
assert os.write(fd, b'new') == 3
os.fchmod(fd, 0o600)
assert mode('data') == 0o600
os.unlink('data')
os.fchmod(fd, 0o640)
assert os.fstat(fd).st_mode & 0o7777 == 0o640
os.close(fd)
print('OPEN_HANDLE_OK')

os.mkdir('directory', 0o700)
assert mode('directory') == 0o700
pathlib.Path('directory/child').write_text('payload')
os.chmod('directory', 0o600)
denied(lambda: os.stat('directory/child'))
denied(lambda: os.chdir('directory'))
assert os.getcwd() == '/'
os.chmod('directory', 0o700)
os.symlink('directory/child', 'link')
os.chmod('link', 0o640)
assert mode('directory/child') == 0o640
assert stat.S_IMODE(os.lstat('link').st_mode) == 0o777
try:
    os.chmod('link', 0o000, follow_symlinks=False)
except OSError as error:
    assert error.errno == errno.ENOTSUP, error
else:
    raise AssertionError('Symlink permission change silently succeeded')
assert mode('directory/child') == 0o640
print('DIRECTORY_LINK_OK')

fd = os.open('directory', os.O_RDONLY)
os.chmod('child', 0o620, dir_fd=fd)
assert os.stat('child', dir_fd=fd).st_mode & 0o7777 == 0o620
os.rename('directory', 'renamed')
os.chmod('child', 0o640, dir_fd=fd)
assert mode('renamed/child') == 0o640
os.close(fd)
print('ANCHORED_MODE_OK')

with os.scandir('renamed') as entries:
    entry = next(entries)
    assert entry.name == 'child' and entry.is_file() and not entry.is_symlink()
    assert stat.S_IMODE(entry.stat().st_mode) == 0o640
with os.scandir(b'renamed') as entries:
    entry = next(entries)
    assert entry.name == b'child' and os.fspath(entry) == b'renamed/child'
    assert stat.S_IMODE(entry.stat().st_mode) == 0o640
fd = os.open('renamed', os.O_RDONLY)
with os.scandir(fd) as entries:
    entry = next(entries)
    assert entry.path == 'child' and os.fspath(entry) == 'child'
    os.rename('renamed', 'scan-renamed')
os.close(fd)
assert stat.S_IMODE(entry.stat().st_mode) == 0o640
del entry
os.rename('scan-renamed', 'renamed')
os.chdir('renamed')
for path in (None, '.', b'.'):
    with os.scandir(path) as entries:
        entry = next(entries)
        assert os.fsdecode(entry.name) == 'child'
        assert stat.S_IMODE(entry.stat().st_mode) == 0o640
os.chdir('/')
print('SCANDIR_MODES_OK')

os.chdir('renamed')
assert mode('child') == 0o640
os.chmod('child', 0o600)
assert mode('child') == 0o600
subprocess.run('chmod 640 child; test -r child && ! test -x child', shell=True, check=True)
subprocess.run('cd /; cd renamed; chmod 640 child; test -r child', shell=True, check=True)
assert mode('child') == 0o640
os.chdir('/')
print('RELATIVE_CWD_OK')

pathlib.Path('edit').write_text('old old\n')
os.chmod('edit', 0o640)
subprocess.run(['chmod', 'u+x,g-w,o=', 'edit'], check=True)
assert mode('edit') == 0o740
subprocess.run(['chmod', 'u=rw,g=u,o=', 'edit'], check=True)
assert mode('edit') == 0o660
subprocess.run(['chmod', '0640', 'edit'], check=True)
subprocess.run(['sed', '-i.bak', 's/old/new/g', 'edit'], check=True)
assert mode('edit') == mode('edit.bak') == 0o640
assert pathlib.Path('edit').read_text() == 'new new\n'
assert pathlib.Path('edit.bak').read_text() == 'old old\n'
subprocess.run('test -r edit && test -w edit && ! test -x edit', shell=True, check=True)
subprocess.run(['chmod', '-w', 'edit'], check=True)
assert mode('edit') == 0o440
denied(lambda: open('edit', 'w'))
subprocess.run(['chmod', '640', 'edit'], check=True)
print('BUSYBOX_MODE_SED_OK')

old = os.umask(0o077)
assert old == 0o022
with open('mask-file', 'w') as output:
    output.write('mask')
assert mode('mask-file') == 0o600
os.symlink('dangling-target', 'dangling-create')
fd = os.open('dangling-create', os.O_WRONLY | os.O_CREAT, 0o400)
os.close(fd)
assert mode('dangling-target') == 0o400
fd = os.open('dangling-create', os.O_RDONLY | os.O_CREAT, 0o777)
os.close(fd)
assert mode('dangling-target') == 0o400
try:
    os.open('dangling-create', os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o777)
except FileExistsError:
    pass
else:
    raise AssertionError('Exclusive creation followed an existing symlink')
with open('after-create-error', 'w') as output:
    output.write('default mode after failed exclusive creation')
assert mode('after-create-error') == 0o600
os.mkdir('mask-dir')
assert mode('mask-dir') == 0o700
fd = os.open('explicit-file', os.O_WRONLY | os.O_CREAT, 0o400)
assert os.fstat(fd).st_mode & 0o7777 == 0o400
os.close(fd)
# Existing files must keep their mode even when opened with O_CREAT.
fd = os.open('mask-file', os.O_WRONLY | os.O_CREAT, 0o777)
os.close(fd)
assert mode('mask-file') == 0o600
subprocess.run('test "$(umask)" = 0077; : > child-mask; mkdir child-dir', shell=True, check=True)
assert mode('child-mask') == 0o600 and mode('child-dir') == 0o700
assert os.umask(old) == 0o077
print('UMASK_INHERITANCE_OK')

os.mkdir('recursive')
os.mkdir('recursive/sub')
pathlib.Path('recursive/sub/file').write_bytes(b'bytes')
os.symlink('../edit', 'recursive/link')
subprocess.run(['chmod', '-R', 'u=rwX,go=', 'recursive'], check=True)
assert mode('recursive') == mode('recursive/sub') == 0o700
assert mode('recursive/sub/file') == 0o600
assert mode('edit') == 0o640
print('RECURSIVE_CHMOD_OK')

pathlib.Path('mode-cases').write_bytes(b'modes')
fd = os.open('mode-cases', os.O_RDONLY)
state = 731
for index in range(24):
    state = (state * 1103515245 + 12345) & 0x7fffffff
    expected = state & 0o7777
    os.chmod('mode-cases', expected)
    assert mode('mode-cases') == expected
    assert stat.S_IMODE(os.fstat(fd).st_mode) == expected
    for request, bit in ((os.R_OK, 0o400), (os.W_OK, 0o200), (os.X_OK, 0o100)):
        assert os.access('mode-cases', request) == bool(expected & bit)
os.close(fd)
os.chmod('mode-cases', 0o640)
os.utime('mode-cases', ns=(1234567890, 987654321))
value = os.stat('mode-cases')
assert value.st_atime_ns == 1234567890 and value.st_mtime_ns == 987654321
assert abs(value.st_atime - 1.234567890) < 1e-9
assert stat.S_ISREG(value.st_mode) and value.st_size == 5
print('MODE_BOUNDARIES_AND_STAT_FIELDS_OK')

for call in (lambda: os.chmod('missing', 0o600), lambda: os.stat('missing')):
    try:
        call()
    except FileNotFoundError:
        pass
    else:
        raise AssertionError('Missing file succeeded')
for value in (-1, 128):
    try:
        os.fchmod(value, 0o600)
    except OSError as error:
        assert error.errno == errno.EBADF, error
    else:
        raise AssertionError('Invalid fd succeeded')
print('ERRORS_OK')
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('runner', 'ash', 'python', 'metadata'):
        parser.add_argument(f'--{name}', type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='efi-permissions-') as temporary:
        root = Path(temporary)
        shutil.copyfile(args.ash, root / 'ash.wasm')
        shutil.copyfile(args.python, root / 'python.wasm')
        shutil.copyfile(args.metadata, root / 'metadata.wasm')
        shutil.copytree(args.python.parent / 'lib', root / 'lib')
        result = subprocess.run([str(args.runner.resolve()), str(root), 'python', '-c', CHECK],
                                capture_output=True, timeout=180)
        print(result.stdout.decode('utf-8', 'replace'), end='', flush=True)
        assert result.returncode == 0 and not result.stderr, result.stderr.decode('utf-8', 'replace')
        view = DirectoryWorkspace(root)
        expected = {'edit': 0o640, 'edit.bak': 0o640, 'mask-file': 0o600,
                    'mask-dir': 0o700, 'explicit-file': 0o400, 'renamed/child': 0o640,
                    'recursive': 0o700, 'recursive/sub/file': 0o600,
                    'dangling-target': 0o400, 'after-create-error': 0o600}
        for name, value in expected.items():
            assert view.entries[name]['mode'] == value, (name, view.entries[name])
        assert view.read('edit') == b'new new\n'
        print('PASS independent saved namespace modes/bytes', flush=True)
        result = subprocess.run([str(args.runner.resolve()), str(root), 'python', '-c',
                                 "import os;assert os.stat('edit').st_mode & 0o7777 == 0o640;assert os.umask(0o022)==0o022;print('RELOAD_OK')"],
                                capture_output=True, timeout=180)
        assert result.returncode == 0 and result.stdout == b'RELOAD_OK\n' and not result.stderr, result.stderr
        print('PASS actual process restart and persisted modes', flush=True)
        result = subprocess.run([str(args.runner.resolve()), str(root), 'metadata'],
                                capture_output=True, timeout=180)
        assert result.returncode == 0 and result.stdout == b'C_METADATA_ANCHORS_OK\n' and not result.stderr, result.stderr
        view = DirectoryWorkspace(root)
        for path in ('c-metadata/root', 'c-metadata/sub/local'):
            assert view.entries[path]['mode'] == 0o600, (path, view.entries[path])
        print('PASS real BusyBox libc wrappers with cwd and explicit root/directory fds', flush=True)


if __name__ == '__main__':
    main()
