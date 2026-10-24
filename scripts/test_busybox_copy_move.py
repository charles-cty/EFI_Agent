# /// script
# requires-python = ">=3.11"
# ///
"""Run upstream cp/mv through actual Python and ash; inspect saved data."""
import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile

from wasm_copy_move_fixture import COPY_MOVE_CHECK
from wasm_namespace_fixture import DirectoryWorkspace


def verify_copy_move(workspace):
    assert workspace.read('copy-move-result') == b'UPSTREAM_COPY_MOVE_OK\n'
    assert workspace.read('copy-move/source') == b'source\0\xff retained\n'
    assert workspace.read('copy-move/move-dest') == b'moved'
    assert workspace.entries['copy-move/move-dest']['mode'] == 0o700
    assert workspace.entries['copy-move/moved-tree/link']['target'] == 'deep/binary'
    assert workspace.entries['copy-move/moved-tree/plain']['mode'] == 0o750
    assert workspace.entries['copy-move/moved-tree/plain']['modified'] == 1600000000000000000
    assert workspace.read('copy-move/moved-tree/deep/binary') == bytes(range(256)) * 257
    assert 'copy-move/move-source' not in workspace.entries
    assert 'copy-move/archive' not in workspace.entries
    count = 524289
    assert workspace.read('copy-move/boundary-copy') == (bytes(range(251)) * (count // 251 + 1))[:count]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('runner', 'ash', 'python'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='efi-copy-move-') as temporary:
        root = Path(temporary)
        for name in ('ash', 'python'):
            shutil.copyfile(getattr(args, name), root / (name + '.wasm'))
        shutil.copytree(args.python.parent / 'lib', root / 'lib')
        result = subprocess.run(
            [str(args.runner.resolve()), str(root), 'python', '-c', COPY_MOVE_CHECK],
            capture_output=True, timeout=180,
        )
        print(result.stdout.decode('utf-8', 'replace'), end='', flush=True)
        assert result.returncode == 0 and not result.stderr, result.stderr.decode('utf-8', 'replace')
        verify_copy_move(DirectoryWorkspace(root))
        print('PASS independent saved copy/move bytes, modes, links and times')


if __name__ == '__main__':
    main()
