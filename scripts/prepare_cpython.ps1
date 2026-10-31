param(
    [string]$Archive,
    [string]$Output = 'artifacts/wasm-guests'
)
Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$prepareArgs = @('--component', 'python', '--output', $Output)
if ($Archive) { $prepareArgs += '--archive', $Archive }
uv.exe run (Join-Path $PSScriptRoot 'build_guests.py') @prepareArgs
