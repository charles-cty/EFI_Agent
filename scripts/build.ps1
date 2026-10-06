param(
    [ValidateSet('release', 'debug')][string]$Profile = 'release'
)
Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'
$PSNativeCommandArgumentPassing = 'Standard'
$PSNativeCommandUseErrorActionPreference = $true
Set-Location -LiteralPath (Split-Path -Parent $PSScriptRoot)
$Profile = $Profile.ToLowerInvariant()
if (-not $env:EFI_AGENT_SHELL) {
    throw 'Set EFI_AGENT_SHELL to a locally built x64 Shell.efi from vendor/uefi-shell.'
}
$profileArgs = @()
if ($Profile -eq 'release') { $profileArgs += '--release' }
$targetDirectory = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { 'target' }
$targetDirectory = [IO.Path]::GetFullPath($targetDirectory, $PWD.Path)
cargo.exe build -p efi-agent-uefi --target x86_64-unknown-uefi --target-dir $targetDirectory @profileArgs
cargo.exe build -p efi-agent --target-dir $targetDirectory @profileArgs
$launcher = Join-Path $targetDirectory "$Profile/efi-agent.exe"
$efi = Join-Path $targetDirectory "x86_64-unknown-uefi/$Profile/efi-agent-uefi.efi"
& $launcher package $efi artifacts
Write-Output "Profile: $Profile"
Write-Output "Launcher: $launcher"
Write-Output 'VM tree: artifacts/esp'
Write-Output 'Native tree: artifacts/native-esp'
Write-Output 'Images: artifacts/efi-agent-vm.img, artifacts/efi-agent-native.img'
