"""Exercise standard cwd interfaces against retained directory identities."""

CWD_CHECK = r'''
import errno, io, os, subprocess, sys

def child(arguments, expected, **options):
    result = subprocess.run(arguments, capture_output=True, **options)
    assert (result.returncode, result.stdout, result.stderr) == (0, expected, b''), result

os.makedirs('/cwd-tree/a/work')
os.chdir('/cwd-tree/a/work')
inode = os.stat('.').st_ino
with open('payload', 'wb') as target:
    target.write(b'original\0bytes')
os.rename('/cwd-tree/a', '/cwd-tree/moved')
os.makedirs('/cwd-tree/a/work')
with open('/cwd-tree/a/work/payload', 'wb') as target:
    target.write(b'replacement')
assert os.getcwd() == '/cwd-tree/moved/work'
assert os.getcwdb() == b'/cwd-tree/moved/work'
assert os.readlink('/proc/self/cwd') == '/cwd-tree/moved/work'
assert os.stat('.').st_ino == inode
assert os.stat('/proc/self/cwd').st_ino == inode
with open('payload', 'rb') as source:
    assert source.read() == b'original\0bytes'
with io.FileIO('payload') as source:
    assert source.read() == b'original\0bytes'
with open('data', 'wb') as target:
    target.write(b'boundary')
assert open('./' * 2046 + 'data', 'rb').read() == b'boundary'
for index in reversed(range(40)):
    os.symlink('payload' if index == 39 else 'link' + str(index + 1), 'link' + str(index))
assert open('link0', 'rb').read() == b'original\0bytes'
os.symlink('link0', 'link-extra')
try:
    open('link-extra', 'rb')
except OSError as error:
    assert error.errno == errno.ELOOP
else:
    raise AssertionError('relative walk accepted 41 symbolic links')
for name in ['data', 'link-extra'] + ['link' + str(index) for index in range(40)]:
    os.unlink(name)
directory_fd = os.open('.', os.O_RDONLY | os.O_DIRECTORY)
try:
    os.stat('..', dir_fd=directory_fd)
except OSError as error:
    assert error.errno == errno.ENOTCAPABLE
else:
    raise AssertionError('directory fd escaped its anchor')
finally:
    os.close(directory_fd)
child(['python', '-c', 'import os;print(os.getcwd());print(open("payload","rb").read().hex())'],
      b'/cwd-tree/moved/work\n6f726967696e616c006279746573\n')
child(['ash', '-c', 'pwd -P; cat payload'], b'/cwd-tree/moved/work\noriginal\0bytes')
with open('cwd_module.py', 'w') as target:
    target.write('value = 731\n')
sys.path.insert(0, '.')
import cwd_module
assert cwd_module.value == 731 and os.path.isfile(cwd_module.__cached__)
os.unlink(cwd_module.__cached__)
os.rmdir('__pycache__')
os.unlink('cwd_module.py')
sys.path.pop(0)
with open('tool', 'w') as target:
    target.write('#!/bin/ash\npwd -P\n')
os.chmod('tool', 0o755)
child(['./tool'], b'/cwd-tree/moved/work\n')
os.unlink('tool')
child(['python', '-c', 'import os;print(os.getcwd())'], b'/cwd-tree/moved\n', cwd='..')
os.symlink('/cwd-tree/moved/work', '/cwd-tree/route')
os.chdir('/cwd-tree/route/..')
assert os.getcwd() == '/cwd-tree/moved'
os.chdir('work')
os.mkdir('/cwd-tree/denied')
os.chmod('/cwd-tree/denied', 0)
try:
    os.chdir('/cwd-tree/denied')
except PermissionError:
    pass
else:
    raise AssertionError('chdir ignored search permission')
assert os.stat('.').st_ino == inode
os.chmod('/cwd-tree/denied', 0o755)
os.rmdir('/cwd-tree/denied')
os.chmod('.', 0)
for options in ({}, {'umask': 0o077}):
    child(['python', '-c', 'import os;print(os.getcwd())'],
          b'/cwd-tree/moved/work\n', **options)
try:
    subprocess.run(['python', '-c', 'pass'], cwd='.', capture_output=True)
except PermissionError:
    pass
else:
    raise AssertionError('explicit child cwd ignored search permission')
os.chmod('/cwd-tree/moved/work', 0o755)

# A waiting child keeps its inherited directory when its parent changes cwd.
code = 'import os,sys;sys.stdin.buffer.read(1);print(os.getcwd());print(open("payload","rb").read().hex())'
process = subprocess.Popen(['python', '-c', code], stdin=subprocess.PIPE,
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE)
os.chdir('/')
os.rename('/cwd-tree/moved', '/cwd-tree/final')
out, err = process.communicate(b'x')
assert (process.returncode, out, err) == (0, b'/cwd-tree/final/work\n6f726967696e616c006279746573\n', b'')
os.chdir('/cwd-tree/final/work')
os.unlink('payload')
parent_inode = os.stat('..').st_ino
os.rmdir('/cwd-tree/final/work')
os.rmdir('/cwd-tree/final')
for function in (os.getcwd, os.getcwdb):
    try:
        function()
    except FileNotFoundError as error:
        assert error.errno == errno.ENOENT
    else:
        raise AssertionError('detached cwd has a pathname')
assert os.stat('.').st_ino == inode and os.listdir('.') == []
assert os.stat('/proc/self/cwd').st_ino == inode
assert os.readlink('/proc/self/cwd').endswith(' (deleted)')
os.chdir('.')
for mask in (None, 0o077):
    options = {} if mask is None else {'umask': mask}
    child(['python', '-c', 'import os;print(os.stat(".").st_ino)'],
          (str(inode) + '\n').encode(), **options)
child(['ash', '-c', 'test -d .; cd ..; test -d .; cd ..; pwd -P'], b'/cwd-tree\n')
try:
    open('must-not-exist', 'wb')
except FileNotFoundError:
    pass
else:
    raise AssertionError('created a file in an unlinked directory')
os.chdir('..')
assert os.stat('.').st_ino == parent_inode
os.chdir('..')
assert os.getcwd() == '/cwd-tree'
os.chdir('/tmp')
os.makedirs('cwd-parent/child')
os.chdir('cwd-parent/child')
os.rename('/tmp/cwd-parent', '/tmp/cwd-moved')
assert os.getcwd() == '/tmp/cwd-moved/child'
child(['ash', '-c', 'cd ..; pwd -P; cd ..; pwd -P; cd ..; pwd -P'],
      b'/tmp/cwd-moved\n/tmp\n/\n')
child(['ash', '-c', 'mkdir -p /tmp/shell-before/work; cd /tmp/shell-before/work; '
       'mv /tmp/shell-before /tmp/shell-after; printf retained > data; '
       'pwd -P; cat data; printf "\\n"; (pwd -P); cd -P ..; pwd -P'],
      b'/tmp/shell-after/work\nretained\n/tmp/shell-after/work\n/tmp/shell-after\n')
os.mkdir('/tmp/cwd-long')
os.chdir('/tmp/cwd-long')
component = 'd' * 250
for _ in range(18):
    os.mkdir(component)
    os.chdir(component)
expected = '/tmp/cwd-long' + ('/' + component) * 18
assert len(expected) > 4096 and os.getcwd() == expected
child(['ash', '-c', 'pwd -P'], (expected + '\n').encode())
os.chdir('/')
with open('cwd-result', 'wb') as target:
    target.write(b'CWD_ANCHORS_OK\n')
print('CWD_ANCHORS_OK')
'''


def verify_cwd(workspace):
    assert workspace.read('cwd-result') == b'CWD_ANCHORS_OK\n'
    assert workspace.read('cwd-tree/a/work/payload') == b'replacement'
    assert workspace.entries['cwd-tree/route']['target'] == '/cwd-tree/moved/work'
    assert not any(path == 'cwd-tree/final' or path.startswith('cwd-tree/final/')
                   for path in workspace.entries)
    assert not any(path == 'tmp' or path.startswith('tmp/') for path in workspace.entries)
