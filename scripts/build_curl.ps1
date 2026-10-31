param(
    [Parameter(Mandatory)][string]$WasiSdk,
    [string]$WolfSsl = 'artifacts/wasm-guests/wolfssl',
    [string]$Source = 'artifacts/guest-sources/curl-8.22.0',
    [string]$Output = 'artifacts/wasm-guests',
    [string]$MsysBash = 'C:\msys64\usr\bin\bash.exe',
    [int]$Jobs = 4,
    [switch]$Reconfigure
)
Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$buildArgs = @('--component', 'curl', '--wasi-sdk', $WasiSdk, '--wolfssl', $WolfSsl,
    '--source', $Source, '--output', $Output, '--msys-bash', $MsysBash, '--jobs', $Jobs)
if ($Reconfigure) { $buildArgs += '--reconfigure' }
uv.exe run (Join-Path $PSScriptRoot 'build_guests.py') @buildArgs
