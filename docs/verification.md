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

## 2026-10-04: minimal coding-agent loop

The no_std core now owns Chat Completions model history and a bounded function
call loop. UEFI executes model-selected `read`, `write`, and `edit` through
HostBridge, correlates tool results with tool_call_id, and requests another model
response. Tool errors return to the model. Assistant message roles, duplicate
call IDs, argument schemas, per-response call count, conversation size, file
size, and the tool round limit are checked. Slash commands stay out of model
history. `/clear` resets it. Tool previews and status updates are rendered
between network operations; output follows the bottom with wrapped-line-aware
scrolling. Input remains blocked during the synchronous network operation.

High-risk edit semantics were checked against an independent exhaustive oracle:
all strings of length zero through five over `a`, `b`, and `中`, with patterns
through length three. The oracle enumerates every character boundary and counts
matches, including overlaps, before deriving the expected replacement. Other
checks cover exact deletion, zero matches, empty patterns, size bounds, duplicate
call IDs, correlated failure results, and stopping file operations at the round
limit with all pending tool calls answered.

The real KVM/OVMF smoke test now requests four successive tool calls: read an
actual host file, replace its unique content with `edited 中 853`, create another
file with a trailing newline, and attempt to replace `aa` in `aaa`. The local
provider checks each correlated result, including the ambiguous-edit error,
before returning the final marker. The test independently reads the files and
checks the exact final bytes/text; the ambiguous file must stay `aaa`.

All four smoke stages pass (serial rendering, explicit host read, text model
response, and the guest-driven tool loop). Windows and Linux native tests pass;
Windows host/UEFI Clippy with warnings denied and UEFI release linking pass.
The service also caps HTTP response reads at 1 MiB and file reads at 512 KiB.

The first run after adding progress redraws exposed a false negative in the
smoke verifier: removing ANSI codes from an incremental draw does not reconstruct
the terminal screen. The final marker's unchanged cells were absent from the
byte log. The verifier now feeds serial bytes into a pyte terminal emulator,
checks the actual screen state, applies resize dimensions, and saves screen.txt.
The full KVM smoke test passes with this corrected verifier and final build.

Not complete: command execution, responsive cancellation, multiline editor and
other complex TUI controls, bare-metal native agent/network configuration,
Windows WHPX/ConPTY, current-terminal launcher tests, and real-provider tests.
The runtime test still uses a local simulated provider and directly launches
QEMU. The original full goal remains open.

## 2026-10-04: native Linux terminal launcher

The first real tmux launcher test booted into the OVMF menu rather than the app:
the launcher's initial resize bytes arrived before firmware boot completed.
The guest now emits a private OSC readiness marker. The launcher removes that
marker from terminal output, waits before forwarding input, and responds with
the current terminal dimensions. The guest waits briefly for dimensions before
its first frame. Marker parsing is checked at every possible socket split, with
one-byte reads, repeated markers, false prefixes, Unicode bytes, and incomplete
suffixes. Other terminal bytes remain unchanged.

VM `/quit` now uses UEFI shutdown, allowing QEMU and the launcher to exit normally.
Bare-metal exit still returns to firmware. A resize before event source startup
was observed during the test; the launcher now reconciles actual size every
250 ms as well as forwarding resize events.

`uv run scripts/smoke_launcher.py` uses a unique tmux server and real PTY. It
starts the native launcher, which starts QEMU/KVM and its own HostBridge service.
The checks cover an 83x23 initial frame, a 107x31 resize, Unicode host-file write,
Backspace and host-file read, `/quit`, and a second launch exited with Ctrl+C.
The test compares exact termios state before and after both exits and verifies
the alternate screen is no longer active. It saves captured screen text. All
checks pass. The direct KVM model/tool smoke also passes with the handshake.

Windows host tests and host/UEFI Clippy pass; UEFI release linking passes. This
proves the Linux current-terminal route, not Windows ConPTY or physical hardware.
Bare-metal networking, Windows WHPX, complex editing controls, in-guest request
cancellation, command tools, and a real provider remain incomplete/unverified.

## 2026-10-04: multiline Unicode prompt editor

The prompt now uses a no_std extended-grapheme editor. Left/Right, Home/End,
Delete, Backspace, and newline insertion maintain a valid UTF-8 and grapheme
cursor boundary. Insertion and deletion can join adjacent combining or ZWJ
sequences, so the cursor is normalized after mutations. The UI marks the cursor,
grows to six prompt lines, and scrolls its viewport to the current position.
Ctrl+J inserts a newline; Enter submits. Firmware scan codes route to the same
editor. Bare-metal execution of these keys remains unverified.

The native launcher now enables bracketed paste and normalizes CRLF/CR to LF.
It removes pasted terminal controls while retaining tabs/newlines. The guest
decoder treats line breaks inside bracketed paste as input, never submit.
The launcher's terminal cleanup disables bracketed paste before restoring the
alternate screen. Modified Enter inserts a newline where terminal input exposes
the modifier. There is a 64 KiB prompt limit.

