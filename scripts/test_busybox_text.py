# /// script
# requires-python = ">=3.11"
# ///
"""Run upstream text applets through real Python/ash and independent saved bytes."""
import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile

from wasm_text_applets_fixture import TEXT_APPLETS_CHECK
from wasm_namespace_fixture import DirectoryWorkspace


def verify_text_applets(workspace):
    assert workspace.read('text-applets-result') == b'UPSTREAM_TEXT_APPLETS_OK\n'
    assert workspace.read('text-input') == b'a\na\nb\nz\n'
    assert workspace.read('text-output') == b'a\nb\nz\n'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('runner', 'ash', 'python'):
        parser.add_argument(f'--{name}', type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='efi-text-applets-') as temporary:
        root = Path(temporary)
        for name in ('ash', 'python'):
            shutil.copyfile(getattr(args, name), root / f'{name}.wasm')
        shutil.copytree(args.python.parent / 'lib', root / 'lib')
        result = subprocess.run([str(args.runner.resolve()), str(root), 'python', '-c', TEXT_APPLETS_CHECK],
                                capture_output=True, timeout=240)
        print(result.stdout.decode('utf-8', 'replace'), end='', flush=True)
        assert result.returncode == 0 and not result.stderr, result.stderr
        verify_text_applets(DirectoryWorkspace(root))
        print('PASS independent saved text applet bytes')


if __name__ == '__main__':
    main()
