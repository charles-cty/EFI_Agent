param(
    [string]$Source = 'artifacts/guest-sources/wolfssh-1.6.0-stable'
)
Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
uv.exe run (Join-Path $PSScriptRoot 'fetch_guest_source.py') wolfssh --source $Source
