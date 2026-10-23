# /// script
# requires-python = ">=3.11"
# ///
"""Run upstream ls through real ash and verify namespace output and links."""
import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile

from wasm_ls_fixture import LS_CHECK
from wasm_namespace_fixture import DirectoryWorkspace


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runner', type=Path, required=True)
    parser.add_argument('--ash', type=Path, required=True)
    parser.add_argument('--python', type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='efi-ls-') as temporary:
        root = Path(temporary)
        for name in ('ash', 'python'):
            shutil.copyfile(getattr(args, name), root / f'{name}.wasm')
        shutil.copytree(args.python.parent / 'lib', root / 'lib')
        result = subprocess.run(
            [str(args.runner.resolve()), str(root), 'python', '-c', LS_CHECK],
            capture_output=True,
            timeout=90,
        )
        print(result.stdout.decode('utf-8', 'replace'), end='', flush=True)
        assert result.returncode == 0 and not result.stderr, result.stderr
        workspace = DirectoryWorkspace(root)
        assert workspace.read('ls-result') == (
            b'alpha\ndangling\ndir-link\nlarge\nlink\nspace name\nsub\n\xe5\x90\x8d\n'
        )
        assert workspace.read('ls-tree/large') == b'L' * 32
        assert workspace.entries['ls-tree/link']['target'] == 'alpha'
        assert workspace.entries['ls-tree/dir-link']['target'] == 'sub'
        assert workspace.entries['ls-tree/dangling']['target'] == 'absent'
        print('PASS saved ls names, regular-file bytes and symbolic links')


if __name__ == '__main__':
    main()
