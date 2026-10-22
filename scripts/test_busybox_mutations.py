# /// script
# requires-python = ">=3.11"
# ///
"""Run mutation applets through unchanged CPython and verify saved metadata."""
import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile

from wasm_mutation_applets_fixture import MUTATION_APPLETS_CHECK
from wasm_namespace_fixture import DirectoryWorkspace


def verify_mutation_applets(workspace):
    assert workspace.read('mutation-applets-result') == b'UPSTREAM_MUTATION_APPLETS_OK\n'
    for name in ('parents', 'masked', 'restricted', 'ordinary', 'prompt', 'blocked',
                 'unreadable', 'verbose', 'parent-link', 'dangling', 'invalid-date'):
        assert 'mutation-applets/' + name not in workspace.entries, name
    reference = workspace.entries['mutation-applets/reference']
    assert reference['accessed'] == 1600000000000000000, reference
    assert reference['modified'] == 1600000000000000000, reference
    link = workspace.entries['mutation-applets/stamp-link']
    assert link['target'] == 'reference' and link['modified'] == 1500000000000000000, link


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('runner', 'ash', 'python'):
        parser.add_argument(f'--{name}', type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='efi-mutation-applets-') as temporary:
        root = Path(temporary)
        for name in ('ash', 'python'):
            shutil.copyfile(getattr(args, name), root / f'{name}.wasm')
        shutil.copytree(args.python.parent / 'lib', root / 'lib')
        result = subprocess.run([str(args.runner.resolve()), str(root), 'python', '-c', MUTATION_APPLETS_CHECK],
                                capture_output=True, timeout=180)
        print(result.stdout.decode('utf-8', 'replace'), end='', flush=True)
        assert result.returncode == 0 and not result.stderr, result.stderr
        verify_mutation_applets(DirectoryWorkspace(root))
        print('PASS independent saved mutation names, bytes and timestamps')


if __name__ == '__main__':
    main()
