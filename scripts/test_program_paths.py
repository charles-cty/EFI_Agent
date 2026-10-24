# /// script
# requires-python = ">=3.11"
# ///
"""Verify guest PATH, shell precedence, executable modes and shebangs."""
import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile

from wasm_namespace_fixture import DirectoryWorkspace
from wasm_program_paths_fixture import PROGRAM_PATHS_CHECK


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('runner', 'ash', 'python'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='efi-program-paths-') as temporary:
        root = Path(temporary)
        for name in ('ash', 'python'):
            shutil.copyfile(getattr(args, name), root / (name + '.wasm'))
        shutil.copytree(args.python.parent / 'lib', root / 'lib')
        (root / 'bin').mkdir()
        (root / 'bin/workspace-entry').write_bytes(b'workspace command data')
        result = subprocess.run(
            [str(args.runner.resolve()), str(root), 'python', '-c', PROGRAM_PATHS_CHECK],
            capture_output=True, timeout=120,
        )
        print(result.stdout.decode('utf-8', 'replace'), end='', flush=True)
        assert result.returncode == 0 and not result.stderr, result.stderr.decode('utf-8', 'replace')
        workspace = DirectoryWorkspace(root)
        assert workspace.read('program-paths-result') == b'PROGRAM_PATHS_OK\n'
        assert workspace.entries['path-one/pick']['mode'] == 0o600
        assert workspace.read('path-two/pick') == b'#!/bin/ash\nprintf path-two\n'
        print('PASS independent saved script bytes and executable modes')


if __name__ == '__main__':
    main()
