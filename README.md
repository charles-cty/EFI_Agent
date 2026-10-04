# EFI Agent

A Rust UEFI coding agent with a Ratatui interface. The firmware application uses
`no_std` and `alloc`. It does not load an operating system.

## Current state

This is an early implementation, not a complete coding agent. The shared TUI,
ANSI serial backend, UEFI SimpleText backend, boot-volume file commands, host
terminal relay, host RPC service, and guest TCP4 transport are implemented.
A real Linux KVM/OVMF test has verified TUI rendering, a host file read, and
a Chat Completions round trip and a guest-driven read/edit/write tool loop
against a local simulated provider. The Linux launcher has also been tested in a
real tmux PTY for initial size, resize, Unicode input, host files, and exit
restoration. Native mode with a model relay and local FAT file tools has been
tested through SimpleText under OVMF. Windows WHPX transport and native ConPTY
input, file operations, and exit have also been tested. Physical bare-metal
networking and direct HTTPS provider access are still pending.

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

The build creates `artifacts/esp/EFI/BOOT/BOOTX64.EFI` and
`artifacts/efi-agent-vm.img`, a GPT disk with a 64 MiB FAT32 EFI System Partition.
The `VM.TXT` marker
selects serial input and output. For bare metal, copy `BOOTX64.EFI` to a FAT ESP
and omit this marker. Bare metal uses firmware text input and output.

For native agent use, create `EFI/AGENT/NATIVE.JSON` on the boot volume:

```json
{
  "relay_address": [192, 168, 1, 73],
  "relay_port": 7420,
  "workspace": "\\work"
}
```

The workspace directory must exist on the boot volume. Model file tools use
EFI_SIMPLE_FILE_SYSTEM_PROTOCOL inside that directory. Tool paths are relative;
absolute paths and parent traversal are rejected. Model requests use EFI TCP4
and DHCP to reach a configured model relay. On a trusted local network, run
`efi-agent serve <workspace> 0.0.0.0:7420` on the relay machine with the API
environment variables below. In native mode the relay only receives model
requests; file tools execute in the firmware application.

The current relay transport is unencrypted and unauthenticated. Use it only on
a trusted network. Direct HTTPS provider requests, relay TLS, and static network
configuration are not implemented. Physical hardware support remains unverified.
The native protocol path has been tested under OVMF with SimpleText and a writable
FAT boot volume, independently of VM mode and host-file routing.

## VM launch

Install QEMU and supply an OVMF image that includes VirtioSerialDxe and
VirtioNetDxe. WHPX must be enabled on Windows; KVM must be available on Linux.
The launcher uses the current terminal, with no graphical QEMU window.
It waits for an application readiness marker before forwarding input, so OVMF
cannot interpret initial terminal dimensions as firmware menu keystrokes.
`/quit` shuts down the VM and returns to the host shell. On bare metal it returns
to firmware. Ctrl+C immediately exits the launcher, including during guest waits.

```powershell
./target/debug/efi-agent.exe vm 'C:\Program Files\qemu\qemu-system-x86_64.exe' 'C:\firmware\OVMF_CODE.fd' 'C:\firmware\OVMF_VARS.fd' ./artifacts/esp ./workspace
```

The launcher accepts an ESP directory (read-only QEMU vvfat) or a raw boot disk
image (read-only virtio-blk). HostBridge supplies writable host files separately.
Supply matching OVMF code and variable-store images. QEMU uses a temporary snapshot of the variable store, so booting does not
modify the supplied template. Read-only vvfat is attached through virtio-blk.

The VM supports `/host-list`, `/host-read <path>`, and
`/host-write <path> <text>`. A normal prompt calls the provider configured on the
host. The agent can read UTF-8 files or directory listings, write files, and
replace one exact occurrence with `edit`. It returns tool results to the model
and stops after at most twelve rounds of executed tools. Slash commands are
separate from model history. `/clear` resets the conversation.

The TUI shows model and tool status, tool arguments, bounded result previews,
and the latest conversation output. Up/Down scroll older/newer output, including
during model waits. Esc cancels the active request and returns to Ready.
Ctrl+C exits. VM dimensions update during network waits. Other input is buffered
until the current operation ends, with a limit of 65,536 input events.

Cancellation stops the guest wait and further tools. It cannot undo a file
operation already sent to the host or stop a synchronous firmware file call.
The host provider call may continue until its response or 120-second timeout.
The relay allows four model requests in flight so a new request can proceed
while a cancelled call finishes. Responses retain their request IDs; cancelled
responses are discarded without losing partially received frame boundaries.

