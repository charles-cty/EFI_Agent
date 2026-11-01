# /// script
# requires-python = ">=3.11"
# ///
"""Verify process, archive and line-editor applets with independent host data."""
import argparse
import ctypes
import ctypes.util
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile
import zlib

from wasm_namespace_fixture import DirectoryWorkspace


CHECK = r'''
import os, pathlib, subprocess
print('START BusyBox tools', flush=True)
P = pathlib.Path
def run(args, data=None, status=0, cwd=None):
    result = subprocess.run(args, input=data, capture_output=True, cwd=cwd)
    assert result.returncode == status, (args, result.returncode, result.stdout, result.stderr)
    return result.stdout

P('edit').write_bytes(b'one\ntwo\nthree\n')
run(['ed', 'edit'], b'2c\nsecond\n.\n1,$p\nw\nq\n')
assert P('edit').read_bytes() == b'one\nsecond\nthree\n'
assert run(['more', 'edit']) == P('edit').read_bytes()
assert run(['less', 'edit']) == P('edit').read_bytes()
run(['vi', 'edit'], status=1)
print('PASS ed and redirected pagers', flush=True)

assert run(['ar', 't', 'oracle.ar']) == b'odd\nlong-member-name.txt\n'
assert run(['ar', 'p', 'oracle.ar', 'odd']) == b'abc'
os.mkdir('ar-out')
run(['ar', 'x', '../oracle.ar'], cwd='ar-out')
assert P('ar-out/long-member-name.txt').read_bytes() == b'long archive member\n'
run(['ar', 'rc', 'created.ar', 'edit', 'payload'])
P('edit').write_bytes(b'replaced\n')
run(['ar', 'r', 'created.ar', 'edit'])
assert run(['ar', 'p', 'created.ar', 'edit']) == b'replaced\n'
print('PASS ar list, extract, create and replace', flush=True)

os.mkdir('cpio-out')
run(['cpio', '-idmu', '-F', '../oracle.cpio'], cwd='cpio-out')
assert P('cpio-out/sub/file').read_bytes() == b'cpio bytes\n'
assert os.readlink('cpio-out/link') == 'sub/file'
assert os.path.samefile('cpio-out/sub/file', 'cpio-out/hard')
assert P('cpio-out/sub/file').stat().st_mode & 0o777 == 0o640
assert b'sub/file\n' in run(['cpio', '-t', '-F', 'oracle.cpio'])
run(['cpio', '-p', 'cpio-out'], b'edit\n', status=1)
P('created.cpio').write_bytes(run(['cpio', '-o', '-H', 'newc'], b'edit\npayload\n'))
run(['cpio', '-t', '-F', 'created.cpio'])
print('PASS cpio newc, links and permissions', flush=True)

payload = P('payload').read_bytes()
assert run(['lzop', '-dc', 'oracle.lzo']) == payload
P('created.lzo').write_bytes(run(['lzop', '-c', 'payload']))
assert run(['lzop', '-dc', 'created.lzo']) == payload
print('PASS lzop compression and decompression', flush=True)

children = []
try:
    for marker in ['applet-first', 'applet-second', 'applet-keep']:
        child = subprocess.Popen(['sh', '-c', 'echo ready; read line', marker],
                                 stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        assert child.stdout.readline() == b'ready\n'
        children.append(child)
    first, second, keep = children
    listing = run(['ps', '-o', 'pid,ppid,user,group,stat,comm,args']).decode()
    for child in children:
        row = next(line.split(None, 6) for line in listing.splitlines()[1:]
                   if line.split()[0] == str(child.pid))
        assert row[1] == str(os.getpid()), row
        assert row[2:4] == ['agent', 'agent'], row
        assert row[4] in ['R', 'S'], row
        assert row[5] == 'sh', row
    run(['ps', '-o', 'rss'], status=1)
    matched = run(['pgrep', '-f', '-P', str(os.getpid()), 'applet-(first|second)$']).split()
    assert set(map(int, matched)) == {first.pid, second.pid}, matched
    assert int(run(['pgrep', '-n', '-f', '-P', str(os.getpid()), 'applet-'])) == keep.pid
    assert int(run(['pgrep', '-o', '-f', '-P', str(os.getpid()), 'applet-'])) == first.pid
    run(['pgrep', '-s', '0', 'ash'], status=1)
    run(['kill', '-0', str(first.pid)])
    run(['kill', '-19', str(first.pid)], status=1)
    run(['pkill', '-19', '-f', '-P', str(os.getpid()), 'applet-first$'], status=1)
    assert first.poll() is None and keep.poll() is None
    run(['kill', str(first.pid)])
    assert first.wait() == 143
    run(['kill', '-0', str(first.pid)], status=1)
    run(['pkill', '-KILL', '-f', '-P', str(os.getpid()), 'applet-second$'])
    assert second.wait() == 137
    assert keep.poll() is None
    assert keep.communicate(b'done\n') == (b'', b'')
    assert keep.returncode == 0
finally:
    for child in children:
        if child.poll() is None:
            child.communicate(b'cleanup\n')
print('BUSYBOX_TOOLS_OK')
'''


