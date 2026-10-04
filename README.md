# EFI Agent

A Rust UEFI coding agent with a Ratatui interface. The firmware application uses
`no_std` and `alloc`. It does not load an operating system.

## Current state

This is an early implementation, not a complete coding agent. The shared TUI,
ANSI serial backend, UEFI SimpleText backend, boot-volume file commands, host
terminal relay, host RPC service, and guest TCP4 transport are implemented.
A real Linux KVM/OVMF test has verified TUI rendering, a host file read, and
a Chat Completions round trip and a guest-driven read/edit/write tool loop
against a local simulated provider. Bare-metal networking, Windows WHPX, and interactive launcher
terminal verification are still pending.

The interface follows the Grok Build header, conversation area, prompt separator,
and muted status-line design. It does not use Grok Build source code or branding.
Reference repository: https://github.com/xai-org/grok-build (reviewed at
`2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8`).

## Windows build

Use PowerShell 7.4 or later with Rust 1.93 or later:

```powershell
rustup.exe target add x86_64-unknown-uefi
./scripts/build.ps1 -Release
cargo.exe test
```

The build creates `artifacts/esp/EFI/BOOT/BOOTX64.EFI`. The `VM.TXT` marker
selects serial input and output. For bare metal, copy `BOOTX64.EFI` to a FAT ESP
and omit this marker. Bare metal uses firmware text input and output.

## VM launch

Install QEMU and supply an OVMF image that includes VirtioSerialDxe and
VirtioNetDxe. WHPX must be enabled on Windows; KVM must be available on Linux.
The launcher uses the current terminal, with no graphical QEMU window.

```powershell
./target/debug/efi-agent.exe vm 'C:\Program Files\qemu\qemu-system-x86_64.exe' 'C:\firmware\OVMF_CODE.fd' 'C:\firmware\OVMF_VARS.fd' ./artifacts/esp ./workspace
```

The VM currently uses read-only QEMU vvfat for the boot files. HostBridge supplies
writable host files separately. Supply matching OVMF code and variable-store
images. QEMU uses a temporary snapshot of the variable store, so booting does not
modify the supplied template. Read-only vvfat is attached through virtio-blk.

The VM supports `/host-list`, `/host-read <path>`, and
`/host-write <path> <text>`. A normal prompt calls the provider configured on the
host. The agent can read UTF-8 files or directory listings, write files, and
replace one exact occurrence with `edit`. It returns tool results to the model
and stops after at most twelve rounds of executed tools. Slash commands are
separate from model history. `/clear` resets the conversation.

The TUI shows model and tool status, tool arguments, bounded result previews,
and the latest conversation output. Up/Down scroll older/newer output.
Requests currently block guest input until completion or timeout; status redraws
between operations do not provide cancellation during a network wait.

## HostBridge

Frames contain a four-byte big-endian length and JSON, with a 1 MiB limit.
Requests carry an ID and a tagged operation: `list`, `read`, `write`, `edit`, or
`complete`. Responses carry the same ID and a result. Files are UTF-8. Paths
are relative to the configured workspace. Parent traversal and resolved paths
outside that workspace are rejected. Concurrent filesystem changes are not
isolated; this service is intended for a local, trusted workspace.

The launcher forwards guest TCP `10.0.2.100:7420` to a loopback service through
QEMU user networking. API credentials stay on the host:

```powershell
$env:EFI_AGENT_API_BASE = 'https://provider.example/v1'
$env:EFI_AGENT_API_KEY = 'your-key'
$env:EFI_AGENT_MODEL = 'your-model'
./target/debug/efi-agent.exe serve ./workspace
```

No third-party provider call has been verified. `complete` returns a serialized
Chat Completions assistant message with optional function calls. The host sends
the three tool definitions, while the UEFI application owns the model loop and
requests file operations over RPC. Invalid arguments and failed operations
become correlated tool error messages. `edit` refuses zero or multiple matches,
including overlapping matches. Files have a 512 KiB size limit.

## Linux VM smoke test

Build in a Linux-local checkout with `scripts/build.sh`. With QEMU, KVM access,
and matching OVMF images available, run:

```sh
uv run scripts/smoke_vm.py \
  --qemu qemu-system-x86_64 --accel kvm \
  --code /usr/share/OVMF/OVMF_CODE_4M.fd \
  --vars /usr/share/OVMF/OVMF_VARS_4M.fd \
  --esp artifacts/esp --launcher target/debug/efi-agent \
  --output artifacts/smoke
```

This boots the UEFI image and checks the actual virtio serial and TCP4 paths.
It uses a local HTTP provider with deterministic text and tool-call responses,
checks actual edited and created files, and needs no API key.
It does not verify the launcher's current-terminal relay.

## Work remaining

- Bare-metal endpoint configuration and direct provider networking.
- Host command execution and native UEFI tool routing.
- Responsive model requests, cancellation, multiline editing, and tool views.
- FAT image packaging.
- Windows WHPX and Linux KVM end-to-end tests.
- Bare-metal file and network configuration and hardware verification.

GOP, virtio-fs, and macOS HVF support are outside the current scope.
