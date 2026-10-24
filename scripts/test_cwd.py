# /// script
# requires-python = ">=3.11"
# ///
"""Run real Python/ash cwd checks and inspect the saved logical namespace."""
import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile

from wasm_cwd_fixture import CWD_CHECK, verify_cwd
from wasm_namespace_fixture import DirectoryWorkspace


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('runner', 'ash', 'python'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='efi-cwd-') as temporary:
        root = Path(temporary)
        for name in ('ash', 'python'):
            shutil.copyfile(getattr(args, name), root / (name + '.wasm'))
        shutil.copytree(args.python.parent / 'lib', root / 'lib')
        result = subprocess.run([str(args.runner.resolve()), str(root), 'python', '-c', CWD_CHECK],
                                capture_output=True, timeout=300)
        print(result.stdout.decode('utf-8', 'replace'), end='', flush=True)
        assert result.returncode == 0 and not result.stderr, result.stderr.decode('utf-8', 'replace')
        verify_cwd(DirectoryWorkspace(root))
        print('PASS independent saved cwd bytes, removed names and temporary mount exclusion')


if __name__ == '__main__':
    main()