The prompt editor supports Left/Right, Home/End (current line), Backspace, and
Delete at Unicode grapheme boundaries. Ctrl+J inserts a newline; Enter submits
the whole prompt. Alt/Shift+Enter also inserts a newline when the host terminal
reports that modifier. Ctrl+Enter also inserts a newline. The prompt expands to
six visible lines and follows the editing cursor. Prompts are limited to 64 KiB.

The launcher enables bracketed paste. Pasted line breaks remain inside a single
prompt and require an explicit Enter to submit. Terminal control characters
are removed from paste; tabs and line breaks are preserved, and CRLF is
normalized to LF. Bare-metal keyboards use the same editor with firmware key
codes; the exact modified-key support depends on firmware.

## Boot image packaging

Both build scripts create `artifacts/efi-agent-vm.img`. Pass this path instead
of the ESP directory to `efi-agent vm`. The image has a protective MBR, primary
and backup GPT tables, and a FAT32 EFI System Partition starting at sector 2048.
Its total size is 66 MiB. Each build replaces only this generated artifact after
the new image is complete.

To package a separate boot tree:

```powershell
./target/debug/efi-agent.exe pack ./native-esp ./efi-agent-native.img
```

The source must contain `EFI/BOOT/BOOTX64.EFI`. For native mode, omit
`EFI/AGENT/VM.TXT`, include `EFI/AGENT/NATIVE.JSON`, and create the configured
workspace directory in that tree before packaging. Packaging copies all regular
files, including workspace contents. It supports BMP Unicode long filenames and
empty files and directories. It rejects links, Windows reparse points,
case-insensitive name collisions, invalid FAT names, non-BMP filename characters,
more than 4096 entries,
more than 32 directory levels, or more than 48 MiB of file data.

`pack` creates a new regular file and refuses to overwrite an existing path.
Keep its output outside the source tree. It does not flash hardware or write a
physical disk. The native disk image has been tested with writable virtio-blk
under OVMF; physical USB media and firmware remain unverified. Firmware must
permit this unsigned application to run.

To check the package with independent Linux tools:

```sh
uv run scripts/smoke_pack.py --launcher target/debug/efi-agent \
  --efi artifacts/esp/EFI/BOOT/BOOTX64.EFI --output artifacts/pack-smoke
```

This needs sgdisk, fsck.fat, and mtools. Native protocol testing now also uses
the pack command:

```sh
uv run scripts/smoke_native.py --launcher target/debug/efi-agent \
  --efi artifacts/esp/EFI/BOOT/BOOTX64.EFI \
  --code /usr/share/OVMF/OVMF_CODE_4M.fd \
  --vars /usr/share/OVMF/OVMF_VARS_4M.fd --output artifacts/native-smoke
```

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

To test the Linux launcher through an actual tmux PTY:

```sh
uv run scripts/smoke_launcher.py --launcher target/debug/efi-agent \
  --code /usr/share/OVMF/OVMF_CODE_4M.fd \
  --vars /usr/share/OVMF/OVMF_VARS_4M.fd \
  --esp artifacts/esp --output artifacts/launcher-smoke
```

This checks initial and changed dimensions, Unicode file input, Backspace,
cursor editing, multiline input, bracketed paste, `/quit`, Ctrl+C, termios
restoration, and leaving the alternate screen.

## Windows VM smoke tests

Run the same `scripts/smoke_vm.py` test with Windows-local paths and
`--accel whpx`. Use `uv.exe run` in PowerShell. This checks serial rendering,
TCP4 file operations, and a deterministic Chat Completions tool loop.

To check the native launcher through a Windows ConPTY session, use psmux:

```powershell
./scripts/smoke_windows.ps1 -Qemu 'C:\qemu\qemu-system-x86_64.exe' -Code 'C:\firmware\OVMF_CODE_4M.fd' -Vars 'C:\firmware\OVMF_VARS_4M.fd'
```

The test checks Unicode file input, Backspace, cursor editing, multiline input,
`/quit`, Ctrl+C, and shell recovery. It saves captures under
`artifacts/windows-smoke`. It does not test physical Windows Terminal keystrokes,
Windows clipboard paste, or live window resize.

## Work remaining

- Direct HTTPS provider networking and physical bare-metal verification.
- Host command execution.
- Richer tool views and conversation navigation.
- Windows live resize, clipboard paste, and physical terminal input checks.
- Bare-metal file and network configuration and hardware verification.

GOP, virtio-fs, and macOS HVF support are outside the current scope.
