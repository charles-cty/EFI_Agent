param(
    [string]$Launcher = 'target/release/efi-agent.exe',
    [string]$Qemu = 'qemu-system-x86_64.exe',
    [string]$Code = 'artifacts/firmware/OVMF_CODE_4M.fd',
    [string]$Vars = 'artifacts/firmware/OVMF_VARS_4M.fd',
    [string]$Esp = 'artifacts/esp',
    [string]$Workspace = 'workspace'
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'
$PSNativeCommandArgumentPassing = 'Standard'
Set-Location -LiteralPath (Split-Path -Parent $PSScriptRoot)
$Qemu = (Get-Command $Qemu -CommandType Application -ErrorAction Stop).Source

# QEMU user networking uses the 10.0.2.0/24 guest network.
$env:EFI_AGENT_IPV4_ADDRESS = '10.0.2.15'
$env:EFI_AGENT_IPV4_NETMASK = '255.255.255.0'
$env:EFI_AGENT_IPV4_GATEWAY = '10.0.2.2'
$env:EFI_AGENT_DNS_ADDRESS = '10.0.2.3'
$env:EFI_AGENT_DNS_PORT = '53'

New-Item -ItemType Directory -Force -Path $Workspace | Out-Null
& $Launcher vm $Qemu $Code $Vars $Esp $Workspace
exit $LASTEXITCODE
