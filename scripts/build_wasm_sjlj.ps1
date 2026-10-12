param(
    [string]$WasiSdk = $env:WASI_SDK_PATH,
    [Parameter(Mandatory)][string]$Binaryen,
    [ValidateSet('0', '1', '2', '3')][string]$Optimization = '2',
    [string]$Output = 'artifacts/wasm-fixtures'
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$PSNativeCommandArgumentPassing = 'Standard'
if (-not $WasiSdk) {
    throw 'Set WASI_SDK_PATH or pass -WasiSdk.'
}
$WasiSdk = (Resolve-Path -LiteralPath $WasiSdk).Path
$clang = Join-Path $WasiSdk 'bin/clang.exe'
$sysroot = Join-Path $WasiSdk 'share/wasi-sysroot'
$optimizer = Join-Path $Binaryen 'bin/wasm-opt.exe'
New-Item -ItemType Directory -Force $Output | Out-Null
$raw = Join-Path $Output 'sjlj.raw.wasm'
$compile = '--target=wasm32-wasip1', "--sysroot=$sysroot", "-O$Optimization",
    '-fno-optimize-sibling-calls', '-Wall', '-Wextra', '-Werror',
    '-Iguests/sjlj/include', '-Iguests/include', '-Wl,-z,stack-size=1048576',
    '-Wl,--max-memory=67108864'
try {
    & $clang @compile scripts/fixtures/wasm/sjlj.c guests/sjlj/runtime.c guests/sjlj/stack.s guests/lib/process.c -o $raw
    & $optimizer $raw --asyncify --pass-arg=asyncify-ignore-imports -o (Join-Path $Output 'sjlj.wasm')
} finally {
    Remove-Item -LiteralPath $raw -ErrorAction SilentlyContinue
}
