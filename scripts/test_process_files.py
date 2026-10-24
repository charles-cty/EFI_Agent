# /// script
# requires-python = ">=3.11"
# ///
"""Run real Python/ash process-file checks and inspect saved namespace bytes."""
import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile

from wasm_process_files_fixture import PROCESS_FILES_CHECK, verify_process_files
from wasm_namespace_fixture import DirectoryWorkspace


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('runner', 'ash', 'python'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='efi-process-files-') as temporary:
        root = Path(temporary)
        for name in ('ash', 'python'):
            shutil.copyfile(getattr(args, name), root / (name + '.wasm'))
        shutil.copytree(args.python.parent / 'lib', root / 'lib')
        result = subprocess.run([str(args.runner.resolve()), str(root), 'python', '-c', PROCESS_FILES_CHECK],
                                capture_output=True, timeout=240)
        print(result.stdout.decode('utf-8', 'replace'), end='', flush=True)
        assert result.returncode == 0 and not result.stderr, result.stderr.decode('utf-8', 'replace')
        verify_process_files(DirectoryWorkspace(root))
        print('PASS independent saved bytes, hard-link identity and dev/proc exclusion')


if __name__ == '__main__':
    main()