Structured randomized testing exercises 12,000 edits using ASCII, Chinese,
combining marks, ZWJ emoji parts, regional indicators, and newlines. After every
operation it independently checks the cursor against full-string grapheme
boundaries. Fixed checks cover Unicode deletion, combining insertion, empty
boundaries, and fragmented multiline paste. All tests pass.

The expanded real tmux/KVM launcher test verifies an actual edited host file,
Ctrl+J input saved with exact newline content, and bracketed pasted content
containing `/quit` and Greek text. Neither newline nor paste creates a file
before explicit Enter; the final file contents are checked independently.
The existing resize, host RPC, quit, and terminal-restoration checks pass.

A resize test exposed `Interrupted` from socket Read on SIGWINCH. The launcher
now continues on interrupted socket/event reads rather than exiting. Host and
UEFI Clippy and release linking pass. Complex TUI work remains incomplete:
history navigation, richer tool views, responsive cancellation, and other
controls still need implementation; Windows ConPTY and physical hardware
verification remain open.

## 2026-10-04: native UEFI file tools and model relay

Native mode loads EFI/AGENT/NATIVE.JSON with a unicast IPv4 relay address,
nonzero port, and absolute UEFI workspace directory. Configuration rejects
unknown fields and unsafe path syntax. File tool paths are relative to that
workspace, reject parent traversal and absolute paths, and use the boot volume's
EFI_SIMPLE_FILE_SYSTEM_PROTOCOL. read can list a directory. write and edit obey
the same file size and unique-match limits as VM tools. Only model requests go
to the configured relay through DHCP/EFI TCP4. VM host commands remain separate.

`scripts/smoke_native.py` creates a writable FAT image with mtools, omits VM.TXT,
boots the release application with OVMF/KVM, and injects virtual keyboard input
through QMP. It has no serial TUI or host launcher. A local framed model relay
checks six model rounds and verifies that it receives no file operations.
The native agent reads seed.txt, edits it to `native 中 419`, creates a file with
an exact trailing newline, rejects `../outside.txt`, and lists the workspace.
After stopping QEMU, mtype independently checks the persisted file contents.
All checks pass. The relay is deterministic, not a real LLM provider.

Boot and final SimpleText screenshots were inspected. They show the native
workspace, file tool results, traversal error, and NATIVE_TOOLS_VERIFIED response.
Core configuration tests, Windows host tests, host/UEFI Clippy, and UEFI release
linking pass. This establishes execution through native firmware protocols under
OVMF. It does not prove physical NIC/firmware compatibility.

Remaining native gaps: direct HTTPS provider access, authenticated/encrypted
relay transport, static IP configuration, physical hardware verification, and
responsive request cancellation. The current relay is for trusted local networks
and uses plain framed TCP. Windows WHPX and broader TUI work remain open.

## 2026-10-04: Windows WHPX and ConPTY verification

Windows Hypervisor Platform is enabled on the test host. QEMU 11.1.0
(`v11.1.0-12130-ge470268ff4`, Windows distribution dated 2026-08-11) was
downloaded and unpacked into a Windows-local temporary directory. Tests used
the matched OVMF_CODE_4M.fd and OVMF_VARS_4M.fd templates already used on Linux.
QEMU ran with `q35,accel=whpx`, with no graphical display.

`uv.exe run scripts/smoke_vm.py --accel whpx` passed serial TUI rendering,
guest TCP4 host file read, a Chat Completions request, and the guest-driven
read/edit/write loop with correlated tool results and ambiguous-edit refusal.
The provider is deterministic local HTTP, not a third-party model service.
The script now stops its children before removing their temporary working
directory; Windows previously refused that cleanup while a child was alive.
The revised script also passed under Linux KVM.

`scripts/smoke_windows.ps1` passed in psmux 3.3.4 using a native Windows
ConPTY and PowerShell 7. It checked Unicode input against actual host file
contents, Backspace, cursor editing, Home/End, and exact multiline file content.
Ctrl+J did not submit before explicit Enter. It checked `/quit` and Ctrl+C by
executing fresh shell commands after exit, verified that the alternate screen
was inactive after both exits, and checked that Ctrl+C left no test QEMU alive.
Captures are saved under artifacts/windows-smoke.

The Windows tests exposed two launcher faults. Control-modified Enter is now
mapped to newline because Windows can report Ctrl+J that way. A socket reset
during firmware shutdown is accepted only after QEMU exits successfully.
The launcher also installs a Ctrl+C handler so a Windows console control event
returns through normal cleanup instead of skipping Rust Drop handlers.

The updated launcher passed the Linux tmux/KVM regression: asymmetric initial
and resized dimensions, Unicode editing, multiline input, bracketed paste,
both exit routes, termios restoration, and alternate-screen restoration.
Windows host tests and Clippy pass. Windows live resize, clipboard paste,
physical Windows Terminal keystrokes, and exact console-mode equality remain
unverified. Physical UEFI hardware, real-provider use, responsive guest
cancellation, direct native HTTPS, command tools, and richer TUI work remain open.
