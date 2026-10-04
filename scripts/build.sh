#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -p efi-agent-uefi --target x86_64-unknown-uefi --release
mkdir -p artifacts/esp/EFI/BOOT artifacts/esp/EFI/AGENT
cp target/x86_64-unknown-uefi/release/efi-agent-uefi.efi artifacts/esp/EFI/BOOT/BOOTX64.EFI
printf 'serial\n' > artifacts/esp/EFI/AGENT/VM.TXT
cargo build -p efi-agent
image_temporary="artifacts/efi-agent-vm.img.$$.tmp"
trap 'rm -f -- "$image_temporary"' EXIT
target/debug/efi-agent pack artifacts/esp "$image_temporary"
mv -f -- "$image_temporary" artifacts/efi-agent-vm.img
printf 'ESP directory: %s/artifacts/esp\n' "$PWD"
printf 'Boot image: %s/artifacts/efi-agent-vm.img\n' "$PWD"
