# Compile the upstream ash translation unit only. Utility implementations,
# full builtin and filesystem support remain in development.
param(
    [string]$WasiSdk = $env:WASI_SDK_PATH,
    [string]$Source = 'artifacts/guest-sources/busybox-1.37.0',
    [string]$Output = 'artifacts/wasm-guests/ash-build'
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$PSNativeCommandArgumentPassing = 'Standard'
if (-not $WasiSdk) {
    throw 'Set WASI_SDK_PATH or pass -WasiSdk.'
}
uv.exe run scripts/prepare_busybox.py --source $Source --output $Output
$WasiSdk = (Resolve-Path -LiteralPath $WasiSdk).Path
$clang = Join-Path $WasiSdk 'bin/clang.exe'
$sysroot = Join-Path $WasiSdk 'share/wasi-sysroot'
$compile = '--target=wasm32-wasip1', "--sysroot=$sysroot", '-O2',
    '-fno-optimize-sibling-calls', '-D_GNU_SOURCE', '-Iguests/busybox/include',
    '-Iguests/sjlj/include', '-Iguests/include', '-Iguests/busybox', "-I$Output", "-I$Source/include", '-iquote', "$Source/shell",
    '-Werror=return-type', '-Werror=implicit-function-declaration', '-ferror-limit=20', '-c'
& $clang @compile (Join-Path $Output 'ash.c') -o (Join-Path $Output 'ash.o')
