# /// script
# requires-python = ">=3.11"
# ///
"""Run real CPython against executable replacement and inspect saved bytes."""
import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile

from wasm_executable_freshness_fixture import (
    EXECUTABLE_FRESHNESS_CHECK, executable_image, verify_executable_freshness,
)
from wasm_namespace_fixture import DirectoryWorkspace


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('runner', 'python'):
        parser.add_argument('--' + name, required=True, type=Path)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='efi-executable-freshness-') as temporary:
        root = Path(temporary)
        shutil.copyfile(args.python, root / 'python.wasm')
        shutil.copytree(args.python.parent / 'lib', root / 'lib')
        (root / 'fresh_image.wasm').write_bytes(executable_image(7))
        result = subprocess.run([str(args.runner.resolve()), str(root), 'python', '-c',
                                 EXECUTABLE_FRESHNESS_CHECK], capture_output=True, timeout=120)
        print(result.stdout.decode('utf-8', 'replace'), end='', flush=True)
        assert result.returncode == 0 and not result.stderr, result.stderr.decode('utf-8', 'replace')
        verify_executable_freshness(DirectoryWorkspace(root))
        print('PASS live old image, same-size/time writes, replacement, unlink and independent saved bytes')


if __name__ == '__main__':
    main()
