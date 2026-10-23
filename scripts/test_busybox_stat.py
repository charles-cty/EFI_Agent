# /// script
# requires-python = ">=3.11"
# ///
"""Verify upstream stat through real ash and independent saved metadata."""
import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile

from wasm_namespace_fixture import DirectoryWorkspace
from wasm_stat_fixture import STAT_CHECK


def verify_stat(workspace):
    source = workspace.entries['stat-dir/file']
    assert source['kind'] == 'File' and source['mode'] == 0o640, source
    assert source['accessed'] == source['modified'] == 1700000123000000000, source
    link = workspace.entries['stat-dir/link']
    assert link['target'] == 'file', link
    assert workspace.read('stat-dir/file') == b'alpha\x00omega'
    assert workspace.read('stat-result') == (
        b'640|regular file|11|1|1|agent|agent|1|1700000123|1700000123\n'
    )
    assert workspace.read('stat-link-result') == b'symbolic link|4|stat-dir/link\n'
    assert workspace.read('stat-link-human-result') == b'SYMLINK_TARGET=file\n'
    assert workspace.read('stat-follow-result') == b'regular file|11|stat-dir/link\n'
    assert workspace.read('stat-multi-result') == (
        b'regular file|stat-dir/file\n'
        b'symbolic link|stat-dir/link\n'
        b'directory|stat-dir\n'
    )
    assert workspace.read('stat-error-result') == b'STAT_ERRORS_OK\n'
    assert workspace.read('stat-time-boundary') == b''.join(
        (str(value) + '|' + str(value) + '\n').encode()
        for value in (2147483647, 2147483648, 2200000000, 4294967296)
    )
    assert workspace.read('stat-input') == b'alpha\x00omega'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('runner', 'ash', 'python'):
        parser.add_argument(f'--{name}', type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='efi-stat-') as temporary:
        root = Path(temporary)
        for name in ('ash', 'python'):
            shutil.copyfile(getattr(args, name), root / f'{name}.wasm')
        shutil.copytree(args.python.parent / 'lib', root / 'lib')
        result = subprocess.run([str(args.runner.resolve()), str(root), 'python', '-c', STAT_CHECK],
                                capture_output=True, timeout=180)
        print(result.stdout.decode('utf-8', 'replace'), end='', flush=True)
        assert result.returncode == 0 and not result.stderr, result.stderr
        verify_stat(DirectoryWorkspace(root))
        print('PASS independent saved stat metadata')


if __name__ == '__main__':
    main()
