# /// script
# requires-python = ">=3.11"
# ///
"""Verify upstream stat through real ash and independent saved metadata."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

from wasm_namespace_fixture import DirectoryWorkspace
from wasm_stat_fixture import STAT_CHECK
from test_busybox_df import host_capacity


def verify_stat(workspace, capacity):
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
    assert workspace.read('stat-filesystem-terse') == (
        b'/tmp 3 1024 1021994 1 33554432 33554432 33554432 4097 4096\n'
    )
    fields = json.loads(workspace.read('stat-filesystem-root'))
    block, fragment, total, free, available, files, files_free = map(int, fields[4:11])
    host_total, host_free, host_available = capacity
    assert block == fragment and total * fragment <= host_total < (total + 1) * fragment, fields
    # Metadata and result files are committed after the statistics snapshot.
    assert abs(free * fragment - host_free) < 4 * 1024 * 1024, (fields, capacity)
    assert abs(available * fragment - host_available) < 4 * 1024 * 1024, (fields, capacity)
    assert 0 <= files_free <= files == 8193, fields


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
        verify_stat(DirectoryWorkspace(root), host_capacity(root))
        print('PASS independent saved stat metadata')


if __name__ == '__main__':
    main()
