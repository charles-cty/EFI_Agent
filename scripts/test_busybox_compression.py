# /// script
# requires-python = ">=3.11"
# ///
"""Exercise gzip applets and independently decode their saved output."""
import argparse
import base64
import gzip
from pathlib import Path
import shutil
import subprocess
import tempfile

from wasm_namespace_fixture import DirectoryWorkspace
from wasm_compression_fixture import COMPRESSION_CHECK, ORACLE_DATA, vectors


def verify_compression(workspace):
    assert workspace.read('compression-result') == b'UPSTREAM_COMPRESSION_OK\n'
    for count in (0, 1, 32767, 32768, 32769, 65535, 65536, 65537,
                  524287, 524288, 524289, 1048576):
        decoded = gzip.decompress(workspace.read(f'compression/roundtrip-{count}.gz'))
        assert len(decoded) == count
        assert all(value == position % 251 for position, value in enumerate(decoded))
    for level in ('1', '4', '6', '9', 'fast', 'best'):
        assert gzip.decompress(workspace.read(f'compression/compressed-{level}.gz')) == ORACLE_DATA
    assert workspace.read('compression/plain') == ORACLE_DATA
    assert workspace.entries['compression/plain']['mode'] == 0o640
    assert workspace.read('compression/archive.tar') == ORACLE_DATA
    assert workspace.read('compression/timestamp') == ORACLE_DATA
    assert workspace.entries['compression/timestamp']['modified'] == 1600000000000000000
    assert 'compression/plain.gz' not in workspace.entries
    assert 'compression/broken' not in workspace.entries
    broken = bytearray(base64.b64decode(vectors()['level-9']))
    broken[-4] ^= 1
    assert workspace.read('compression/broken.gz') == bytes(broken)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('runner', 'ash', 'python'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='efi-compression-') as temporary:
        root = Path(temporary)
        for name in ('ash', 'python'):
            shutil.copyfile(getattr(args, name), root / (name + '.wasm'))
        shutil.copytree(args.python.parent / 'lib', root / 'lib')
        result = subprocess.run(
            [str(args.runner.resolve()), str(root), 'python', '-c', COMPRESSION_CHECK],
            capture_output=True, timeout=240,
        )
        print(result.stdout.decode('utf-8', 'replace'), end='', flush=True)
        assert result.returncode == 0 and not result.stderr, result.stderr.decode('utf-8', 'replace')
        verify_compression(DirectoryWorkspace(root))
        print('PASS independent host gzip decoding and saved bytes/modes/timestamps')


if __name__ == '__main__':
    main()
