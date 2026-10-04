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
