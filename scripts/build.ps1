param(
    [ValidateSet('release', 'debug')][string]$Profile = 'release',
    [string]$Guests = 'artifacts/wasm-guests',
    [string]$WasiSdk = $env:WASI_SDK_PATH,
    [string]$Binaryen = $env:BINARYEN_PATH,
    [string]$MsysBash = 'C:\msys64\usr\bin\bash.exe',
    [int]$Jobs = 4,
    [switch]$VmOnly
)
Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'
$PSNativeCommandArgumentPassing = 'Standard'
$PSNativeCommandUseErrorActionPreference = $true
Set-Location -LiteralPath (Split-Path -Parent $PSScriptRoot)
$project = $PWD.Path
if (-not $env:VSCMD_VER) {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    $installation = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if (-not $installation) { throw 'Install the Visual Studio C++ build tools.' }
    & (Join-Path $installation 'Common7/Tools/Launch-VsDevShell.ps1') -Arch amd64 -HostArch amd64
    Set-Location -LiteralPath $project
}
$Profile = $Profile.ToLowerInvariant()
if (-not $VmOnly) {
    $env:EFI_AGENT_SHELL = uv.exe run scripts/build_guests.py --component shell
}
uv.exe run scripts/build_guests.py --wasi-sdk $WasiSdk --binaryen $Binaryen --output $Guests --msys-bash $MsysBash --jobs $Jobs
$profileArgs = @()
if ($Profile -eq 'release') { $profileArgs += '--release' }
$targetDirectory = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { 'target' }
$targetDirectory = [IO.Path]::GetFullPath($targetDirectory, $PWD.Path)
cargo.exe build --locked -p efi-agent-uefi --target x86_64-unknown-uefi --target-dir $targetDirectory --jobs $Jobs @profileArgs
cargo.exe build --locked -p efi-agent --target-dir $targetDirectory --jobs $Jobs @profileArgs
$launcher = Join-Path $targetDirectory "$Profile/efi-agent.exe"
$efi = Join-Path $targetDirectory "x86_64-unknown-uefi/$Profile/efi-agent-uefi.efi"
& $launcher catalog $Guests
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
