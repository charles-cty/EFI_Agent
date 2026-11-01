param(
    [Parameter(Mandatory)][string]$WasiSdk,
    [string]$Source = 'artifacts/guest-sources/jq-1.8.1',
    [string]$Output = 'artifacts/wasm-guests',
    [string]$MsysBash = 'C:\msys64\usr\bin\bash.exe',
    [int]$Jobs = 4,
    [switch]$Reconfigure
)
Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'
$PSNativeCommandArgumentPassing = 'Standard'
$PSNativeCommandUseErrorActionPreference = $true
$arguments = @('--component', 'jq', '--wasi-sdk', $WasiSdk, '--source', $Source,
               '--output', $Output, '--msys-bash', $MsysBash, '--jobs', $Jobs)
if ($Reconfigure) { $arguments += '--reconfigure' }
uv.exe run (Join-Path $PSScriptRoot 'build_guests.py') @arguments
