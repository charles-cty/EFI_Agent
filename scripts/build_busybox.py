# /// script
# requires-python = ">=3.11"
# dependencies = ["tzdata==2026.5"]
# ///
"""Build the vendored BusyBox WASI port with native WASI SDK and Binaryen tools."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess

from prepare_timezone import prepare


REPOSITORY = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--wasi-sdk', type=Path, default=os.environ.get('WASI_SDK_PATH'))
    parser.add_argument('--binaryen', type=Path, default=os.environ.get('BINARYEN_PATH'))
    parser.add_argument('--output', type=Path, default=REPOSITORY / 'artifacts/wasm-guests')
    parser.add_argument('--build-fixtures', action='store_true')
    args = parser.parse_args()
    if args.wasi_sdk is None:
        parser.error('Set WASI_SDK_PATH or pass --wasi-sdk.')
    if args.binaryen is None:
        parser.error('Set BINARYEN_PATH or pass --binaryen.')

    sdk = args.wasi_sdk.resolve()
    binaryen = args.binaryen.resolve()
    output = args.output.resolve()
    build = output / 'ash-build'
    build.mkdir(parents=True, exist_ok=True)
    suffix = '.exe' if os.name == 'nt' else ''
    clang = sdk / 'bin' / f'clang{suffix}'
    optimizer = binaryen / 'bin' / f'wasm-opt{suffix}'
    compile_args = [
        '--target=wasm32-wasip1', f'--sysroot={sdk / "share/wasi-sysroot"}', '-O2',
        '-fno-optimize-sibling-calls', '-fwrapv', '-D_GNU_SOURCE',
        '-Iguests/busybox/include', '-Iguests/sjlj/include', '-Iguests/include',
        '-Iguests/busybox', '-Ivendor/busybox/include',
        '-iquote', 'vendor/busybox/shell', '-iquote', 'vendor/busybox/coreutils',
        '-Werror=return-type', '-Werror=implicit-function-declaration',
        '-Wl,-z,stack-size=1048576', '-Wl,--max-memory=67108864',
    ]
    for symbol in ('stat', 'lstat', 'fstat', 'fstatat', 'chmod', 'fchmod', 'open',
                   'mkdir', 'chdir', 'getcwd', 'symlink', 'unlink', 'link',
                   'statvfs', 'fstatvfs'):
        compile_args.append(f'-Wl,--wrap={symbol}')
    compile_args.append('-Wl,--undefined=chdir')
    sources = (REPOSITORY / 'guests/busybox/sources.txt').read_text(encoding='utf-8').splitlines()
    prepare(output)
    raw = build / 'ash.raw.wasm'
    try:
        subprocess.run([str(clang), *compile_args, *sources, '-o', str(raw)],
                       cwd=REPOSITORY, check=True)
        subprocess.run([str(optimizer), str(raw), '--asyncify',
                        '--pass-arg=asyncify-ignore-imports', '-o', str(output / 'ash.wasm')],
                       cwd=REPOSITORY, check=True)
        shutil.copyfile(REPOSITORY / 'vendor/busybox/LICENSE', output / 'licenses/ash.LICENSE')
        if args.build_fixtures:
            subprocess.run([
                str(clang), *compile_args, 'scripts/fixtures/wasm/busybox_metadata.c',
                'guests/busybox/metadata.c', 'guests/lib/cwd.c',
                '-o', str(build / 'metadata.wasm'),
            ], cwd=REPOSITORY, check=True)
    finally:
        raw.unlink(missing_ok=True)
    print(f'Built vendored BusyBox: {output / "ash.wasm"}')


if __name__ == '__main__':
    main()
