# /// script
# requires-python = ">=3.11"
# ///
"""Verify upstream base64 through real Python/ash and persisted byte oracles."""
import argparse
import base64
from pathlib import Path
import shutil
import subprocess
import tempfile

from wasm_base64_fixture import BASE64_CHECK
from wasm_namespace_fixture import DirectoryWorkspace


def verify_base64(workspace):
    payload = bytes(range(256)) * 769 + b'\x00\xffZ'
    assert workspace.read('base64-result') == b'UPSTREAM_BASE64_OK\n'
    assert workspace.read('base64-input') == payload
    assert workspace.read('base64-encoded') == base64.b64encode(payload)
    assert workspace.read('base64-decoded') == payload
    assert workspace.read('base64-pipeline') == payload


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('runner', 'ash', 'python'):
        parser.add_argument(f'--{name}', type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='efi-base64-') as temporary:
        root = Path(temporary)
        for name in ('ash', 'python'):
            shutil.copyfile(getattr(args, name), root / f'{name}.wasm')
        shutil.copytree(args.python.parent / 'lib', root / 'lib')
        result = subprocess.run(
            [str(args.runner.resolve()), str(root), 'python', '-c', BASE64_CHECK],
            capture_output=True, timeout=240)
        print(result.stdout.decode('utf-8', 'replace'), end='', flush=True)
        assert result.returncode == 0 and not result.stderr, result.stderr
        verify_base64(DirectoryWorkspace(root))
        print('PASS independent saved base64 bytes')


if __name__ == '__main__':
    main()
