"""Exercise actual shell lookup and subprocess dispatch in the guest namespace."""
PROGRAM_PATHS_CHECK = r'''
import errno
import os
import subprocess

def shell(command, expected, status=0):
    result = subprocess.run(['/bin/ash', '-c', command], capture_output=True)
    assert (result.returncode, result.stdout) == (status, expected), (command, result)
    if status == 0:
        assert not result.stderr, (command, result)

shell('command -v ls; type ls', b'/bin/ls\nls is /bin/ls\n')
shell('PATH=/missing; command -v ls', b'', 127)
shell('PATH=/missing; ls', b'', 127)
shell('ls() { printf function; }; ls', b'function')
shell('PATH=/missing; printf builtin', b'builtin')
assert os.path.isfile('/bin/ls') and os.stat('/bin/ls').st_mode & 0o100
assert {'ls', 'workspace-entry'} <= set(os.listdir('/bin'))
assert open('/bin/workspace-entry', 'rb').read() == b'workspace command data'
assert os.readlink('/bin/python') == '/python.wasm'
with open('/bin/ls', 'rb') as image:
    assert image.read(8) == b'\0asm\x01\0\0\0'

for directory in ('path-one', 'path-two'):
    os.mkdir(directory)
    with open(directory + '/pick', 'w') as script:
        script.write('#!/bin/ash\nprintf ' + directory + '\n')
    os.chmod(directory + '/pick', 0o700)
shell('PATH=/path-one:/path-two:/bin; command -v pick; pick', b'/path-one/pick\npath-one')
shell('PATH=/path-two:/path-one:/bin; pick', b'path-two')
shell('PATH=/path-one:/path-two:/bin which -a pick', b'/path-one/pick\n/path-two/pick\n')
shell('PATH=/path-one:/bin pick; PATH=/path-two:/bin pick', b'path-onepath-two')
shell('cd /path-two; PATH=:/bin; pick; ./pick', b'path-twopath-two')
shell('/bin/env PATH=/path-two:/bin pick', b'path-two')
shell('exec /path-one/pick', b'path-one')
os.chmod('path-one/pick', 0o600)
shell('PATH=/path-one:/path-two:/bin; pick', b'path-two')
result = subprocess.run(['pick'], env={'PATH': '/path-two:/bin'}, capture_output=True)
assert (result.returncode, result.stdout, result.stderr) == (0, b'path-two', b''), result
result = subprocess.run(['./pick'], cwd='/path-two', capture_output=True)
assert (result.returncode, result.stdout, result.stderr) == (0, b'path-two', b''), result

with open('python-script', 'w') as script:
    script.write('#!/bin/python -I\nimport sys\nprint(sys.argv[1])\n')
os.chmod('python-script', 0o700)
shell('./python-script argument', b'argument\n')
for name, contents, expected in [
    ('bad-executable', 'ordinary text\n', errno.ENOEXEC),
    ('recursive-script', '#!/recursive-script\n', errno.ELOOP),
    ('denied-script', '#!/bin/ash\n', errno.EACCES),
]:
    with open(name, 'w') as script:
        script.write(contents)
    os.chmod(name, 0o600 if name == 'denied-script' else 0o700)
    try:
        subprocess.run(['/' + name], capture_output=True)
    except OSError as error:
        assert error.errno == expected, (name, error)
    else:
        raise AssertionError('Invalid executable started: ' + name)

with open('program-paths-result', 'w') as output:
    output.write('PROGRAM_PATHS_OK\n')
print('PROGRAM_PATHS_OK')
'''
