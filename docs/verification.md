# Verification record

## 2026-10-04: initial implementation

Environment: Windows native Rust 1.93.1, invoked from WSL with PowerShell 7
and a Windows-local working directory.

Verified:

- `cargo check` for the native launcher and shared core.
- `cargo check -p efi-agent-uefi --target x86_64-unknown-uefi`.
- Release linking of the UEFI application and ESP directory creation.
- Native and UEFI Clippy checks with warnings treated as errors.
- RPC length boundaries (0, 1, 256, 1 MiB, and 1 MiB + 1).
- Fragmented Unicode input and asymmetric terminal resize dimensions.
- Recovery after malformed resize input and rejection of zero dimensions.
- A real loopback TCP connection with requests sent one byte at a time,
  response ID matching, and UTF-8 file reads from an actual temporary directory.

Not verified:

- UEFI execution, either on hardware or under OVMF.
- OVMF VirtioSerialDxe availability or device binding.
- QEMU launch, WHPX, KVM, or ConPTY terminal behavior.
- Guest TCP4, host RPC from the guest, or a live provider response.
- Bare-metal network and file writes.
- Linux native builds or runtime.

Windows firmware virtualization is enabled. QEMU was not found in PATH or the
usual `C:\Program Files\qemu` and `C:\Programs\qemu` directories. This does not
prove it is absent from all other locations. No VM test has been claimed.

The full objective remains open. See the README work list. Compilation and host
tests do not establish firmware or hypervisor support.

## 2026-10-04: TCP4 and KVM runtime

Implemented guest TCP4 service binding and child ownership, DHCP initialization,
connect/transmit/receive completion events, deadlines, cancellation, and framed
RPC. Flexible-array packet offsets are checked at compile time. Completion
contexts stay allocated until CloseEvent; a queued token must complete or be
cancelled before its storage is released. Network operations currently block
guest input; interactive cancellation remains pending.

The VM now calls HostBridge for host file commands and normal model prompts.
The launcher accepts paired OVMF CODE and VARS images and uses a snapshot for
VARS. A real boot found two faults in the initial launcher: a missing variable
store and an unsuitable default disk attachment for read-only vvfat. The boot
disk now uses virtio-blk with explicit read-only access.

The first KVM TUI render panicked in uefi-rs Serial::write: VirtioSerialDxe
reported SUCCESS for a short 128-byte write out of 4100 bytes. The screen dump
showed the exact assertion. The serial backend now uses the protocol's returned
byte count and retries the remaining output. It also honors actual read counts.
These calls retain uefi-rs protocol ownership and use the raw ABI only for I/O.

Runtime environment: QEMU 8.2.2, Linux KVM, distro OVMF_CODE_4M.fd and
OVMF_VARS_4M.fd, Windows-built release BOOTX64.EFI copied to a Linux-local ESP,
and a Linux-native HostBridge executable built with Rust 1.97.1.

`uv run scripts/smoke_vm.py` passed all three runtime checks:

1. OVMF starts BOOTX64.EFI and the custom serial backend renders the TUI.
2. A resize and `/host-read needle.txt` reach the guest; EFI TCP4 over virtio-net
   and QEMU guestfwd returns the actual host file marker.
3. A normal prompt reaches a local HTTP Chat Completions provider. The provider
   checks the URL, Authorization header, model, final message, and stream flag.
   Its response reaches the guest and appears in the serial TUI transcript.

The test saves serial bytes, a transcript, and process logs. It uses a simulated
provider; it does not establish third-party provider compatibility. It launches
QEMU directly to test the guest transport and does not exercise the native
launcher's interactive terminal relay. Linux host tests also pass. Windows
native host tests, host Clippy, UEFI Clippy, and release linking pass.

Still unverified: Windows WHPX/ConPTY, interactive Linux launcher behavior,
bare-metal hardware, cancellation under stalled firmware, and a real provider.
Still incomplete: model tool execution, richer TUI controls, bare-metal network
configuration, direct provider access, and FAT image creation. The goal is open.
