# /// script
# requires-python = ">=3.11"
# ///
"""Run upstream basic applets through real Python, ash and native wasmi."""
import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile

from wasm_basic_applets_fixture import BASIC_APPLETS_CHECK
from wasm_namespace_fixture import DirectoryWorkspace


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('runner', 'ash', 'python'):
        parser.add_argument(f'--{name}', type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='efi-basic-applets-') as temporary:
        root = Path(temporary)
        for name in ('ash', 'python'):
            shutil.copyfile(getattr(args, name), root / f'{name}.wasm')
        shutil.copytree(args.python.parent / 'lib', root / 'lib')
        result = subprocess.run([str(args.runner.resolve()), str(root), 'python', '-c', BASIC_APPLETS_CHECK],
                                capture_output=True, timeout=180)
        print(result.stdout.decode('utf-8', 'replace'), end='', flush=True)
        assert result.returncode == 0 and not result.stderr, result.stderr
        assert DirectoryWorkspace(root).read('basic-applets-result') == b'UPSTREAM_BASIC_APPLETS_OK\n'
        print('PASS independent saved basic applet result')


if __name__ == '__main__':
    main()