def ar_entry(name, data):
    header = f'{name:<16}{0:<12}{1:<6}{1:<6}{"100644":<8}{len(data):<10}`\n'.encode()
    assert len(header) == 60
    return header + data + (b'\n' if len(data) % 2 else b'')


def newc_entry(name, data, inode, mode=0o100640, links=1):
    fields = [inode, mode, 1, 1, links, 0, len(data), 0, 0, 0, 0, len(name) + 1, 0]
    header = b'070701' + ''.join(f'{value:08x}' for value in fields).encode()
    named = header + name.encode() + b'\0'
    named += bytes(-len(named) % 4)
    return named + data + bytes(-len(data) % 4)


def read_newc(data):
    entries = {}
    offset = 0
    while True:
        assert data[offset:offset + 6] == b'070701'
        fields = [int(data[offset + 6 + index * 8:offset + 14 + index * 8], 16) for index in range(13)]
        name = data[offset + 110:offset + 109 + fields[11]].decode()
        offset = (offset + 110 + fields[11] + 3) & ~3
        body = data[offset:offset + fields[6]]
        offset = (offset + fields[6] + 3) & ~3
        if name == 'TRAILER!!!':
            return entries
        entries[name] = body


def read_ar(data):
    assert data[:8] == b'!<arch>\n'
    offset = 8
    entries = {}
    while offset < len(data):
        header = data[offset:offset + 60]
        assert header[58:] == b'`\n'
        name = header[:16].decode().strip().rstrip('/')
        size = int(header[48:58])
        offset += 60
        entries[name] = data[offset:offset + size]
        offset += size
        # ar permits the last odd member to end without an alignment byte.
        if size % 2 and offset < len(data):
            assert data[offset:offset + 1] == b'\n'
            offset += 1
    assert offset == len(data)
    return entries


