# /// script
# requires-python = ">=3.11"
# ///
"""Run real Python/ash device integration and independently inspect saved bytes."""
import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile

from wasm_character_devices_fixture import CHARACTER_DEVICES_CHECK, verify_character_devices
from wasm_namespace_fixture import DirectoryWorkspace


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['runner', 'ash', 'python']:
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='efi-character-devices-') as temporary:
        root = Path(temporary)
        for name in ['ash', 'python']:
            shutil.copyfile(getattr(args, name), root / (name + '.wasm'))
        shutil.copytree(args.python.parent / 'lib', root / 'lib')
        result = subprocess.run(
            [str(args.runner.resolve()), str(root), 'python', '-c', CHARACTER_DEVICES_CHECK],
            capture_output=True, timeout=240)
        print(result.stdout.decode('utf-8', 'replace'), end='', flush=True)
        assert result.returncode == 0 and not result.stderr, result.stderr.decode('utf-8', 'replace')
        verify_character_devices(DirectoryWorkspace(root))
        print('PASS independent saved zeros, random digests and device mount exclusion')


if __name__ == '__main__':
    main()
