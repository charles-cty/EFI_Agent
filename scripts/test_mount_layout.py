# /// script
# requires-python = ">=3.11"
# ///
"""Verify native mount-conflict failures and independently inspect saved data."""
import argparse
from pathlib import Path
import subprocess
import tempfile

from wasm_namespace_fixture import DirectoryWorkspace


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runner', type=Path, required=True)
    args = parser.parse_args()
    for name in ['dev', 'tmp', 'proc', 'bin']:
        kinds = ['file'] if name == 'bin' else ['file', 'empty-directory', 'directory']
        for kind in kinds:
            with tempfile.TemporaryDirectory(prefix='efi-mount-layout-') as temporary:
                root = Path(temporary)
                path = root / name
                if kind == 'file':
                    path.write_bytes(b'original workspace bytes\x00\xff\n')
                else:
                    path.mkdir()
                    if kind == 'directory':
                        (path / 'child').write_bytes(b'original child bytes\x00\xff\n')
                result = subprocess.run(
                    [str(args.runner.resolve()), str(root), 'absent'],
                    capture_output=True, timeout=30)
                diagnostic = result.stderr.decode('utf-8', 'replace')
                assert result.returncode != 0 and not result.stdout, (kind, result)
                assert 'MountError' in diagnostic and f'/{name}' in diagnostic and 'Exists' in diagnostic, diagnostic
                saved = DirectoryWorkspace(root)
                if kind == 'file':
                    assert saved.read(name) == b'original workspace bytes\x00\xff\n'
                else:
                    assert saved.entries[name]['kind'] == 'Directory'
                    if kind == 'directory':
                        assert saved.read(name + '/child') == b'original child bytes\x00\xff\n'
                print(f'PASS mount conflict /{name} {kind} and independent saved data', flush=True)


if __name__ == '__main__':
    main()
