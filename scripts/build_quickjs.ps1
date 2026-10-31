param(
    [string]$WasiSdk = $env:WASI_SDK_PATH,
    [string]$Source = 'vendor/quickjs',
    [string]$Output = 'artifacts/wasm-guests'
)
Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
uv.exe run (Join-Path $PSScriptRoot 'build_guests.py') --component quickjs --wasi-sdk $WasiSdk --source $Source --output $Output
