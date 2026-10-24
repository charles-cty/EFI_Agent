# /// script
# requires-python = ">=3.11"
# ///
"""Run actual df and verify saved results against independent host capacity."""
import argparse
import ctypes
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

from wasm_df_fixture import DF_CHECK
from wasm_namespace_fixture import DirectoryWorkspace


def host_capacity(path):
    if os.name == 'nt':
        kernel = ctypes.WinDLL('kernel32', use_last_error=True)
        query = kernel.GetDiskFreeSpaceExW
        query.argtypes = (ctypes.c_wchar_p,) + (ctypes.POINTER(ctypes.c_uint64),) * 3
        query.restype = ctypes.c_int
        available, total, free = ctypes.c_uint64(), ctypes.c_uint64(), ctypes.c_uint64()
        if not query(str(path), ctypes.byref(available), ctypes.byref(total), ctypes.byref(free)):
            raise ctypes.WinError(ctypes.get_last_error())
        return total.value, free.value, available.value
    info = os.statvfs(path)
    return tuple(count * info.f_frsize for count in (info.f_blocks, info.f_bfree, info.f_bavail))


def verify_df(workspace, capacity):
    assert workspace.read('df-result') == b'UPSTREAM_DF_OK\n'
    values = json.loads(workspace.read('df-work/measurements.json'))
    total, free, available = capacity
    assert values[2] * values[1] <= total < (values[2] + 1) * values[1], (values, capacity)
    # Snapshot writes and metadata commits can consume clusters after measurement.
    assert abs(values[3] * values[1] - free) < 4 * 1024 * 1024, (values, capacity)
    assert abs(values[4] * values[1] - available) < 4 * 1024 * 1024, (values, capacity)
    assert 0 <= values[6] <= values[5] == 8193, values
    assert 'df-work/data' not in workspace.entries
    assert workspace.entries['df-work/link']['target'] == 'data'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('runner', 'ash', 'python'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='efi-df-') as temporary:
        root = Path(temporary)
        for name in ('ash', 'python'):
            shutil.copyfile(getattr(args, name), root / (name + '.wasm'))
        shutil.copytree(args.python.parent / 'lib', root / 'lib')
        result = subprocess.run(
            [str(args.runner.resolve()), str(root), 'python', '-c', DF_CHECK],
            capture_output=True, timeout=240)
        print(result.stdout.decode('utf-8', 'replace'), end='', flush=True)
        assert result.returncode == 0 and not result.stderr, result.stderr.decode('utf-8', 'replace')
        verify_df(DirectoryWorkspace(root), host_capacity(root))
        print('PASS independent saved bytes and host filesystem capacity')


if __name__ == '__main__':
    main()
