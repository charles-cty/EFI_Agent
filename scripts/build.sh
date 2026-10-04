#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -p efi-agent-uefi --target x86_64-unknown-uefi --release
mkdir -p artifacts/esp/EFI/BOOT artifacts/esp/EFI/AGENT
cp target/x86_64-unknown-uefi/release/efi-agent-uefi.efi artifacts/esp/EFI/BOOT/BOOTX64.EFI
printf 'serial\n' > artifacts/esp/EFI/AGENT/VM.TXT
cargo build -p efi-agent
printf 'ESP directory: %s/artifacts/esp\n' "$PWD"
