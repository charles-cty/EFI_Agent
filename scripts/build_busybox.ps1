param(
    [string]$WasiSdk = $env:WASI_SDK_PATH,
    [string]$Binaryen = $env:BINARYEN_PATH,
    [string]$Output = 'artifacts/wasm-guests',
    [switch]$BuildFixtures
)
Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$PSNativeCommandArgumentPassing = 'Standard'
Set-Location -LiteralPath (Split-Path -Parent $PSScriptRoot)
$buildArgs = @('--output', $Output)
if ($WasiSdk) { $buildArgs += '--wasi-sdk', $WasiSdk }
if ($Binaryen) { $buildArgs += '--binaryen', $Binaryen }
if ($BuildFixtures) { $buildArgs += '--build-fixtures' }
uv.exe run scripts/build_busybox.py @buildArgs
