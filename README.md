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

See [UEFI safety](docs/uefi-safety.md) for the application-level event dispatcher,
callback rules, watchdog handling, cooperative single-processor execution, and
firmware network and cryptographic RNG capability checks. `/capabilities`
shows the detected firmware protocols and RNG result.

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

Run the commands below from your project checkout. External tool locations are
discovered through PATH or supplied as arguments. Firmware files use the
project-relative `artifacts/firmware` directory; no particular drive, user
directory, project location, or WSL distribution name is required.

Install QEMU and supply an OVMF image that includes VirtioSerialDxe and
VirtioNetDxe. WHPX must be enabled on Windows; KVM must be available on Linux.
Use a current QEMU release whose socket chardev supports `reconnect-ms`.
The bridge socket reconnects every second after a host disconnect.

### Prepare OVMF manually on Windows

Use PowerShell 7 and a current 7-Zip release. This procedure does not require
WSL or an EDK II build.

1. Open the [Ubuntu package search for ovmf](https://packages.ubuntu.com/search?keywords=ovmf).
   Select a currently supported Ubuntu release, open its `ovmf` package page,
   and follow the download link for the current `ovmf_*_all.deb` package.
   The package contains VM firmware that also works with Windows QEMU.
2. Obtain the SHA256 value for that selected package from its official download
   page. Run `Get-FileHash -Algorithm SHA256 -LiteralPath '<downloaded-package.deb>'`
   in PowerShell and compare the result. Use the value for the package you
   downloaded; this guide does not pin a version or checksum.
3. Open the package in 7-Zip. Open its `data.tar.*` member and the inner tar
   archive, then browse to `usr/share/OVMF`.
4. Create `artifacts\firmware` in the project directory. Extract
   `OVMF_CODE_4M.fd` and `OVMF_VARS_4M.fd` into that directory. Take both files
   from the same package. Use the ordinary variants for this unsigned UEFI
   application; do not select `.ms` or `.secboot` variants. Do not mix package
   versions or 2 MiB and 4 MiB flash layouts.
5. Use the pair in the launch command below. After boot, confirm that the agent
   interface appears, run `/capabilities` to inspect firmware capabilities, and
   send a prompt to check the network and model request. A download and checksum
   check alone do not establish firmware compatibility. Repeat these runtime
   checks when you replace the firmware pair.

`OVMF_CODE_4M.fd` contains the firmware code. `OVMF_VARS_4M.fd` is the matching
UEFI variable-store template. Keep the extracted template: the launcher uses a
temporary snapshot and does not modify it.

### Copy OVMF from WSL instead

If your WSL distribution already provides OVMF, you can copy its firmware pair
instead of downloading and unpacking a package on Windows. On Ubuntu or Debian
under WSL, install or update the package if needed:

```bash
sudo apt update
sudo apt install ovmf
ovmf_dir=$(dpkg-query -L ovmf | awk '/\/OVMF_CODE_4M[.]fd$/ { sub(/\/[^/]*$/, ""); print }')
test -n "$ovmf_dir" && ls -l "$ovmf_dir/OVMF_CODE_4M.fd" "$ovmf_dir/OVMF_VARS_4M.fd"
wslpath -w "$ovmf_dir"
```

The last command prints the Windows UNC path for your distribution's OVMF
directory. If the package does not contain this firmware pair, select a package
that does before continuing. In Windows PowerShell 7, open your terminal in the
Windows project checkout, replace
the source placeholder with that printed path, and copy both files:

```powershell
$source = '<Windows path printed by wslpath>'
New-Item -ItemType Directory -Force -Path .\artifacts\firmware | Out-Null
Copy-Item -LiteralPath (Join-Path $source 'OVMF_CODE_4M.fd') -Destination .\artifacts\firmware\OVMF_CODE_4M.fd
Copy-Item -LiteralPath (Join-Path $source 'OVMF_VARS_4M.fd') -Destination .\artifacts\firmware\OVMF_VARS_4M.fd
```

Use the UNC path only as the copy source. Launch Windows QEMU with the copied
files in `artifacts\firmware`, not with files on the WSL UNC path. Copy the pair
together after a package update, and perform the same runtime checks described
above. Once copied, Windows VM launches do not require WSL.

### Start the VM

The launcher uses the current terminal, with no graphical QEMU window.
It waits for an application readiness marker before forwarding input, so OVMF
cannot interpret initial terminal dimensions as firmware menu keystrokes.
`/quit` or `/exit` shuts down the VM and returns to the host shell. On bare metal it returns
to firmware. Ctrl+C immediately exits the launcher, including during guest waits.

```powershell
$qemu = (Get-Command qemu-system-x86_64.exe -CommandType Application -ErrorAction Stop).Source
./target/debug/efi-agent.exe vm $qemu ./artifacts/firmware/OVMF_CODE_4M.fd ./artifacts/firmware/OVMF_VARS_4M.fd ./artifacts/esp ./workspace
```

If QEMU is not on PATH, set `$qemu` to your executable's actual path instead.

VM memory defaults to 128 MiB. Append `--memory-mib 256` to the launch command
to allocate 256 MiB, or supply another positive integer in MiB. This setting
applies to both Windows WHPX and Linux KVM. Smaller values depend on the OVMF
image and workload and must be verified before use.

The launcher accepts an ESP directory (read-only QEMU vvfat) or a raw boot disk
image (read-only virtio-blk). HostBridge supplies writable host files separately.
Supply matching OVMF code and variable-store images. QEMU uses a temporary snapshot of the variable store, so booting does not
modify the supplied template. Read-only vvfat is attached through virtio-blk.

Slash commands control the session: `/help`, `/clear`, `/capabilities`,
`/quit`, and `/exit`. File operations are agent tools, not slash commands.
A normal prompt calls the provider configured on the host. The agent can read UTF-8 files or directory listings, write files, and
replace one exact occurrence with `edit`. It returns tool results to the model
and stops after at most twelve rounds of executed tools. All tools use one
configured workspace: the launcher workspace in VM mode or the boot-volume
workspace in native mode. Slash commands are
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

HostBridge connections stay open while the application is idle. After the first
connection, the guest sends a heartbeat after 30 seconds without RPC activity.
A failed heartbeat clears the transport and reconnects automatically; if the
relay remains unavailable, the idle loop retries on the next heartbeat interval.
Heartbeat responses have a five-second deadline. User input remains buffered and
Esc/Ctrl+C can cancel the wait. Only heartbeats are retried automatically; file
operations and model requests are never replayed. The host applies its read
timeout only after a frame starts, so normal inactivity does not close a session.
Connection setup and frame transmission retain their own bounded timeouts; the
five-second deadline applies to the heartbeat reply after transmission.

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
  --code artifacts/firmware/OVMF_CODE_4M.fd \
  --vars artifacts/firmware/OVMF_VARS_4M.fd --output artifacts/native-smoke
```

## HostBridge

The launcher and relay validate API configuration before starting. Missing or
blank base URL, key, or model produces an immediate error. The base must be an
HTTP or HTTPS URL. Each prompt goes directly to the model request. There is no separate API
availability probe. Connection failures and provider HTTP errors, including
authentication, quota, and billing errors, are reported from the actual request.

Frames contain a four-byte big-endian length and JSON, with a 1 MiB limit.
Requests carry an ID and a tagged operation: `ping`, `read`, `write`, `edit`, or
`complete`. Responses carry the same ID and a result. A model request can send
text progress frames with a `delta` field before its final response. The guest
uses request IDs to discard both progress and final frames from cancelled calls.
Files are UTF-8. Paths
are relative to the configured workspace. Parent traversal and resolved paths
outside that workspace are rejected. Concurrent filesystem changes are not
isolated; this service is intended for a local, trusted workspace.

The launcher forwards guest TCP `10.0.2.100:7420` to a loopback service through
QEMU user networking. API credentials stay on the host:

```powershell
$env:EFI_AGENT_API_BASE = 'https://provider.example/v1'
$env:EFI_AGENT_API_KEY = 'your-key'
$env:EFI_AGENT_MODEL = 'your-model'
$env:EFI_AGENT_REASONING_EFFORT = 'medium'
./target/debug/efi-agent.exe serve ./workspace
```

Set the base URL to the API root, for example `https://provider.example/v1`.
The host appends `/chat/completions`. Set the key and model on the launcher or
relay host; credentials do not enter the UEFI application.

Reasoning is enabled by default with `reasoning_effort: "medium"`.
`EFI_AGENT_REASONING_EFFORT` accepts `none`, `minimal`, `low`, `medium`, `high`,
or `xhigh`. Omit it to use `medium`; use `none` to request no reasoning.
The selected provider and model must support the requested value and the
Chat Completions `reasoning_effort` field. Unsupported settings return an API
error. The host does not retry with reduced reasoning. For Linux, use
`export EFI_AGENT_REASONING_EFFORT=high` before starting the launcher or relay.

Model replies use SSE streaming. Assistant text appears as it arrives in VM
and native mode. The host assembles tool IDs, names, and JSON arguments across
chunks. Tools run only after a complete, successful model reply. Truncated
streams and `length` or `content_filter` finish reasons report an error and do
not execute partial tool calls. Partial text remains visible after an error
or cancellation but does not enter model history. Returned `reasoning_content`
is retained in assistant history for subsequent tool rounds; it is not shown
as answer text. Other provider-specific reasoning formats and signatures are
not supported. The host limits stream wire data to 8 MiB and each assembled
message to less than 1 MiB. The existing 120-second API timeout applies to the
whole stream.

No third-party provider call has been verified. `complete` returns a serialized
Chat Completions assistant message with optional function calls. The host sends
the three tool definitions, while the UEFI application owns the model loop and
requests file operations over RPC. Invalid arguments and failed operations
become correlated tool error messages. `edit` refuses zero or multiple matches,
including overlapping matches. Files have a 512 KiB size limit.

## Linux VM smoke test

Build in a Linux-local checkout with `scripts/build.sh`. With QEMU, KVM access,
and matching OVMF images copied to `artifacts/firmware` in that checkout, run:

```sh
uv run scripts/smoke_vm.py \
  --qemu qemu-system-x86_64 --accel kvm \
  --code artifacts/firmware/OVMF_CODE_4M.fd \
  --vars artifacts/firmware/OVMF_VARS_4M.fd \
  --esp artifacts/esp --launcher target/debug/efi-agent \
  --output artifacts/smoke
```

This boots the UEFI image and checks the actual virtio serial and TCP4 paths.
It uses a local SSE provider with deterministic text and fragmented tool calls.
It checks text before stream completion, default reasoning effort, retained
reasoning content across tool rounds, actual edited and created files, and
refusal to execute truncated tool replies. It needs no API key.
It does not verify the launcher's current-terminal relay.

To test the Linux launcher through an actual tmux PTY:

```sh
uv run scripts/smoke_launcher.py --launcher target/debug/efi-agent \
  --code artifacts/firmware/OVMF_CODE_4M.fd \
  --vars artifacts/firmware/OVMF_VARS_4M.fd \
  --esp artifacts/esp --output artifacts/launcher-smoke
```

This checks initial and changed dimensions, Unicode prompt input, Backspace,
cursor editing, multiline input, bracketed paste, `/exit`, Ctrl+C, termios
restoration, and leaving the alternate screen.

## Windows VM smoke tests

Run the same `scripts/smoke_vm.py` test with Windows-local paths and
`--accel whpx`. Use `uv.exe run` in PowerShell. This checks serial rendering,
TCP4 file operations, and a deterministic Chat Completions tool loop.

To check the native launcher through a Windows ConPTY session, use psmux:

```powershell
$qemu = (Get-Command qemu-system-x86_64.exe -CommandType Application -ErrorAction Stop).Source
./scripts/smoke_windows.ps1 -Qemu $qemu -Code ./artifacts/firmware/OVMF_CODE_4M.fd -Vars ./artifacts/firmware/OVMF_VARS_4M.fd
```

The script discovers `psmux.exe` through PATH. If it is installed elsewhere,
pass its executable path with `-Psmux`. Supply your QEMU executable path with
`-Qemu` if it is not on PATH.

The test checks Unicode prompt input, Backspace, cursor editing, multiline input,
`/exit`, Ctrl+C, and shell recovery. It saves captures under
`artifacts/windows-smoke`. It does not test physical Windows Terminal keystrokes,
Windows clipboard paste, or live window resize.

## Work remaining

- Direct HTTPS provider networking and physical bare-metal verification.
- Host command execution.
- Richer tool views and conversation navigation.
- Windows live resize, clipboard paste, and physical terminal input checks.
- Bare-metal file and network configuration and hardware verification.

GOP, virtio-fs, and macOS HVF support are outside the current scope.
