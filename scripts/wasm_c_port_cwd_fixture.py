"""Check shared C cwd support and actual application-relative file access."""

C_PORT_CWD_CHECK = r'''
import hashlib, os, subprocess

payload = b'C_PORT_CWD_ORACLE\0\xff'
def child(arguments, expected=b'', **options):
    result = subprocess.run(arguments, capture_output=True, **options)
    assert (result.returncode, result.stdout, result.stderr) == (0, expected, b''), result

os.makedirs('/c-port-cwd/metadata/base')
os.makedirs('/c-port-cwd/metadata/other/inner')
with open('/c-port-cwd/metadata/base/value', 'wb') as target:
    target.write(b'bad')
with open('/c-port-cwd/metadata/other/value', 'wb') as target:
    target.write(b'correct')
os.symlink('../other/inner', '/c-port-cwd/metadata/base/link')
child(['cwd_probe', '/c-port-cwd/metadata/base', 'metadata'], b'C_CWD_OK\n',
      cwd='/c-port-cwd/metadata/base')
child(['ash', '-c', 'test -d . && ! test -L . && test -L /proc/self/cwd && find . -type f'],
      b'./value\n', cwd='/c-port-cwd/metadata/base')

os.makedirs('/c-port-cwd/before/work')
with open('/c-port-cwd/before/work/input', 'wb') as target:
    target.write(payload)
child(['cwd_probe', '/c-port-cwd/moved/work', 'rename'], b'C_CWD_OK\n',
      cwd='/c-port-cwd/before/work')
os.makedirs('/c-port-cwd/before/work')
with open('/c-port-cwd/before/work/input', 'wb') as target:
    target.write(b'decoy')
cwd = '/c-port-cwd/moved/work'
os.chdir(cwd)
os.chmod(cwd, 0)
child(['cwd_probe', cwd, 'inherited'], b'C_CWD_OK\n')
os.chmod(cwd, 0o755)
os.chdir('/')
with open(cwd + '/opts', 'w') as target:
    target.write('url = "file://' + cwd + '/input"\noutput = "download"\n')
with open('/c-port-cwd/before/work/opts', 'w') as target:
    target.write('url = "file:///must-not-exist"\n')
child(['curl', '-q', '-sS', '--config', 'opts'], cwd=cwd)
assert open(cwd + '/download', 'rb').read() == payload
# Both the upload source and config paths use inherited cwd, while the URL is absolute.
child(['curl', '-q', '-sS', '-T', 'input', 'file://' + cwd + '/uploaded'], cwd=cwd)
assert open(cwd + '/uploaded', 'rb').read() == payload
expected = cwd + '\n' + hashlib.sha256(payload).hexdigest() + '  input\n'
script = ('const r = await ash`pwd -P; sha256sum input`;'
          'if(r.stdout !== ' + repr(expected) + ') throw Error(r.stdout);'
          'print("QUICKJS_CWD_OK");')
child(['quickjs', '-e', script], b'QUICKJS_CWD_OK\n', cwd=cwd)
with open(cwd + '/password', 'wb') as target:
    target.write(b'secret\r\n')
with open('/c-port-cwd/before/work/password', 'wb') as target:
    target.write(b'incorrect')
ssh = subprocess.run(ssh_arguments + ['--password-file', 'password', '--', 'nonzero'],
                     cwd=cwd, capture_output=True)
assert (ssh.returncode, ssh.stdout, ssh.stderr) == (23, b'remote-output\n', b'remote-error\n'), ssh

# A waiting C process keeps its anchor after rename and old-name reuse.
process = subprocess.Popen(['cwd_probe', '/c-port-cwd/final/work', 'wait'],
                           cwd=cwd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                           stderr=subprocess.PIPE)
os.rename('/c-port-cwd/moved', '/c-port-cwd/final')
os.makedirs('/c-port-cwd/moved/work')
with open('/c-port-cwd/moved/work/input', 'wb') as target:
    target.write(b'second-decoy')
out, err = process.communicate(b'x')
assert (process.returncode, out, err) == (0, b'C_CWD_OK\n', b''), (process.returncode, out, err)
os.mkdir('/c-port-cwd/deleted')
process = subprocess.Popen(['cwd_probe', 'deleted', 'wait'],
                           cwd='/c-port-cwd/deleted', stdin=subprocess.PIPE,
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE)
os.rmdir('/c-port-cwd/deleted')
out, err = process.communicate(b'x')
assert (process.returncode, out, err) == (0, b'C_CWD_OK\n', b''), (process.returncode, out, err)
os.mkdir('/tmp/c-port-long')
os.chdir('/tmp/c-port-long')
component = 'c' * 250
for _ in range(18):
    os.mkdir(component)
    os.chdir(component)
expected = '/tmp/c-port-long' + ('/' + component) * 18
assert len(expected) > 4096
child(['cwd_probe', expected, 'inspect'], b'C_CWD_OK\n')
os.chdir('/')
with open('c-port-cwd-result', 'wb') as target:
    target.write(b'C_PORT_CWD_OK\n')
print('C_PORT_CWD_OK')
'''


def c_port_cwd_check(host, port, pin):
    arguments = ['wolfssh', '--host', host, '--port', str(port),
                 '--user', 'guest', '--host-key-sha256', pin]
    return 'ssh_arguments = ' + repr(arguments) + '\n' + C_PORT_CWD_CHECK


def verify_c_port_cwd(workspace):
    payload = b'C_PORT_CWD_ORACLE\0\xff'
    assert workspace.read('c-port-cwd-result') == b'C_PORT_CWD_OK\n'
    for name in ('input', 'download', 'uploaded', 'probe-written'):
        assert workspace.read('c-port-cwd/final/work/' + name) == payload
    assert workspace.read('c-port-cwd/before/work/input') == b'decoy'
    assert workspace.read('c-port-cwd/moved/work/input') == b'second-decoy'
    assert 'c-port-cwd/deleted' not in workspace.entries
    assert not any(path == 'tmp' or path.startswith('tmp/') for path in workspace.entries)
