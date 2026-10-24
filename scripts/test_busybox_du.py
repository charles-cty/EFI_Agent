# /// script
# requires-python = ">=3.11"
# ///
"""Run actual du and independently query saved host file allocations."""
import argparse
import ctypes
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

from wasm_du_fixture import DU_CHECK
from wasm_namespace_fixture import DirectoryWorkspace


def host_allocated(path):
    if os.name != 'nt':
        return path.stat().st_blocks * 512
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    query = kernel.GetCompressedFileSizeW
    query.argtypes = (ctypes.c_wchar_p, ctypes.POINTER(ctypes.c_uint32))
    query.restype = ctypes.c_uint32
    high = ctypes.c_uint32()
    low = query(str(path), ctypes.byref(high))
    if low == 0xffffffff and ctypes.get_last_error():
        raise ctypes.WinError(ctypes.get_last_error())
    return (high.value << 32) | low


def make_sparse(path):
    with path.open('wb') as output:
        if os.name == 'nt':
            import msvcrt
            kernel = ctypes.WinDLL('kernel32', use_last_error=True)
            ioctl = kernel.DeviceIoControl
            ioctl.argtypes = (ctypes.c_void_p, ctypes.c_uint32, ctypes.c_void_p,
                              ctypes.c_uint32, ctypes.c_void_p, ctypes.c_uint32,
                              ctypes.POINTER(ctypes.c_uint32), ctypes.c_void_p)
            ioctl.restype = ctypes.c_int
            returned = ctypes.c_uint32()
            if not ioctl(msvcrt.get_osfhandle(output.fileno()), 0x900c4,
                         None, 0, None, 0, ctypes.byref(returned), None):
                raise ctypes.WinError(ctypes.get_last_error())
        output.seek(8 * 1024 * 1024)
        output.write(b'Z')
    assert host_allocated(path) < path.stat().st_size // 2


def verify_du(workspace, allocation):
    assert workspace.read('du-result') == b'UPSTREAM_DU_OK\n'
    measured = json.loads(workspace.read('du-work/measurements.json'))
    for name, (size, blocks, kibibytes) in measured.items():
        data = workspace.read(name)
        assert len(data) == size, name
        if name != 'host-sparse':
            assert all(value == index % 251 for index, value in enumerate(data)), name
        else:
            assert data == b'\0' * (8 * 1024 * 1024) + b'Z'
        allocated = allocation(name)
        assert blocks == (allocated + 511) // 512, (name, blocks, allocated)
        assert kibibytes == (blocks + 1) // 2, (name, kibibytes, blocks)
    assert 'du-work/tree/sub/large' not in workspace.entries
    assert workspace.entries['du-work/tree/link']['target'] == 'sub/large'
    assert workspace.entries['du-work/tree/dir-link']['target'] == 'sub'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('runner', 'ash', 'python'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='efi-du-') as temporary:
        root = Path(temporary)
        for name in ('ash', 'python'):
            shutil.copyfile(getattr(args, name), root / (name + '.wasm'))
        shutil.copytree(args.python.parent / 'lib', root / 'lib')
        make_sparse(root / 'host-sparse')
        result = subprocess.run(
            [str(args.runner.resolve()), str(root), 'python', '-c', DU_CHECK],
            capture_output=True, timeout=240)
        print(result.stdout.decode('utf-8', 'replace'), end='', flush=True)
        assert result.returncode == 0 and not result.stderr, result.stderr.decode('utf-8', 'replace')
        workspace = DirectoryWorkspace(root)
        verify_du(workspace, lambda name: host_allocated(root / workspace.entries[name]['backing']))
        print('PASS independent saved bytes and host allocation, including sparse files')


if __name__ == '__main__':
    main()
