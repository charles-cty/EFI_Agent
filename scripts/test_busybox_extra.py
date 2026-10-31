# /// script
# requires-python = ">=3.11"
# ///
"""Verify added upstream applets with real guests and independent host oracles."""
import argparse
import bz2
import hashlib
import json
import lzma
from pathlib import Path
import shutil
import subprocess
import tempfile
import textwrap
import zlib

from wasm_namespace_fixture import DirectoryWorkspace


CHECK = r'''
import json, os, pathlib, subprocess
P = pathlib.Path
def run(arguments, data=None, status=0):
    result = subprocess.run(arguments, input=data, capture_output=True)
    assert result.returncode == status, (arguments, result.returncode, result.stdout, result.stderr)
    return result.stdout

data = P('payload').read_bytes()
for size in [0, 1, 4095, 4096, 65537]:
    name = 'hash-' + str(size)
    for algorithm in ['md5', 'sha1', 'sha256', 'sha512']:
        expected = json.loads(P('hashes').read_text())[name][algorithm]
        assert run([algorithm + 'sum', name]).split()[0].decode() == expected
assert run(['cksum'], b'abc') == b'1219131554 3\n'
assert run(['crc32', 'payload']).split()[0].decode() == P('crc').read_text()
run(['sha512sum', 'missing'], status=1)

run(['dd', 'if=payload', 'of=dd-result', 'bs=13', 'skip=2', 'count=7', 'status=none'])
assert P('dd-result').read_bytes() == data[26:117]
P('seek-result').write_bytes(b'0123456789abcdef')
run(['dd', 'of=seek-result', 'bs=3', 'seek=2', 'conv=notrunc', 'status=none'], b'XYZ')
assert P('seek-result').read_bytes() == b'012345XYZ9abcdef'
run(['dd', 'if=payload', 'of=dd-aligned', 'bs=65536', 'status=none'])
assert P('dd-aligned').read_bytes() == data
run(['truncate', '-s', '7', 'resized'])
assert P('resized').read_bytes() == bytes(7)
P('resized').write_bytes(b'abcd')
run(['truncate', '-s', '2', 'resized'])
run(['truncate', '-s', '9', 'resized'])
assert P('resized').read_bytes() == b'ab' + bytes(7)
run(['split', '-b', '4096', 'payload', 'chunk-'])
assert b''.join(path.read_bytes() for path in sorted(P('.').glob('chunk-*'))) == data
assert all(path.stat().st_size == 4096 for path in sorted(P('.').glob('chunk-*'))[:-1])

temporary = run(['mktemp', '/tmp/extra.XXXXXX']).decode().strip()
directory = run(['mktemp', '-d', '/tmp/extra.XXXXXX']).decode().strip()
assert P(temporary).stat().st_mode & 0o777 == 0o600
assert P(directory).stat().st_mode & 0o777 == 0o700
assert not run(['mktemp', '-q', '/tmp/invalid.XXXXX'], status=1)
os.symlink('payload', 'extra-link')
assert run(['readlink', 'extra-link']) == b'payload\n'
assert run(['readlink', '-f', 'extra-link']) == b'/payload\n'
run(['readlink', 'payload'], status=1)

P('left').write_bytes(b'a\nb\n')
P('right').write_bytes(b'b\nc\n')
assert run(['paste', 'left', 'right']) == b'a\tb\nb\tc\n'
assert run(['comm', '-12', 'left', 'right']) == b'b\n'
assert run(['tac'], b'a\nb\nlast') == b'lastb\na\n'
assert run(['nl', '-ba', '-w1', '-s:'], b'a\n\nb\n') == b'1:a\n2:\n3:b\n'
assert run(['fold', '-b', '-w3'], b'abcdefg\n') == b'abc\ndef\ng\n'
assert run(['strings'], b'\x00abc\x00abcdef\xff') == b'abcdef\n'
assert not run(['cmp', '-s', 'left', 'left'])
assert not run(['cmp', '-s', 'left', 'right'], status=1)
assert run(['expr', '1000000000', '*', '3']) == b'3000000000\n'
assert run(['expr', 'match', 'abcdef', r'\(abc\).*']) == b'abc\n'
assert run(['bc'], b'2^100\nscale=8;1/7\n') == b'1267650600228229401496703205376\n.14285714\n'
assert run(['dc'], b'2 100 ^ p\n') == b'1267650600228229401496703205376\n'

P('old').write_bytes(b'one\ntwo\nthree\n')
P('new').write_bytes(b'one\nsecond\nthree\nfour\n')
patch = run(['diff', '-u', 'old', 'new'], status=1)
P('change.patch').write_bytes(patch)
run(['patch', '--dry-run', 'old', 'change.patch'])
assert P('old').read_bytes() == b'one\ntwo\nthree\n'
run(['patch', 'old', 'change.patch'])
assert P('old').read_bytes() == P('new').read_bytes()
run(['patch', '-R', 'old', 'change.patch'])
assert P('old').read_bytes() == b'one\ntwo\nthree\n'

P('encoded.bz2').write_bytes(run(['bzip2', '-c', 'payload']))
assert run(['bunzip2', '-c', 'oracle.bz2']) == data
assert run(['bzip2', '-dc', 'oracle.bz2']) == data
assert run(['unxz', '-c', 'oracle.xz']) == data
assert run(['xz', '-dc', 'oracle.xz']) == data
assert run(['lzma', '-dc', 'oracle.lzma']) == data
print('EXTRA_APPLETS_OK')
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('runner', 'ash', 'python'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    payload = bytes((index * 37 + index // 17) % 256 for index in range(90001))
    with tempfile.TemporaryDirectory(prefix='efi-extra-applets-') as temporary:
        root = Path(temporary)
        for name in ('ash', 'python'):
            shutil.copyfile(getattr(args, name), root / f'{name}.wasm')
        shutil.copytree(args.python.parent / 'lib', root / 'lib')
        (root / 'payload').write_bytes(payload)
        (root / 'oracle.bz2').write_bytes(bz2.compress(payload))
        (root / 'oracle.xz').write_bytes(lzma.compress(payload))
        (root / 'oracle.lzma').write_bytes(lzma.compress(payload, format=lzma.FORMAT_ALONE))
        (root / 'crc').write_text(f'{zlib.crc32(payload):08x}', encoding='ascii')
        expected = {}
        for size in (0, 1, 4095, 4096, 65537):
            name = f'hash-{size}'
            (root / name).write_bytes(payload[:size])
            expected[name] = {algorithm: hashlib.new(algorithm, payload[:size]).hexdigest()
                              for algorithm in ('md5', 'sha1', 'sha256', 'sha512')}
        (root / 'hashes').write_text(json.dumps(expected), encoding='ascii')
        result = subprocess.run([str(args.runner.resolve()), str(root), 'python', '-c', textwrap.dedent(CHECK)],
                                capture_output=True, timeout=240)
        print(result.stdout.decode('utf-8', 'replace'), end='', flush=True)
        assert result.returncode == 0, result.stderr.decode('utf-8', 'replace')
        assert b'EXTRA_APPLETS_OK' in result.stdout
        saved = DirectoryWorkspace(root)
        assert bz2.decompress(saved.read('encoded.bz2')) == payload
        assert saved.read('dd-aligned') == payload
        assert saved.read('dd-result') == payload[26:117]
        assert saved.read('seek-result') == b'012345XYZ9abcdef'
        assert saved.read('resized') == b'ab' + bytes(7)
        print('PASS 27 added applets, boundary hashes, independent bzip2 and saved bytes')


if __name__ == '__main__':
    main()
