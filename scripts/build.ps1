param([switch]$Release)
Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
Set-Location -LiteralPath (Split-Path -Parent $PSScriptRoot)
$buildArgs = @('build', '-p', 'efi-agent-uefi', '--target', 'x86_64-unknown-uefi')
$profile = 'debug'
if ($Release) {
    $buildArgs += '--release'
    $profile = 'release'
}
cargo.exe @buildArgs
$boot = Join-Path $PWD 'artifacts\esp\EFI\BOOT'
$config = Join-Path $PWD 'artifacts\esp\EFI\AGENT'
New-Item -ItemType Directory -Force -Path $boot, $config | Out-Null
Copy-Item -LiteralPath "target\x86_64-unknown-uefi\$profile\efi-agent-uefi.efi" -Destination (Join-Path $boot 'BOOTX64.EFI')
Set-Content -LiteralPath (Join-Path $config 'VM.TXT') -Value 'serial' -Encoding utf8NoBOM
cargo.exe build -p efi-agent
Write-Output "ESP directory: $PWD\artifacts\esp"