class LzoOracle:
    def __init__(self, library):
        self.library = ctypes.CDLL(library)
        signature = [ctypes.c_void_p, ctypes.c_size_t, ctypes.c_void_p,
                     ctypes.POINTER(ctypes.c_size_t), ctypes.c_void_p]
        for name in ('lzo1x_1_compress', 'lzo1x_decompress_safe'):
            function = getattr(self.library, name)
            function.argtypes = signature
            function.restype = ctypes.c_int

    def encode(self, data):
        header = struct.pack('>HHHBBIIII B', 0x1040, 0x20a0, 0x0940, 1, 3,
                             1, 0o100644, 0, 0, 0)
        result = b'\x89LZO\0\r\n\x1a\n' + header + struct.pack('>I', zlib.adler32(header))
        for offset in range(0, len(data), 65536):
            block = data[offset:offset + 65536]
            output = ctypes.create_string_buffer(len(block) + len(block) // 16 + 67)
            size = ctypes.c_size_t(len(output))
            work = ctypes.create_string_buffer(1024 * 1024)
            assert self.library.lzo1x_1_compress(block, len(block), output, ctypes.byref(size), work) == 0
            encoded = output.raw[:size.value] if size.value < len(block) else block
            result += struct.pack('>III', len(block), len(encoded), zlib.adler32(block)) + encoded
        return result + bytes(4)

    def decode(self, data):
        assert data[:9] == b'\x89LZO\0\r\n\x1a\n'
        flags = struct.unpack_from('>I', data, 17)[0]
        end = 34 + data[33]
        checksum = zlib.crc32 if flags & 0x1000 else zlib.adler32
        assert struct.unpack_from('>I', data, end)[0] == checksum(data[9:end])
        offset = end + 4
        result = bytearray()
        while True:
            size = struct.unpack_from('>I', data, offset)[0]
            offset += 4
            if size == 0:
                assert offset == len(data)
                return bytes(result)
            encoded_size = struct.unpack_from('>I', data, offset)[0]
            offset += 4
            expected = []
            for mask, function in [(1, zlib.adler32), (0x100, zlib.crc32),
                                   (2, zlib.adler32), (0x200, zlib.crc32)]:
                if flags & mask and (mask in (1, 0x100) or encoded_size < size):
                    expected.append((mask, function, struct.unpack_from('>I', data, offset)[0]))
                    offset += 4
            encoded = data[offset:offset + encoded_size]
            offset += encoded_size
            block = encoded
            if encoded_size < size:
                output = ctypes.create_string_buffer(size)
                count = ctypes.c_size_t(size)
                assert self.library.lzo1x_decompress_safe(encoded, encoded_size, output, ctypes.byref(count), None) == 0
                assert count.value == size
                block = output.raw
            for mask, function, value in expected:
                assert function(block if mask in (1, 0x100) else encoded) == value
            result.extend(block)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('runner', 'ash', 'python'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--lzo-library', default=ctypes.util.find_library('lzo2'), required=False)
    args = parser.parse_args()
    if not args.lzo_library:
        parser.error('Supply an independent liblzo2 shared library with --lzo-library')
    oracle = LzoOracle(args.lzo_library)
    payload = b'archive compression boundary\n' * 9001 + bytes(range(256))
    with tempfile.TemporaryDirectory(prefix='efi-busybox-tools-') as temporary:
        root = Path(temporary)
        for name in ('ash', 'python'):
            shutil.copyfile(getattr(args, name), root / f'{name}.wasm')
        shutil.copytree(args.python.parent / 'lib', root / 'lib')
        (root / 'payload').write_bytes(payload)
        (root / 'oracle.lzo').write_bytes(oracle.encode(payload))
        (root / 'oracle.ar').write_bytes(b'!<arch>\n' + ar_entry('//', b'long-member-name.txt/\n')
                                       + ar_entry('odd/', b'abc') + ar_entry('/0', b'long archive member\n'))
        cpio = newc_entry('sub', b'', 1, 0o40755)
        cpio += newc_entry('sub/file', b'cpio bytes\n', 2, links=2)
        cpio += newc_entry('hard', b'', 2, links=2)
        cpio += newc_entry('link', b'sub/file', 3, 0o120777)
        cpio += newc_entry('TRAILER!!!', b'', 0)
        (root / 'oracle.cpio').write_bytes(cpio)
        result = subprocess.run([str(args.runner.resolve()), str(root), 'python', '-c', CHECK],
                                capture_output=True, timeout=180)
        print(result.stdout.decode('utf-8', 'replace'), end='', flush=True)
        assert result.returncode == 0, result.stderr.decode('utf-8', 'replace')
        assert b'BUSYBOX_TOOLS_OK' in result.stdout
        saved = DirectoryWorkspace(root)
        assert saved.read('edit') == b'replaced\n'
        assert read_ar(saved.read('created.ar')) == {'edit': b'replaced\n', 'payload': payload}
        assert read_newc(saved.read('created.cpio')) == {'edit': b'replaced\n', 'payload': payload}
        assert oracle.decode(saved.read('created.lzo')) == payload
        print('PASS 11 applet names: process isolation/status, archive oracles, ed saved bytes and redirected pagers')


if __name__ == '__main__':
    main()
