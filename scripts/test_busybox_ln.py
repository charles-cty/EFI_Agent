# /// script
# requires-python = ">=3.11"
# ///
"""Run real ln and independently inspect persisted inode and backing groups."""
import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile

from wasm_ln_fixture import LN_CHECK
from wasm_namespace_fixture import DirectoryWorkspace


def verify_ln(workspace):
    assert workspace.read('ln-result') == b'UPSTREAM_LN_OK\n'
    paths = ['ln-work/python', 'ln-work/moved/alias', 'ln-work/moved/relative',
             'ln-work/cp-link', 'ln-work/replace', 'ln-work/closed-output']
    entries = [workspace.entries[path] for path in paths]
    assert len({entry['inode'] for entry in entries}) == 1, entries
    assert len({entry['backing'] for entry in entries}) == 1, entries
    assert all(entry['mode'] == 0o640 for entry in entries), entries
    for path in paths:
        assert workspace.read(path) == b'shared bytes\0\xff', path
    assert workspace.read('ln-work/replace.bak') == b'old'
    assert workspace.read('ln-work/source/first') == b'archive group'
    assert workspace.entries['ln-work/source/first']['inode'] == workspace.entries['ln-work/source/second']['inode']
    for first, second in [('ln-work/dangling', 'ln-work/dangling-alias'),
                          ('ln-work/link', 'ln-work/physical-link'),
                          ('ln-work/target', 'ln-work/followed')]:
        assert workspace.entries[first]['inode'] == workspace.entries[second]['inode']
    for absent in ['ln-work/a/file', 'ln-work/copy/first', 'ln-work/copy/second',
                   'ln-work/directory-hardlink', 'ln-work/absent', 'ln-work/no-write/denied']:
        assert absent not in workspace.entries, absent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('runner', 'ash', 'python'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='efi-ln-') as temporary:
        root = Path(temporary)
        for name in ('ash', 'python'):
            shutil.copyfile(getattr(args, name), root / (name + '.wasm'))
        shutil.copytree(args.python.parent / 'lib', root / 'lib')
        result = subprocess.run(
            [str(args.runner.resolve()), str(root), 'python', '-c', LN_CHECK],
            capture_output=True, timeout=240)
        print(result.stdout.decode('utf-8', 'replace'), end='', flush=True)
        assert result.returncode == 0 and not result.stderr, result.stderr.decode('utf-8', 'replace')
        verify_ln(DirectoryWorkspace(root))
        print('PASS independent saved hard-link inode groups, backing bytes and modes')


if __name__ == '__main__':
    main()
