# /// script
# requires-python = ">=3.11"
# ///
"""Verify upstream realpath through real ash and independent path oracles."""
import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile

from wasm_namespace_fixture import DirectoryWorkspace
from wasm_realpath_fixture import REALPATH_CHECK


def verify_realpath(workspace):
    assert workspace.read('realpath-result') == b'UPSTREAM_REALPATH_OK\n'
    assert workspace.read('real-link-target') == b'real-dir'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('runner', 'ash', 'python'):
        parser.add_argument(f'--{name}', type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='efi-realpath-') as temporary:
        root = Path(temporary)
        for name in ('ash', 'python'):
            shutil.copyfile(getattr(args, name), root / f'{name}.wasm')
        shutil.copytree(args.python.parent / 'lib', root / 'lib')
        result = subprocess.run([str(args.runner.resolve()), str(root), 'python', '-c', REALPATH_CHECK],
                                capture_output=True, timeout=120)
        print(result.stdout.decode('utf-8', 'replace'), end='', flush=True)
        assert result.returncode == 0 and not result.stderr, result.stderr
        workspace = DirectoryWorkspace(root)
        verify_realpath(workspace)
        print('PASS independent saved realpath bytes')


if __name__ == '__main__':
    main()
