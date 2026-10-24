# /// script
# requires-python = ">=3.11"
# ///
"""Run actual unzip and verify saved bytes and metadata independently."""
import argparse
import calendar
import io
from pathlib import Path
import shutil
import subprocess
import tempfile
import zipfile

from wasm_namespace_fixture import DirectoryWorkspace
from wasm_unzip_fixture import UNZIP_CHECK, ZIP_DATA, ZIP_TIME


def verify_unzip(workspace):
    assert workspace.read('unzip-result') == b'UPSTREAM_UNZIP_OK\n'
    assert workspace.read('unzip-work/out/deep/inner/data.bin') == ZIP_DATA
    assert workspace.read('unzip-work/out/script.sh') == b'#!/bin/ash\necho zip\n'
    assert workspace.entries['unzip-work/out/script.sh']['mode'] == 0o751
    assert workspace.entries['unzip-work/out/deep']['mode'] == 0o500
    timestamp = calendar.timegm(ZIP_TIME) * 1000000000
    for name in ('deep', 'deep/inner', 'deep/inner/data.bin', 'script.sh'):
        assert workspace.entries['unzip-work/out/' + name]['modified'] == timestamp
    assert workspace.entries['unzip-work/links/deflated-link']['target'] == 'target'
    assert workspace.entries['unzip-work/links/stored-link']['target'] == 'target'
    assert workspace.read('unzip-work/outside/keep') == b'untouched'
    assert 'unzip-work/outside/new' not in workspace.entries
    assert 'unzip-work/escape' not in workspace.entries and 'absolute' not in workspace.entries
    binary = workspace.read('unzip-work/boundary/binary')
    assert len(binary) == 524289
    assert all(value == index % 251 for index, value in enumerate(binary))
    with zipfile.ZipFile(io.BytesIO(workspace.read('unzip-work/boundary.zip'))) as source:
        assert source.read('binary') == binary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('runner', 'ash', 'python'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='efi-unzip-') as temporary:
        root = Path(temporary)
        for name in ('ash', 'python'):
            shutil.copyfile(getattr(args, name), root / (name + '.wasm'))
        shutil.copytree(args.python.parent / 'lib', root / 'lib')
        result = subprocess.run(
            [str(args.runner.resolve()), str(root), 'python', '-c', UNZIP_CHECK],
            capture_output=True, timeout=240,
        )
        print(result.stdout.decode('utf-8', 'replace'), end='', flush=True)
        assert result.returncode == 0 and not result.stderr, result.stderr.decode('utf-8', 'replace')
        verify_unzip(DirectoryWorkspace(root))
        print('PASS independent ZIP saved bytes, modes, timestamps and containment')


if __name__ == '__main__':
    main()
