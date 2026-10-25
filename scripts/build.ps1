param(
    [ValidateSet('release', 'debug')][string]$Profile = 'release',
    [string]$Guests = 'artifacts/wasm-guests',
    [switch]$VmOnly
)
Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'
$PSNativeCommandArgumentPassing = 'Standard'
$PSNativeCommandUseErrorActionPreference = $true
Set-Location -LiteralPath (Split-Path -Parent $PSScriptRoot)
$Profile = $Profile.ToLowerInvariant()
if (-not $VmOnly) {
    $shell = if ($env:EFI_AGENT_SHELL) {
        [IO.Path]::GetFullPath($env:EFI_AGENT_SHELL, $PWD.Path)
    } else {
        $shellProfile = $Profile.ToUpperInvariant()
        $candidates = @(Get-ChildItem -LiteralPath (Join-Path $PWD 'vendor/uefi-shell/edk2/Build/Shell') -Filter '*.efi' -File -Recurse -ErrorAction SilentlyContinue |
            Where-Object { $_.FullName -match "[\\/]${shellProfile}_[^\\/]+[\\/]X64[\\/]Shell_[0-9A-Fa-f-]+\.efi$" } |
            Sort-Object LastWriteTime -Descending)
        if (-not $candidates) {
            throw 'Build the x64 Shell from the vendor/uefi-shell submodule first, or set EFI_AGENT_SHELL to its .efi output.'
        }
        $candidates[0].FullName
    }
    if (-not (Test-Path -LiteralPath $shell -PathType Leaf)) {
        throw "UEFI Shell image not found: $shell"
    }
    $env:EFI_AGENT_SHELL = $shell
}
$profileArgs = @()
if ($Profile -eq 'release') { $profileArgs += '--release' }
$targetDirectory = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { 'target' }
$targetDirectory = [IO.Path]::GetFullPath($targetDirectory, $PWD.Path)
cargo.exe build -p efi-agent-uefi --target x86_64-unknown-uefi --target-dir $targetDirectory @profileArgs
cargo.exe build -p efi-agent --target-dir $targetDirectory @profileArgs
$launcher = Join-Path $targetDirectory "$Profile/efi-agent.exe"
$efi = Join-Path $targetDirectory "x86_64-unknown-uefi/$Profile/efi-agent-uefi.efi"
if (-not (Test-Path -LiteralPath (Join-Path $Guests 'programs.json'))) {
    & $launcher catalog $Guests
}
$packageArgs = @($efi, 'artifacts', $Guests)
if ($VmOnly) { $packageArgs += '--vm-only' }
& $launcher package @packageArgs
Write-Output "Profile: $Profile"
Write-Output "Launcher: $launcher"
Write-Output 'VM tree: artifacts/esp'
Write-Output 'VM image: artifacts/efi-agent-vm.img'
if (-not $VmOnly) {
    Write-Output 'Native tree: artifacts/native-esp'
    Write-Output 'Native image: artifacts/efi-agent-native.img'
}
