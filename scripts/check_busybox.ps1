# Compile the vendored ash translation unit without linking the full image.
param(
    [string]$WasiSdk = $env:WASI_SDK_PATH,
    [string]$Output = 'artifacts/wasm-guests/ash-build'
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$PSNativeCommandArgumentPassing = 'Standard'
if (-not $WasiSdk) {
    throw 'Set WASI_SDK_PATH or pass -WasiSdk.'
}
Set-Location -LiteralPath (Split-Path -Parent $PSScriptRoot)
New-Item -ItemType Directory -Force -Path $Output | Out-Null
$WasiSdk = (Resolve-Path -LiteralPath $WasiSdk).Path
$clang = Join-Path $WasiSdk 'bin/clang.exe'
$sysroot = Join-Path $WasiSdk 'share/wasi-sysroot'
$compile = '--target=wasm32-wasip1', "--sysroot=$sysroot", '-O2',
    '-fno-optimize-sibling-calls', '-D_GNU_SOURCE', '-Iguests/busybox/include',
    '-Iguests/sjlj/include', '-Iguests/include', '-Iguests/busybox', '-Ivendor/busybox/include', '-iquote', 'vendor/busybox/shell',
    '-Werror=return-type', '-Werror=implicit-function-declaration', '-ferror-limit=20', '-c'
& $clang @compile vendor/busybox/shell/ash.c -o (Join-Path $Output 'ash.o')
