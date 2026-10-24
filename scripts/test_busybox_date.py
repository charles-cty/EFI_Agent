# /// script
# requires-python = ">=3.11"
# ///
"""Run upstream date through actual CPython and check saved reference metadata."""
import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile

from wasm_date_fixture import DATE_CHECK
from wasm_namespace_fixture import DirectoryWorkspace


def verify_date(workspace):
    assert workspace.read('date-result') == b'UPSTREAM_DATE_OK\n'
    assert workspace.read('date-reference') == b'reference remains unchanged\0'
    reference = workspace.entries['date-reference']
    assert reference['accessed'] == 1111111111222333444, reference
    assert reference['modified'] == 1234567890555666777, reference


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('runner', 'ash', 'python'):
        parser.add_argument(f'--{name}', type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='efi-date-') as temporary:
        root = Path(temporary)
        for name in ('ash', 'python'):
            shutil.copyfile(getattr(args, name), root / f'{name}.wasm')
        shutil.copytree(args.python.parent / 'lib', root / 'lib')
        shutil.copytree(args.ash.parent / 'share/zoneinfo', root / 'share/zoneinfo')
        result = subprocess.run([str(args.runner.resolve()), str(root), 'python', '-c', DATE_CHECK],
                                capture_output=True, timeout=180)
        print(result.stdout.decode('utf-8', 'replace'), end='', flush=True)
        assert result.returncode == 0 and not result.stderr, result.stderr
        verify_date(DirectoryWorkspace(root))
        print('PASS independent saved date reference bytes and nanoseconds')


if __name__ == '__main__':
    main()
