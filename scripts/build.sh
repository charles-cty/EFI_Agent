#!/usr/bin/env bash
# Build both programs, then assemble the selected packages.
set -euo pipefail
cd "$(dirname "$0")/.."
profile=release
vm_only=false
guests=artifacts/wasm-guests
wasi_sdk=${WASI_SDK_PATH:-}
binaryen=${BINARYEN_PATH:-}
jobs=4
usage() {
    printf 'Usage: bash scripts/build.sh [--profile release|debug] [--vm-only] [--guests DIR] [--wasi-sdk DIR] [--binaryen DIR] [--jobs N]\n'
}
while (($#)); do
    case "$1" in
        --profile)
            if (($# < 2)); then usage >&2; exit 2; fi
            profile=$2
            shift 2
            ;;
        --help) usage; exit 0 ;;
        --vm-only) vm_only=true; shift ;;
        --guests|--wasi-sdk|--binaryen|--jobs)
            if (($# < 2)); then usage >&2; exit 2; fi
            case "$1" in
                --guests) guests=$2 ;;
                --wasi-sdk) wasi_sdk=$2 ;;
                --binaryen) binaryen=$2 ;;
                --jobs) jobs=$2 ;;
            esac
            shift 2
            ;;
        *) usage >&2; exit 2 ;;
    esac
done
if [[ "$profile" != release && "$profile" != debug ]]; then
    printf 'Profile must be release or debug.\n' >&2
    exit 2
fi
if [[ "$vm_only" == false ]]; then
    shell_path=$(uv run scripts/build_guests.py --component shell)
    export EFI_AGENT_SHELL=$shell_path
fi
guest_args=(--output "$guests" --jobs "$jobs")
if [[ -n "$wasi_sdk" ]]; then guest_args+=(--wasi-sdk "$wasi_sdk"); fi
if [[ -n "$binaryen" ]]; then guest_args+=(--binaryen "$binaryen"); fi
uv run scripts/build_guests.py "${guest_args[@]}"
build_args=()
if [[ "$profile" == release ]]; then build_args+=(--release); fi
target_dir=${CARGO_TARGET_DIR:-target}
cargo build --locked -p efi-agent-uefi --target x86_64-unknown-uefi --target-dir "$target_dir" --jobs "$jobs" "${build_args[@]}"
cargo build --locked -p efi-agent --target-dir "$target_dir" --jobs "$jobs" "${build_args[@]}"
launcher="$target_dir/$profile/efi-agent"
efi="$target_dir/x86_64-unknown-uefi/$profile/efi-agent-uefi.efi"
"$launcher" catalog "$guests"
package_args=("$efi" artifacts "$guests")
if [[ "$vm_only" == true ]]; then package_args+=(--vm-only); fi
"$launcher" package "${package_args[@]}"
printf 'Profile: %s\nLauncher: %s\nVM tree: artifacts/esp\nVM image: artifacts/efi-agent-vm.img\n' "$profile" "$launcher"
if [[ "$vm_only" == false ]]; then
    printf 'Native tree: artifacts/native-esp\nNative image: artifacts/efi-agent-native.img\n'
fi
