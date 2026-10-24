"""Independent small WASI images and live executable replacement checks."""

IMAGE_BUILDER = r'''
def executable_image(code):
    assert 0 <= code < 64
    def name(text):
        data = text.encode()
        return bytes([len(data)]) + data
    def section(kind, data):
        assert len(data) < 128
        return bytes([kind, len(data)]) + data
    types = b'\x03\x60\x01\x7f\x00\x60\x04\x7f\x7f\x7f\x7f\x01\x7f\x60\x00\x00'
    imports = b'\x02' + name('wasi_snapshot_preview1') + name('proc_exit') + b'\x00\x00'
    imports += name('wasi_snapshot_preview1') + name('fd_read') + b'\x00\x01'
    exports = b'\x02' + name('memory') + b'\x02\x00' + name('_start') + b'\x00\x02'
    # Read one stdin byte before exit, so the parent can retain a live old Store.
    body = b'\x00\x41\x08\x41\x10\x36\x02\x00\x41\x0c\x41\x01\x36\x02\x00'
    body += b'\x41\x00\x41\x08\x41\x01\x41\x00\x10\x01\x1a'
    body += b'\x41' + bytes([code]) + b'\x10\x00\x0b'
    return (b'\0asm\x01\0\0\0' + section(1, types) + section(2, imports)
            + section(3, b'\x01\x02') + section(5, b'\x01\x00\x01')
            + section(7, exports) + section(10, b'\x01' + bytes([len(body)]) + body))
'''

EXECUTABLE_FRESHNESS_CHECK = IMAGE_BUILDER + r'''
import errno, os, subprocess

def run(command, code):
    result = subprocess.run([command], input=b'x', capture_output=True)
    assert (result.returncode, result.stdout, result.stderr) == (code, b'', b''), result

path = '/fresh_image.wasm'
os.chmod(path, 0o755)
run('fresh_image', 7)
before = os.stat(path)
old = subprocess.Popen(['fresh_image'], stdin=subprocess.PIPE,
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE)
assert old.poll() is None
with open(path, 'r+b') as target:
    assert target.write(executable_image(9)) == before.st_size
os.utime(path, ns=(before.st_atime_ns, before.st_mtime_ns))
after = os.stat(path)
assert (after.st_ino, after.st_size, after.st_mtime_ns) == (before.st_ino, before.st_size, before.st_mtime_ns)
run('fresh_image', 9)
assert old.poll() is None
out, err = old.communicate(b'x')
assert (old.returncode, out, err) == (7, b'', b'')
os.link(path, '/fresh_alias.wasm')
run('/fresh_alias.wasm', 9)
with open('/fresh_replacement', 'wb') as target:
    target.write(executable_image(13))
os.chmod('/fresh_replacement', 0o755)
os.replace('/fresh_replacement', path)
run('fresh_image', 13)
run('/fresh_alias.wasm', 9)
with open(path, 'wb'):
    pass
try:
    subprocess.run(['fresh_image'])
except OSError as error:
    assert error.errno == errno.ENOEXEC, error
else:
    raise AssertionError('empty replacement ran a cached image')
os.unlink(path)
try:
    subprocess.run(['fresh_image'])
except FileNotFoundError:
    pass
else:
    raise AssertionError('deleted executable ran a cached image')
with open(path, 'wb') as target:
    target.write(executable_image(13))
os.chmod(path, 0o755)
run('fresh_image', 13)
with open('/executable-freshness-result', 'wb') as target:
    target.write(b'EXECUTABLE_FRESHNESS_OK\n')
print('EXECUTABLE_FRESHNESS_OK')
'''

exec(IMAGE_BUILDER)


def verify_executable_freshness(workspace):
    assert workspace.read('executable-freshness-result') == b'EXECUTABLE_FRESHNESS_OK\n'
    assert workspace.read('fresh_image.wasm') == executable_image(13)
    assert workspace.read('fresh_alias.wasm') == executable_image(9)
    assert workspace.entries['fresh_image.wasm']['inode'] != workspace.entries['fresh_alias.wasm']['inode']
    assert 'fresh_replacement' not in workspace.entries
