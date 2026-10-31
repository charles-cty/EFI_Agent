param(
    [string]$Source = 'artifacts/guest-sources/wolfssl-5.9.4-stable'
)
Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
uv.exe run (Join-Path $PSScriptRoot 'fetch_guest_source.py') wolfssl --source $Source
