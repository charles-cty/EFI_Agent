# /// script
# requires-python = ">=3.11"
# ///
"""Install prepared assets and run real Python and shell command dispatch."""
import argparse
from pathlib import Path
import subprocess
import tempfile

from wasm_installation_fixture import INSTALLATION_CHECK, verify_installation
from wasm_namespace_fixture import DirectoryWorkspace


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('runner', 'launcher', 'prepared'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='efi-installation-') as temporary:
        root = Path(temporary)
        subprocess.run([str(args.launcher.resolve()), 'install', str(args.prepared.resolve()),
                        str(root), '--guest'], check=True, timeout=120)
        result = subprocess.run([str(args.runner.resolve()), str(root), 'python', '-c',
                                 INSTALLATION_CHECK], capture_output=True, timeout=180)
        print(result.stdout.decode('utf-8', 'replace'), end='', flush=True)
        assert result.returncode == 0 and not result.stderr, result.stderr.decode('utf-8', 'replace')
        verify_installation(DirectoryWorkspace(root), args.prepared)
        print('PASS installed aliases, applications, libraries and saved bytes; notices stay outside workspace')


if __name__ == '__main__':
    main()
