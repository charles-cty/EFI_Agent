# /// script
# requires-python = ">=3.11"
# ///
"""Verify ephemeral mount semantics with actual Python and BusyBox processes."""
import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile

from wasm_namespace_fixture import DirectoryWorkspace
from wasm_temporary_fixture import TEMPORARY_CHECK, verify_temporary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('runner', 'ash', 'python'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='efi-tmp-mount-') as temporary:
        root = Path(temporary)
        for name in ('ash', 'python'):
            shutil.copyfile(getattr(args, name), root / (name + '.wasm'))
        shutil.copytree(args.python.parent / 'lib', root / 'lib')
        result = subprocess.run(
            [str(args.runner.resolve()), str(root), 'python', '-c', TEMPORARY_CHECK],
            capture_output=True, timeout=240)
        print(result.stdout.decode('utf-8', 'replace'), end='', flush=True)
        assert result.returncode == 0 and not result.stderr, result.stderr.decode('utf-8', 'replace')
        verify_temporary(DirectoryWorkspace(root))
        print('PASS independent saved bytes, metadata and temporary mount exclusion')


if __name__ == '__main__':
    main()
