# /// script
# requires-python = ">=3.11"
# ///
"""Verify upstream sha256sum through ash and independent saved bytes."""
import argparse
import hashlib
from pathlib import Path
import shutil
import subprocess
import tempfile

from wasm_namespace_fixture import DirectoryWorkspace
from wasm_sha256_fixture import SHA256_CHECK


def verify_sha256(workspace):
    payloads = {'sha-empty': b'', 'sha-one': b'one\x00\xff',
                'sha-many': bytes(range(256)) * 257}
    expected = b''.join(hashlib.sha256(payload).hexdigest().encode() + b'  ' + path.encode() + b'\n'
                        for path, payload in payloads.items())
    assert workspace.read('sha-result') == b'UPSTREAM_SHA256_OK\n'
    assert workspace.read('sha-checksums') == expected
    for path, payload in payloads.items():
        assert workspace.read(path) == payload


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('runner', 'ash', 'python'):
        parser.add_argument(f'--{name}', type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='efi-sha256-') as temporary:
        root = Path(temporary)
        for name in ('ash', 'python'):
            shutil.copyfile(getattr(args, name), root / f'{name}.wasm')
        shutil.copytree(args.python.parent / 'lib', root / 'lib')
        result = subprocess.run([str(args.runner.resolve()), str(root), 'python', '-c', SHA256_CHECK],
                                capture_output=True, timeout=300)
        print(result.stdout.decode('utf-8', 'replace'), end='', flush=True)
        assert result.returncode == 0 and not result.stderr, result.stderr
        verify_sha256(DirectoryWorkspace(root))
        print('PASS independent saved sha256 bytes')


if __name__ == '__main__':
    main()
