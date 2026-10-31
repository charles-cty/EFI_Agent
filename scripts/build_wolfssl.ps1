param(
    [Parameter(Mandatory)][string]$WasiSdk,
    [string]$Source = 'artifacts/guest-sources/wolfssl-5.9.4-stable',
    [string]$Output = 'artifacts/wasm-guests/wolfssl',
    [int]$Jobs = 4
)
Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
uv.exe run (Join-Path $PSScriptRoot 'build_guests.py') --component wolfssl --wasi-sdk $WasiSdk --source $Source --output $Output --jobs $Jobs
