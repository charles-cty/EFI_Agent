param(
    [string]$Source = 'artifacts/guest-sources/curl-8.22.0'
)
Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
uv.exe run (Join-Path $PSScriptRoot 'fetch_guest_source.py') curl --source $Source
