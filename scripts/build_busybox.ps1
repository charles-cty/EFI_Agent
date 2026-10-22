param(
    [string]$WasiSdk = $env:WASI_SDK_PATH,
    [Parameter(Mandatory)][string]$Binaryen,
    [string]$Source = 'artifacts/guest-sources/busybox-1.37.0',
    [string]$Output = 'artifacts/wasm-guests',
    [switch]$BuildFixtures
)
Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$PSNativeCommandArgumentPassing = 'Standard'
if (-not $WasiSdk) { throw 'Set WASI_SDK_PATH or pass -WasiSdk.' }
$build = Join-Path $Output 'ash-build'
uv.exe run scripts/prepare_busybox.py --source $Source --output $build
$WasiSdk = (Resolve-Path -LiteralPath $WasiSdk).Path
$clang = Join-Path $WasiSdk 'bin/clang.exe'
$sysroot = Join-Path $WasiSdk 'share/wasi-sysroot'
$optimizer = Join-Path $Binaryen 'bin/wasm-opt.exe'
$compile = '--target=wasm32-wasip1', "--sysroot=$sysroot", '-O2',
    '-fno-optimize-sibling-calls', '-fwrapv', '-D_GNU_SOURCE', '-Iguests/busybox/include',
    '-Iguests/sjlj/include', '-Iguests/include', '-Iguests/busybox', "-I$build", "-I$Source/include", '-iquote', "$Source/shell",
    '-Werror=return-type', '-Werror=implicit-function-declaration', '-Wl,-z,stack-size=1048576', '-Wl,--max-memory=67108864'
foreach ($symbol in @('stat', 'lstat', 'fstat', 'fstatat', 'chmod', 'fchmod', 'open', 'mkdir', 'chdir')) {
    $compile += "-Wl,--wrap=$symbol"
}
$sources = (Join-Path $build 'ash.c'), (Join-Path $build 'shell_read.c'), (Join-Path $Source 'shell/ash_ptr_hack.c'),
    (Join-Path $Source 'coreutils/echo.c'), (Join-Path $Source 'libbb/process_escape_sequence.c'),
    (Join-Path $build 'printf.c'), (Join-Path $Source 'libbb/bb_strtonum.c'),
    (Join-Path $build 'test.c'), (Join-Path $Source 'coreutils/test_ptr_hack.c'),
    (Join-Path $build 'math.c'), (Join-Path $build 'awk.c'), (Join-Path $Source 'libbb/xregcomp.c'),
    (Join-Path $build 'sed.c'), (Join-Path $Source 'libbb/getopt32.c'),
    (Join-Path $Source 'libbb/llist.c'), (Join-Path $Source 'libbb/get_line_from_file.c'),
    'guests/busybox/utility_support.c',
    (Join-Path $build 'grep.c'), (Join-Path $Source 'libbb/recursive_action.c'),
    (Join-Path $Source 'libbb/concat_path_file.c'), (Join-Path $Source 'libbb/concat_subpath_file.c'),
    (Join-Path $Source 'libbb/last_char_is.c'),
    (Join-Path $Source 'coreutils/chmod.c'), (Join-Path $Source 'libbb/parse_mode.c'),
    (Join-Path $Source 'libbb/mode_string.c'), 'guests/busybox/metadata.c',
    (Join-Path $build 'cat.c'), (Join-Path $build 'head.c'),
    (Join-Path $build 'tail.c'), (Join-Path $Source 'coreutils/wc.c'), (Join-Path $build 'tee.c'),
    (Join-Path $Source 'libbb/bb_cat.c'), (Join-Path $Source 'libbb/copyfd.c'),
    (Join-Path $build 'print_numbered_lines.c'), (Join-Path $Source 'libbb/printable.c'),
    (Join-Path $Source 'libbb/xatonum.c'), (Join-Path $Source 'libbb/wfopen_input.c'),
    (Join-Path $Source 'libbb/read.c'), (Join-Path $build 'fflush_stdout_and_exit.c'),
    'guests/busybox/stream_support.c',
    (Join-Path $build 'sleep.c'), (Join-Path $build 'duration.c'),
    (Join-Path $Source 'coreutils/basename.c'), (Join-Path $Source 'coreutils/dirname.c'),
    (Join-Path $build 'pwd.c'), (Join-Path $Source 'coreutils/env.c'),
    (Join-Path $Source 'coreutils/printenv.c'), (Join-Path $Source 'coreutils/seq.c'),
    (Join-Path $build 'yes.c'), (Join-Path $Source 'coreutils/true.c'),
    (Join-Path $Source 'coreutils/false.c'),
    (Join-Path $Source 'libbb/get_last_path_component.c'),
    (Join-Path $Source 'libbb/single_argv.c'), (Join-Path $Source 'libbb/xgetcwd.c'),
    (Join-Path $build 'mkdir.c'), (Join-Path $build 'rmdir.c'),
    (Join-Path $build 'rm.c'), (Join-Path $build 'touch.c'),
    (Join-Path $build 'realpath.c'),
    (Join-Path $build 'od.c'), (Join-Path $Source 'libbb/dump.c'),
    'guests/busybox/dump_support.c',
    (Join-Path $build 'uudecode.c'), (Join-Path $Source 'libbb/uuencode.c'),
    (Join-Path $build 'md5_sha1_sum.c'), (Join-Path $Source 'libbb/hash_md5_sha.c'),
    (Join-Path $Source 'libbb/xfuncs.c'),
    (Join-Path $build 'ln.c'),
    (Join-Path $build 'hexdump.c'),
    (Join-Path $build 'xargs.c'),
    (Join-Path $build 'find.c'),
    (Join-Path $Source 'libbb/xrealloc_vector.c'),
    (Join-Path $Source 'libbb/make_directory.c'), (Join-Path $build 'remove_file.c'),
    (Join-Path $Source 'libbb/ask_confirmation.c'), (Join-Path $Source 'libbb/time.c'),
    (Join-Path $build 'date.c'), (Join-Path $Source 'libbb/compare_string_array.c'),
    (Join-Path $build 'xreadlink.c'),
    (Join-Path $build 'sort.c'), (Join-Path $build 'uniq.c'), (Join-Path $build 'cut.c'),
    (Join-Path $Source 'coreutils/tr.c'),
    'guests/busybox/text_support.c',
    'guests/busybox/main.c', 'guests/busybox/support.c', 'guests/busybox/awk_support.c', 'guests/busybox/applets.c', 'guests/busybox/exec.c',
    'guests/lib/process.c', 'guests/sjlj/runtime.c', 'guests/sjlj/stack.s'
$raw = Join-Path $build 'ash.raw.wasm'
try {
    & $clang @compile @sources -o $raw
    & $optimizer $raw --asyncify --pass-arg=asyncify-ignore-imports -o (Join-Path $Output 'ash.wasm')
    if ($BuildFixtures) {
        & $clang @compile scripts/fixtures/wasm/busybox_metadata.c guests/busybox/metadata.c -o (Join-Path $build 'metadata.wasm')
    }
} finally {
    if (Test-Path -LiteralPath $raw) { Remove-Item -LiteralPath $raw }
}
