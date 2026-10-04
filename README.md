# EFI Agent

A Rust UEFI coding agent with a Ratatui interface. The firmware application uses
`no_std` and `alloc`. It does not load an operating system.

## Current state

This is an early implementation, not a complete coding agent. The shared TUI,
ANSI serial backend, UEFI SimpleText backend, boot-volume file commands, host
terminal relay, and host RPC service are implemented. The guest TCP4 transport,
model tool loop, and firmware runtime verification are still pending.

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
./target/debug/efi-agent.exe vm 'C:\Program Files\qemu\qemu-system-x86_64.exe' 'C:\firmware\OVMF_CODE.fd' ./artifacts/esp ./workspace
```

The VM currently uses read-only QEMU vvfat for the boot files. HostBridge supplies
writable host files separately. The launcher has not yet been tested against a
live VM. It requires an OVMF code image that can boot without a separate writable
variable-store image; broader firmware packaging remains pending.

## HostBridge

Frames contain a four-byte big-endian length and JSON, with a 1 MiB limit.
Requests carry an ID and a tagged operation: `list`, `read`, `write`, or
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

No live model call has been verified. `complete` currently returns a single
Chat Completions text response; it does not yet execute model tool calls.

## Work remaining

- Guest TCP4 transport, configuration, and timeouts.
- Minimal agent loop with read, write, edit, and command tools.
- Responsive model requests, cancellation, multiline editing, and tool views.
- FAT image packaging and OVMF code/variable-store support.
- Windows WHPX and Linux KVM end-to-end tests.
- Bare-metal file and network configuration and hardware verification.

GOP, virtio-fs, and macOS HVF support are outside the current scope.
