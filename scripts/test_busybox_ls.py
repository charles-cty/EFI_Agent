# /// script
# requires-python = ">=3.11"
# ///
"""Verify upstream ls output and saved workspace bytes through real ash."""
import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runner', type=Path, required=True)
    parser.add_argument('--ash', type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='efi-ls-') as temporary:
        root = Path(temporary)
        shutil.copyfile(args.ash, root / 'ash.wasm')
        command = r'''mkdir -p ls-tree/sub
printf a > ls-tree/alpha
printf h > ls-tree/.hidden
printf b > ls-tree/sub/beta
ls ls-tree
ls -A ls-tree
ls -a ls-tree
ls -d ls-tree ls-tree/alpha
ls -F ls-tree
ls -l ls-tree/alpha
ls -R ls-tree
ls missing-entry
'''
        result = subprocess.run([str(args.runner.resolve()), str(root), 'ash', '-c', command],
                                capture_output=True, timeout=60)
        expected = (
            b'alpha\nsub\n'
            b'.hidden\nalpha\nsub\n'
            b'.\n..\n.hidden\nalpha\nsub\n'
            b'ls-tree\nls-tree/alpha\n'
            b'alpha\nsub/\n'
        )
        assert result.returncode == 1, (result.returncode, result.stdout, result.stderr)
        assert result.stdout.startswith(expected), result.stdout
        assert b'ls-tree/sub:\nbeta\n' in result.stdout, result.stdout
        assert result.stdout.endswith(b'ls-tree/sub:\nbeta\n'), result.stdout
        assert b'missing-entry' in result.stderr, result.stderr
        print('PASS upstream ls options, recursive traversal, errors and saved bytes')


if __name__ == '__main__':
    main()
