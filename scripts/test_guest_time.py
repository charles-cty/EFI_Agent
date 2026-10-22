# /// script
# requires-python = ">=3.11"
# ///
"""Verify real guest waits against guest and independent host elapsed time."""
import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

from wasm_time_fixture import TIME_CHECK
from wasm_namespace_fixture import DirectoryWorkspace


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('runner', 'ash', 'python'):
        parser.add_argument(f'--{name}', type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='efi-time-') as temporary:
        root = Path(temporary)
        for name in ('ash', 'python'):
            shutil.copyfile(getattr(args, name), root / f'{name}.wasm')
        shutil.copytree(args.python.parent / 'lib', root / 'lib')
        result = subprocess.run([str(args.runner.resolve()), str(root), 'python', '-c', TIME_CHECK],
                                capture_output=True, timeout=120)
        assert result.returncode == 0 and not result.stderr, result.stderr
        assert result.stdout == b'MONOTONIC_WAIT_OK\n', result.stdout
        assert DirectoryWorkspace(root).read('monotonic-result') == b'MONOTONIC_WAIT_OK\n'
        print('PASS actual Python time.sleep, monotonic reads, upstream sleep and ash timed reads')
        for command, lower in [('sleep .15', .15), ('sleep .05 .1', .15)]:
            script = f"printf 'READY\\n'; {command}; printf 'DONE\\n'"
            with subprocess.Popen([str(args.runner.resolve()), str(root), 'ash', '-c', script],
                                  stdout=subprocess.PIPE, stderr=subprocess.PIPE) as process:
                assert process.stdout.readline() == b'READY\n'
                before = time.monotonic()
                assert process.stdout.readline() == b'DONE\n'
                elapsed = time.monotonic() - before
                stdout, stderr = process.communicate(timeout=30)
                assert process.returncode == 0 and not stdout and not stderr, (stdout, stderr)
            # Exclude module compilation and startup. Allow only a small host
            # pipe-delivery interval between the first marker and its read.
            assert lower - .005 <= elapsed < 15, (command, elapsed)
        print('PASS independent host elapsed-time lower and upper bounds')


if __name__ == '__main__':
    main()
