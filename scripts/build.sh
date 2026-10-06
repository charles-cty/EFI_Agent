#!/usr/bin/env bash
# Build both programs with one profile, then assemble VM and native packages.
set -euo pipefail
cd "$(dirname "$0")/.."
profile=release
usage() {
    printf 'Usage: bash scripts/build.sh [--profile release|debug]\n'
}
while (($#)); do
    case "$1" in
        --profile)
            if (($# < 2)); then usage >&2; exit 2; fi
            profile=$2
            shift 2
            ;;
        --help) usage; exit 0 ;;
        *) usage >&2; exit 2 ;;
    esac
done
if [[ "$profile" != release && "$profile" != debug ]]; then
    printf 'Profile must be release or debug.\n' >&2
    exit 2
fi
if [[ -z "${EFI_AGENT_SHELL:-}" ]]; then
    printf 'Set EFI_AGENT_SHELL to a locally built x64 Shell.efi from vendor/uefi-shell.\n' >&2
    exit 2
fi
build_args=()
if [[ "$profile" == release ]]; then build_args+=(--release); fi
target_dir=${CARGO_TARGET_DIR:-target}
cargo build -p efi-agent-uefi --target x86_64-unknown-uefi --target-dir "$target_dir" "${build_args[@]}"
cargo build -p efi-agent --target-dir "$target_dir" "${build_args[@]}"
launcher="$target_dir/$profile/efi-agent"
efi="$target_dir/x86_64-unknown-uefi/$profile/efi-agent-uefi.efi"
"$launcher" package "$efi" artifacts
printf 'Profile: %s\nLauncher: %s\nVM tree: artifacts/esp\nNative tree: artifacts/native-esp\n' "$profile" "$launcher"
printf 'Images: artifacts/efi-agent-vm.img, artifacts/efi-agent-native.img\n'
