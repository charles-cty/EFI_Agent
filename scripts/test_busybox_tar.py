# /// script
# requires-python = ">=3.11"
# ///
"""Run upstream tar and independently inspect saved archives and metadata."""
import argparse
import io
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile

from wasm_namespace_fixture import DirectoryWorkspace
from wasm_tar_fixture import TAR_CHECK


def verify_tar(workspace):
    assert workspace.read('tar-result') == b'UPSTREAM_TAR_OK\n'
    binary = bytes(index % 251 for index in range(524289))
    assert workspace.read('tar-work/copied/binary') == binary
    assert workspace.entries['tar-work/copied/binary']['inode'] == workspace.entries['tar-work/copied/hard']['inode']
    assert workspace.entries['tar-work/copied/sym']['target'] == 'binary'
    assert workspace.entries['tar-work/copied/binary']['mode'] == 0o640
    assert workspace.entries['tar-work/copied/binary']['modified'] == 1700000123000000000
    assert workspace.read('tar-work/out/locked/data') == b'locked data\x00'
    assert workspace.entries['tar-work/out/locked']['mode'] == 0o500
    assert workspace.entries['tar-work/out/locked']['modified'] == 1700000123000000000
    for size in (0, 1, 511, 512, 513, 65535, 65536, 65537):
        assert workspace.read('tar-work/out/boundary-' + str(size)) == bytes(
            index % 251 for index in range(size))
    assert workspace.read('tar-work/outside/keep') == b'untouched'
    assert workspace.read('tar-work/guard/safe') == b'safe'
    assert 'tar-work/safe' not in workspace.entries
    assert 'tar-work/guard/bad-link' not in workspace.entries
    with tarfile.open(fileobj=io.BytesIO(workspace.read('tar-work/created.tar'))) as source:
        members = {item.name.lstrip('./'): item for item in source}
        regular = members['binary'] if members['binary'].isreg() else members['hard']
        assert source.extractfile(regular).read() == binary
        assert regular.mode == 0o640 and regular.mtime == 1700000123
        assert members['sym'].linkname == 'binary'
    for name in ('created.tar.gz', 'automatic.tgz'):
        with tarfile.open(fileobj=io.BytesIO(workspace.read('tar-work/' + name)), mode='r:gz') as source:
            assert source.extractfile('binary').read() == binary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('runner', 'ash', 'python'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='efi-tar-') as temporary:
        root = Path(temporary)
        for name in ('ash', 'python'):
            shutil.copyfile(getattr(args, name), root / (name + '.wasm'))
        shutil.copytree(args.python.parent / 'lib', root / 'lib')
        result = subprocess.run([str(args.runner.resolve()), str(root), 'python', '-c', TAR_CHECK],
                                capture_output=True, timeout=300)
        print(result.stdout.decode('utf-8', 'replace'), end='', flush=True)
        assert result.returncode == 0 and not result.stderr, result.stderr.decode('utf-8', 'replace')
        verify_tar(DirectoryWorkspace(root))
        print('PASS independent tar archive bytes, metadata, links and containment')


if __name__ == '__main__':
    main()
