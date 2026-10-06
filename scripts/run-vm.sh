#!/usr/bin/env bash
# Launch the existing Release VM without building it.
set -euo pipefail
cd "$(dirname "$0")/.."

# QEMU user networking uses the 10.0.2.0/24 guest network.
export EFI_AGENT_IPV4_ADDRESS=10.0.2.15
export EFI_AGENT_IPV4_NETMASK=255.255.255.0
export EFI_AGENT_IPV4_GATEWAY=10.0.2.2
export EFI_AGENT_DNS_ADDRESS=10.0.2.3
export EFI_AGENT_DNS_PORT=53

mkdir -p workspace
exec target/release/efi-agent vm \
    qemu-system-x86_64 \
    artifacts/firmware/OVMF_CODE_4M.fd \
    artifacts/firmware/OVMF_VARS_4M.fd \
    artifacts/esp \
    workspace
