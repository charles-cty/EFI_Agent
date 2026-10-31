param(
    [Parameter(Mandatory)][string]$WasiSdk,
    [string]$WolfSsl = 'artifacts/wasm-guests/wolfssl',
    [string]$Source = 'artifacts/guest-sources/wolfssh-1.6.0-stable',
    [string]$Output = 'artifacts/wasm-guests'
)
Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
uv.exe run (Join-Path $PSScriptRoot 'build_guests.py') --component wolfssh --wasi-sdk $WasiSdk --wolfssl $WolfSsl --source $Source --output $Output
