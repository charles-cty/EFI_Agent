# /// script
# requires-python = ">=3.11"
# ///
"""Run upstream xargs integration and verify independently saved child files."""
import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile

from wasm_namespace_fixture import DirectoryWorkspace
from wasm_xargs_fixture import XARGS_CHECK


def verify_xargs(workspace):
    assert workspace.read('xargs-result') == b'UPSTREAM_XARGS_OK\n'
    assert workspace.read('xargs-work/done-one') == b'one\n'
    assert workspace.read('xargs-work/done-two') == b'two\n'
    assert workspace.read('xargs-work/late-done') == b'done'
    assert workspace.entries['xargs-work/args']['mode'] == 0o700


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('runner', 'ash', 'python'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='efi-xargs-') as temporary:
        root = Path(temporary)
        for name in ('ash', 'python'):
            shutil.copyfile(getattr(args, name), root / (name + '.wasm'))
        shutil.copytree(args.python.parent / 'lib', root / 'lib')
        result = subprocess.run(
            [str(args.runner.resolve()), str(root), 'python', '-c', XARGS_CHECK],
            capture_output=True, timeout=240,
        )
        print(result.stdout.decode('utf-8', 'replace'), end='', flush=True)
        assert result.returncode == 0 and not result.stderr, result.stderr.decode('utf-8', 'replace')
        verify_xargs(DirectoryWorkspace(root))
        print('PASS independent saved xargs child completion bytes and modes')


if __name__ == '__main__':
    main()
