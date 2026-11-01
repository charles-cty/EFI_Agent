# /// script
# requires-python = ">=3.11"
# dependencies = ["tzdata==2026.5"]
# ///
"""Build all shipped guests with native Windows or Linux tools."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import zipfile

from fetch_guest_source import REPOSITORY, fetch_file, fetch_source


SUFFIX = '.exe' if os.name == 'nt' else ''


def run(*arguments, **options):
    subprocess.run([str(value) for value in arguments], cwd=REPOSITORY, check=True, **options)


def copy(source, destination):
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)


def build_quickjs(sdk, source, output):
    build = output / 'quickjs-build'
    build.mkdir(parents=True, exist_ok=True)
    run(f'node{SUFFIX}', 'scripts/prepare_quickjs.mjs', build)
    version = (source / 'VERSION').read_text(encoding='utf-8').strip()
    flags = [
        '--target=wasm32-wasip1', f'--sysroot={sdk / "share/wasi-sysroot"}',
        '-O2', '-D_GNU_SOURCE', f'-DCONFIG_VERSION="{version}"',
        f'-I{source}', f'-I{build}', '-Iguests/include',
        '-Wl,-z,stack-size=1048576', '-Wl,--max-memory=67108864',
        '-Wl,--wrap=chdir', '-Wl,--wrap=getcwd', '-Wl,--undefined=chdir',
        '-Wl,--wrap=stat', '-Wl,--wrap=lstat', '-Wl,--wrap=fstatat', '-lm',
    ]
    sources = [source / name for name in
               ('quickjs.c', 'dtoa.c', 'libregexp.c', 'libunicode.c', 'cutils.c')]
    sources += ['guests/quickjs/main.c', 'guests/lib/process.c',
                'guests/lib/shell.c', 'guests/lib/cwd.c']
    run(sdk / f'bin/clang{SUFFIX}', *flags, *sources, '-o', output / 'quickjs.wasm')
    copy(source / 'LICENSE', output / 'licenses/quickjs.LICENSE')
    copy(REPOSITORY / 'vendor/zx/LICENSE', output / 'licenses/zx.LICENSE')
    copy(REPOSITORY / 'vendor/zx/build/3rd-party-licenses', output / 'licenses/zx-third-party.LICENSE')


def build_wolfssl(sdk, source, output, jobs):
    build = output / 'build'
    settings = (REPOSITORY / 'guests/wolfssl').as_posix()
    run(f'cmake{SUFFIX}', '--fresh', '-S', source, '-B', build, '-G', 'Ninja',
        f'-DCMAKE_TOOLCHAIN_FILE={sdk.as_posix()}/share/cmake/wasi-sdk-p1.cmake',
        f'-DWASI_SDK_PREFIX={sdk.as_posix()}', '-DCMAKE_BUILD_TYPE=Release',
        '-DBUILD_SHARED_LIBS=OFF', f'-DCMAKE_INSTALL_PREFIX={output.as_posix()}',
        f'-DCMAKE_C_FLAGS=-I"{settings}"', '-DWOLFSSL_USER_SETTINGS=yes',
        '-DWOLFSSL_SINGLE_THREADED=yes', '-DWOLFSSL_ASM=no', '-DWOLFSSL_EXAMPLES=no',
        '-DWOLFSSL_CRYPT_TESTS=no', '-DWOLFSSL_CRYPT_BENCHMARKS=no')
    run(f'cmake{SUFFIX}', '--build', build, '--target', 'wolfssl', '--parallel', jobs)
    run(f'cmake{SUFFIX}', '--install', build)
    platform = build / 'efi-platform.o'
    run(sdk / f'bin/clang{SUFFIX}', '--target=wasm32-wasip1', '-O2', '-Wall', '-Wextra',
        '-Werror', '-DWOLFSSL_USER_SETTINGS', f'-I{settings}', f'-I{output / "include"}',
        '-c', 'guests/wolfssl/platform.c', '-o', platform)
    run(sdk / f'bin/llvm-ar{SUFFIX}', 'rcs', output / 'lib/libwolfssl.a', platform)
    copy(REPOSITORY / 'guests/wolfssl/user_settings.h', output / 'include/user_settings.h')
    copy(source / 'COPYING', output / 'COPYING')


def build_curl(sdk, source, wolfssl, output, jobs, msys_bash, reconfigure):
    build = output / 'curl-build'
    build.mkdir(parents=True, exist_ok=True)
    compiler = sdk / f'bin/clang{SUFFIX}'
    shim, cwd = build / 'posix-network.o', build / 'cwd.o'
    flags = ['--target=wasm32-wasip1', '-O2', '-Wall', '-Wextra', '-Werror']
    run(compiler, *flags, '-Iguests/posix/include', '-Iguests/include',
        '-c', 'guests/posix/network.c', '-o', shim)
    run(compiler, *flags, '-c', 'guests/lib/cwd.c', '-o', cwd)
    run(sdk / f'bin/llvm-ar{SUFFIX}', 'rcs', build / 'libefi_posix.a', shim, cwd)
    script = (REPOSITORY / 'scripts/build_curl.sh').read_text(encoding='utf-8')
    configuration = json.dumps({'source': str(source), 'sdk': str(sdk),
                                'wolfssl': str(wolfssl), 'project': str(REPOSITORY),
                                'script': script}, indent=2)
    record = build / 'configuration.json'
    changed = not record.is_file() or record.read_text(encoding='utf-8') != configuration
    if (reconfigure or changed) and (build / 'Makefile').exists():
        (build / 'Makefile').unlink()
    shell = msys_bash if os.name == 'nt' else 'bash'
    print(f'curl logs: {build / "configure.log"}, {build / "build.log"}', flush=True)
    run(shell, '-c', script, 'efi-curl-build', source, sdk, REPOSITORY,
        build, jobs, wolfssl, 'windows' if os.name == 'nt' else 'linux')
    record.write_text(configuration, encoding='utf-8')
    copy(build / 'src/curl', output / 'curl.wasm')
    copy(source / 'COPYING', output / 'licenses/curl.LICENSE')
    copy(wolfssl / 'COPYING', output / 'licenses/wolfssl.COPYING')


def build_wolfssh(sdk, source, wolfssl, output):
    build = output / 'wolfssh-build'
    build.mkdir(parents=True, exist_ok=True)
    compiler = sdk / f'bin/clang{SUFFIX}'
    flags = ['--target=wasm32-wasip1', '-O2', '-ffunction-sections', '-fdata-sections',
             '-DWOLFSSL_USER_SETTINGS', '-DWOLFSSH_USER_IO', '-DNO_WOLFSSH_SERVER',
             '-DWOLFSSH_SHELL', f'-I{source}', f'-I{wolfssl / "include"}',
             '-Iguests/posix/include', '-Iguests/include']
    objects = []
    for name in ('ssh', 'internal', 'log', 'io', 'port', 'ossh'):
        target = build / f'{name}.o'
        run(compiler, *flags, '-c', source / f'src/{name}.c', '-o', target)
        objects.append(target)
    library = build / 'libwolfssh.a'
    run(sdk / f'bin/llvm-ar{SUFFIX}', 'rcs', library, *objects)
    run(compiler, *flags, '-Wall', '-Wextra', '-Werror', 'guests/wolfssh/main.c',
        'guests/posix/network.c', 'guests/lib/cwd.c', library, wolfssl / 'lib/libwolfssl.a',
        '-Wl,--fatal-warnings', '-Wl,-z,stack-size=1048576', '-Wl,--max-memory=67108864',
        '-Wl,--wrap=chdir', '-Wl,--wrap=getcwd', '-Wl,--undefined=chdir',
        '-Wl,--wrap=stat', '-Wl,--wrap=lstat', '-Wl,--wrap=fstatat',
        '-o', output / 'wolfssh.wasm')
    copy(source / 'LICENSING', output / 'licenses/wolfssh.LICENSING')
    copy(wolfssl / 'COPYING', output / 'licenses/wolfssl.COPYING')


def build_jq(sdk, source, output, jobs, msys_bash, reconfigure):
    build = output / 'jq-build'
    build.mkdir(parents=True, exist_ok=True)
    cwd = build / 'cwd.o'
    run(sdk / f'bin/clang{SUFFIX}', '--target=wasm32-wasip1', '-O2',
        '-Wall', '-Wextra', '-Werror', '-c', 'guests/lib/cwd.c', '-o', cwd)
    run(sdk / f'bin/llvm-ar{SUFFIX}', 'rcs', build / 'libefi_cwd.a', cwd)
    script = (REPOSITORY / 'scripts/build_jq.sh').read_text(encoding='utf-8')
    configuration = json.dumps({'source': str(source), 'sdk': str(sdk),
                                'project': str(REPOSITORY), 'script': script}, indent=2)
    record = build / 'configuration.json'
    changed = not record.is_file() or record.read_text(encoding='utf-8') != configuration
    if (reconfigure or changed) and (build / 'Makefile').exists():
        (build / 'Makefile').unlink()
    print(f'jq logs: {build / "configure.log"}, {build / "build.log"}', flush=True)
    shell = msys_bash if os.name == 'nt' else 'bash'
    run(shell, '-c', script, 'efi-jq-build', source, sdk, build, jobs,
        'windows' if os.name == 'nt' else 'linux')
    record.write_text(configuration, encoding='utf-8')
    copy(build / 'jq', output / 'jq.wasm')
    copy(source / 'COPYING', output / 'licenses/jq.COPYING')
    copy(source / 'vendor/oniguruma/COPYING', output / 'licenses/oniguruma.COPYING')


def prepare_python(output, archive=None):
    cache = REPOSITORY / 'artifacts/python-prebuilt'
    if archive is None:
        archive = fetch_file(
            'https://github.com/brettcannon/cpython-wasi-build/releases/download/'
            'v3.14.7/python-3.14.7-wasi_sdk-24.zip',
            cache / 'python-3.14.7-wasi_sdk-24.zip')
    distribution = cache / 'extracted'
    if distribution.exists():
        shutil.rmtree(distribution)
    with zipfile.ZipFile(archive) as package:
        package.extractall(distribution)
    library = distribution / 'lib/python3.14/site-packages'
    for name in ('sitecustomize.py', '_efi_process.py', '_efi_filesystem.py'):
        copy(REPOSITORY / 'guests/python' / name, library / name)
    copy(distribution / 'python.wasm', output / 'python.wasm')
    if (output / 'lib/python3.14').exists():
        shutil.rmtree(output / 'lib/python3.14')
    shutil.copytree(distribution / 'lib', output / 'lib', dirs_exist_ok=True)
    copy(distribution / 'LICENSE', output / 'licenses/python.LICENSE')


def prepare_shell():
    source = REPOSITORY / 'vendor/uefi-shell'
    if not (source / 'License.txt').is_file():
        run(f'git{SUFFIX}', 'submodule', 'update', '--init', '--', 'vendor/uefi-shell',
            stdout=sys.stderr)
    selected = os.environ.get('EFI_AGENT_SHELL')
    if selected:
        binary = Path(selected).resolve()
        if not binary.is_file():
            raise FileNotFoundError(f'UEFI Shell image not found: {binary}')
        return binary
    return fetch_file('https://github.com/pbatard/UEFI-Shell/releases/download/26H1/shellx64.efi',
                      REPOSITORY / 'artifacts/uefi-shell-26H1/shellx64.efi')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--component', default='all',
                        choices=('all', 'quickjs', 'wolfssl', 'curl', 'wolfssh', 'jq', 'python', 'shell'))
    parser.add_argument('--wasi-sdk', type=Path, default=os.environ.get('WASI_SDK_PATH'))
    parser.add_argument('--binaryen', type=Path, default=os.environ.get('BINARYEN_PATH'))
    parser.add_argument('--output', type=Path)
    parser.add_argument('--source', type=Path)
    parser.add_argument('--wolfssl', type=Path)
    parser.add_argument('--archive', type=Path)
    parser.add_argument('--msys-bash', default=r'C:\msys64\usr\bin\bash.exe')
    parser.add_argument('--jobs', type=int, default=4)
    parser.add_argument('--reconfigure', action='store_true')
    args = parser.parse_args()
    if args.component == 'shell':
        print(prepare_shell())
        return
    if args.component != 'python' and args.wasi_sdk is None:
        parser.error('Set WASI_SDK_PATH or pass --wasi-sdk.')
    if args.component == 'all' and args.binaryen is None:
        parser.error('Set BINARYEN_PATH or pass --binaryen.')
    if args.jobs < 1:
        parser.error('--jobs must be positive.')
    output = args.output or REPOSITORY / 'artifacts/wasm-guests'
    if args.output is None and args.component == 'wolfssl':
        output /= 'wolfssl'
    output = output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    sdk = args.wasi_sdk.resolve() if args.wasi_sdk else None
    wolfssl = (args.wolfssl or REPOSITORY / 'artifacts/wasm-guests/wolfssl').resolve()
    components = ('python', 'busybox', 'quickjs', 'wolfssl', 'curl', 'wolfssh', 'jq')
    if args.component != 'all':
        components = (args.component,)
    else:
        wolfssl = output / 'wolfssl'
    for component in components:
        print(f'Build guest: {component}', flush=True)
        if component == 'python':
            prepare_python(output, args.archive)
        elif component == 'busybox':
            run(sys.executable, 'scripts/build_busybox.py', '--wasi-sdk', sdk,
                '--binaryen', args.binaryen.resolve(), '--output', output)
        elif component == 'quickjs':
            source = (args.source or REPOSITORY / 'vendor/quickjs').resolve()
            build_quickjs(sdk, source, output)
        else:
            source = fetch_source(component, args.source)
            if component == 'wolfssl':
                destination = wolfssl if args.component == 'all' else output
                destination.mkdir(parents=True, exist_ok=True)
                build_wolfssl(sdk, source, destination, args.jobs)
            elif component == 'curl':
                build_curl(sdk, source, wolfssl, output, args.jobs,
                           args.msys_bash, args.reconfigure)
            elif component == 'wolfssh':
                build_wolfssh(sdk, source, wolfssl, output)
            elif component == 'jq':
                build_jq(sdk, source, output, args.jobs, args.msys_bash, args.reconfigure)


if __name__ == '__main__':
    main()
