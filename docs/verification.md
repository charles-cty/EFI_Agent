# Verification record

## 2026-10-05: One environment configuration for VM and native packages

VM launch and native packaging use the same EFI_AGENT_* variables and host
configuration parser. The package command now takes only the EFI file and
output directory. Build scripts no longer accept --config / -Config. They
generate EFI/AGENT/CONFIG.JSON in the native boot tree and image. The native
workspace is \work. EFI_AGENT_CA_CERTIFICATE names a host DER file; both modes
copy it to EFI/AGENT/CA.DER and generate its firmware path. Packaging without
that variable removes a previously generated private CA.

Verified on Windows and a Linux-local checkout:

- Both Release build scripts complete without a user config file.
- Launcher Clippy with warnings denied and all 11 launcher tests pass.
- Actual packaging preserves quoted/backslash API keys and Unicode model
  names, Responses format, effort, custom DNS address/port, and private CA bytes.
- Repeat packaging removes the old CA; VM packages contain no API config.
- Independent Linux mtools extraction matches the generated native config.
  Unset optional variables restore defaults. A missing API key rejects the
  package before its output tree is created.
- The full tmux launcher smoke test with the system QEMU 8.2.2 passes direct
  EFI boot, environment-injected private CA/config, HTTPS tools, exact saved
  host file bytes, editing/resize, and terminal restoration. A separate QEMU
  11.1.2 installation on PATH reports a 2011 firmware time and correctly fails
  the current test certificate's validity check. The independent clock oracle
  still passes; the QEMU/firmware time difference has not been diagnosed.

Generated packages use provider.example and test credentials. This entry
supersedes earlier instructions to supply a user-authored CONFIG.JSON.

## 2026-10-05: Direct provider access in UEFI; relay removed

This entry supersedes the relay architecture and relay-address build options
described in earlier entries. Both VM and native mode now execute provider
requests in firmware. The host relay, serve command, framed RPC transport,
heartbeat code, and relay-only disconnect test have been removed.

Implemented direct DNS A queries over TCP, HTTP/1.1 body framing, no_std Rustls
with RustCrypto, public trust roots, an optional private DER CA, firmware RNG,
and firmware-clock certificate validation. Certificate chain, hostname, and
expiry checks cannot be disabled. HTTP remains available for explicit local
testing. All read/write/edit tools use the boot-volume workspace. Shared no_std
Chat Completions/Responses parsing retains reasoning, opaque signatures, usage,
and stream limits. Each model request has a separate connection; cancellation
closes it and does not block the next prompt.

The VM launcher accepts Cargo's .efi directly, an ESP tree, or a disk image.
It creates a private writable FAT disk, injects API settings from environment
variables, copies in the host workspace, and saves changed files after QEMU
stops. A host conflict rejects the save; the disk is retained for recovery.
Save errors propagate on normal exits. Native CONFIG.JSON contains direct API
settings and credentials. Packaging scripts use --config / -Config and default
to Release for both programs. Final packages contain test-key/test-model with
provider.example, not usable third-party credentials.

Verified:

- Linux-local and Windows-local unit/integration tests: 40 core and 11 launcher
  tests pass. The desktop clipboard integration test stays ignored on Linux.
- Launcher all-target and UEFI-target Clippy with warnings denied on both
  platforms. Release firmware linking and both platform packaging scripts pass.
- QEMU/KVM boots with an independent local HTTPS provider and a temporary CA.
  Chat Completions and Responses each pass direct DNS, certificate-verified TLS,
  fragmented HTTP chunking/UTF-8/SSE/tool arguments, streamed text before
  completion, draft editing during waits, retained reasoning state, exact tool
  results, HTTP 402 detail, usage/cache ratio, truncated-tool rejection,
  cancellation, and a new request after cancellation.
- Actual FAT files contain the expected edited Unicode bytes, newly created
  file, and unchanged ambiguous-edit file; truncated tools create no file.
- Final HTTP and HTTPS smoke runs pass after handling TCP4 peer FIN both at
  receive submission and at token completion. A close-delimited HTTP error
  retains its complete provider message. TLS still rejects a TCP EOF without
  close_notify; DNS rejects EOF before its complete response.
- Untrusted CA, wrong hostname, and expired certificate are rejected in actual
  firmware before any HTTP request reaches the provider. Expired-certificate
  testing also passes after extracting the clock conversion into shared code.
- The clock oracle walks every calendar day from 1970 through 2101 independently
  of the production formula, checks both ends of each day, and covers leap
  centuries, positive/negative timezones, and pre-epoch rejection.
- Actual FAT save tests on Linux and Windows preserve unrelated host edits,
  save Unicode and newly created files, and reject conflicts before writing.
- Linux tmux/KVM launcher tests pass dimensions, resize, Unicode editing,
  multiline/bracketed paste, direct HTTPS tools, exact saved host file bytes,
  /exit, the two-press Ctrl+C gesture, termios and alternate-screen restoration.
  The same full test passes with Cargo's .efi as the boot input.
- Native SimpleText/QMP keyboard input under OVMF, without VM.TXT, drives five
  direct HTTPS model rounds and verifies actual local FAT file bytes.
- Windows WHPX/psmux ConPTY boots Cargo's .efi directly and passes Unicode
  editing, multiline clipboard paste, local /status, /exit and Ctrl+C recovery.
- Independent sgdisk and mtools checks verify both platforms' generated GPT
  images, exact EFI bytes, direct native config/workspace, VM-only marker, and
  absence of old NATIVE.JSON. README, AGENTS.md and UEFI safety documentation
  describe the direct architecture.

Limits: no third-party provider or physical motherboard/NIC/USB has been tested.
Windows ConPTY testing does not include model traffic; actual direct HTTPS
runtime tests ran on Linux KVM. DNS uses TCP and an explicit IPv4 DNS server.
Firmware RNG and wall-clock quality remain platform requirements. An
unspecified firmware timezone is treated as UTC. VM host files are updated only
at exit; concurrent filesystems are not isolated against changes during save.
Native packages and VM recovery disks contain credentials.

## 2026-10-05: Release and native boot packaging scripts

Both build scripts now default to Release for the UEFI application and host
launcher. The profile can be set to Debug. A relay IPv4 address is required;
the port defaults to 7420. Each script creates separate VM and native boot
trees and GPT/FAT32 images. The native tree contains NATIVE.JSON and a work
directory, excludes VM.TXT, and retains existing workspace files. Each image
is replaced only after successful packaging. CARGO_TARGET_DIR is supported.

Verified with actual Linux-local and Windows-local builds:

- Release and Debug builds and both image packages on each platform.
- Debug with address 010.023.004.005 and port 1, normalized to [10, 23, 4, 5].
- Release with port 65535, then default port 7420 on repeat packaging.
- Independent sgdisk, fsck.fat, and mtools checks of both platforms' Debug
  and Release images: GPT validity, FAT consistency, exact EFI bytes against
  the selected profile's binary, parsed JSON, workspace presence, and absence
  of the opposite mode's marker/configuration.
- Repeat packaging removes stale mode markers. A Linux native workspace file
  retains its exact Unicode bytes in the rebuilt disk image.
- Both scripts reject malformed IPv4, octet 256, zero and multicast addresses,
  port 0/65536, and unknown profiles before compilation.
- Bash and PowerShell syntax checks and git diff whitespace checks.

Windows Debug verification used a separate CARGO_TARGET_DIR to avoid replacing
the existing Debug launcher. Final Windows packages use Release with example
relay address 192.168.1.73 and default port 7420; users must rebuild with their
actual relay address. No VM boot, live provider, physical USB write, or
bare-metal hardware run was performed for this script update.

## 2026-10-05: Linux and bare-metal README instructions

Added Linux prerequisites, build commands, OVMF preparation, the KVM access
check, API environment setup, and the terminal VM launch command. Moved native
setup into a separate bare-metal section with USB boot-volume contents,
firmware network requirements, relay startup, UEFI shell launch, and checks
for model access and boot-volume file tools.

Checked commands and paths against the build script, launcher argument parsing,
and UEFI runtime configuration and file access. README Bash examples pass
`bash -n`; `git diff --check` passes. This change is documentation only. No
build, VM run, provider call, USB preparation, or physical hardware test was
performed for this update. Physical bare-metal support remains unverified.

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
installation directories inspected on that machine. This does not
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

## 2026-10-04: request cancellation and responsive network waits

The guest now polls terminal input during DHCP, TCP connect, and receive waits.
Esc cancels the active request; Up/Down scroll and VM dimensions update during
the wait. Other keys enter a bounded queue and run through the normal editor
after the operation ends. A terminal regression caught dropped keys when the
host had written a file but its RPC confirmation had not reached the guest.
Input buffering fixes that timing case. Ctrl+C retains its exit behavior.
Firmware Escape now maps to cancellation rather than application exit.

TCP4 cancellation retires the queued token before releasing packet storage.
A cancellation/completion race preserves successfully received bytes. The bridge
keeps partial response headers and bodies across cancellation, discards responses
with older request IDs, and uses one deadline for the full response exchange.
Transport faults reset the connection; a completed RPC error leaves its framed
stream intact. Linux testing exposed why closing and reopening guest TCP alone
is insufficient: QEMU guestfwd can keep its host channel and deliver an old
response after the next request starts.

The host runs up to four model calls independently and serializes complete
response frames through one writer lock. File operations retain arrival order.
A new prompt can complete before a cancelled provider call returns. Cancelling
cannot roll back a dispatched file operation or interrupt a synchronous firmware
file call. Once frame transmission starts, the guest finishes it to preserve
stream alignment; its 30-second send deadline still applies. Host provider calls
may continue until completion or the 120-second provider timeout.

Both KVM and WHPX `smoke_vm.py` passed a deliberately delayed model request,
Esc cancellation and resize within three seconds, and a fresh completion before
the delayed provider was released. They then passed the read/edit/write loop
and ambiguous-edit refusal. Late cancelled responses did not enter the UI.

The native SimpleText/QMP test passed two cancellation boundaries: two bytes of
a response header, then a header plus thirteen body bytes. The relay resumed
those obsolete frames only after the next request arrived. The guest discarded
them, retained the two cancelled user prompts without stale assistant replies,
and completed six local FAT tool rounds. The first fresh request arrived about
0.70 seconds after Escape, including automated typing. Cancellation and final
SimpleText screenshots were inspected; they show Ready and the expected tool
results. This is OVMF protocol execution, not physical-hardware verification.

A core check cancels a three-tool batch after its first operation, verifies that
the remaining IDs get unexecuted results, and checks the next prompt's history.
Another check distinguishes standalone Escape from fragmented cursor sequences.
Windows and Linux tests, host and UEFI Clippy, and release UEFI linking pass.
Linux tmux and Windows ConPTY launcher regressions pass, including editing,
multiline input, and terminal restoration. Linux also checks live resize and
bracketed paste. Physical hardware, real-provider use, direct native HTTPS,
command tools, richer TUI work, and the previously noted Windows input checks
remain open.

## 2026-10-04: GPT/FAT32 boot image packaging

The host launcher now has `pack <ESP-directory> <new-disk.img>`. It creates a
66 MiB disk with a protective MBR, primary and backup GPT tables, and a 64 MiB
FAT32 EFI System Partition at sector 2048. Build scripts publish
artifacts/efi-agent-vm.img after packaging succeeds. The VM launcher accepts
either a boot directory or a raw disk image, mounted read-only through
virtio-blk. Native testing uses the same pack command for a writable disk image
with NATIVE.JSON and a local workspace, without VM.TXT.

`smoke_pack.py` checks the disk with sgdisk, fsck.fat, and mtools. GPT checksums
and FAT consistency pass. Independent extraction matches the EFI executable,
VM marker, binary data, empty files, Chinese filenames (including a single
Chinese character), and files of 511/512/513 and 4095/4096/4097 bytes. It verifies
refusal of an existing output without changing its hash, symlinks, case
collisions, invalid/non-BMP names, excessive file data, and output inside the
source tree. Windows packaging also passed a Chinese filename and junction
refusal; independent Linux extraction checked that Windows-generated image.

The crates.io fatfs 0.3.6 version failed fsck due to malformed dot entries.
The current upstream 0.4.0 fixed those entries but panicked when a filename
began with a multibyte UTF-8 character. The repository therefore includes the
upstream library at revision 2aefc2a027ce94ed0671752814dac203f0450e11, with one
UTF-8 boundary fix, original licenses, and vendor/fatfs/UPSTREAM.md. The package
test covers that fix through independent extraction. Non-BMP filename characters
are rejected because this library accepts UCS-2 filename characters.

KVM boot from the generated GPT disk passed serial rendering, TCP4 host files,
responsive cancellation, simulated Chat Completions, and the file tool loop.
WHPX/ConPTY boot from the Windows-generated GPT disk passed Unicode editing,
multiline input, both exits, and terminal recovery. Native SimpleText boot from
the packed writable GPT disk passed two partial-frame cancellation boundaries,
local read/edit/write/list, traversal refusal, and independent persisted-file
checks. Windows and Linux tests, host Clippy, and UEFI release builds pass.
After installing the missing Linux UEFI target, the Linux build script created
the application and GPT image entirely with Linux tools in a Linux-local tree.

The pack command does not write physical media. USB-device boot, physical
firmware compatibility, Secure Boot signing, and physical NIC behavior remain
unverified. The previous real-provider, native HTTPS, and TUI gaps remain open.

## 2026-10-04: Reasoning configuration and streamed model replies

Chat Completions requests now send `stream: true` and `reasoning_effort`.
The default effort is `medium`. `EFI_AGENT_REASONING_EFFORT` selects none,
minimal, low, medium, high, or xhigh. Invalid values fail before the API call.
The provider must support the selected value; the host does not downgrade it.
The README documents the host environment, API root, and protocol limits.

The host reads bounded SSE events without a new dependency. It assembles text,
reasoning_content, and indexed tool calls, including fragmented arguments and
interleaved calls. Text progress frames reach both VM and native UEFI interfaces.
Successful assistant messages retain reasoning_content for later tool rounds.
Tools execute only after a valid finish reason and [DONE]. Partial text is
visible but does not enter history after an error or cancellation.

Linux KVM and Windows WHPX smoke tests passed with a local SSE provider. The
provider withheld completion until the guest displayed a Unicode text prefix.
Both tests checked the default medium value, seven-byte wire writes, fragmented
tool arguments, reasoning_content echoed on later rounds, correlated file
results, and actual file contents. A complete write argument followed by a
truncated stream did not create the target file. Esc cancellation, resize,
new requests before a cancelled provider completed, and stale stream refusal
also passed. Native SimpleText/OVMF testing passed fragmented progress frames,
partial-frame cancellation, local FAT tools, and persisted-file checks.

Linux and Windows each passed 22 tests (4 host and 18 core), host Clippy,
UEFI-target Clippy, and release UEFI builds. Parser tests cover one-byte reads,
interleaved calls, UTF-8, CRLF, usage-only events, truncated or failed replies,
limits, invalid tool indices, and failed progress writes. Agent tests confirm
that streamed text is not duplicated and failed replies do not enter history.

The provider tests are deterministic local simulations. Real service-provider
reasoning, credentials, and streaming behavior remain unverified. Provider
reasoning formats other than reasoning_content are not implemented.

## 2026-10-04: Configurable VM memory

The launcher defaults to 128 MiB for WHPX and KVM. A trailing
`--memory-mib <MiB>` option selects a positive u32 value. Zero, negative,
non-numeric, and overflowing values fail before QEMU starts. CLI help and the
README document the option. The standalone VM smoke test also uses 128 MiB.

Windows WHPX passed default-memory boot, ConPTY input, host files, editor,
multiline input, quit, and Ctrl+C restoration. At 128 MiB the SSE provider smoke
test passed cancellation, resize, live text, truncated-tool refusal, and the
complete file tool loop. A custom 192 MiB launch booted and its live QEMU command
line contained `-m 192`. Linux and Windows builds and host Clippy passed.
Two custom-memory ConPTY runs passed file-content checks but stopped at screen
assertions: captures omitted spaces in rendered answer text. The second run
passed the Unicode file screen check before failing the multiline screen check.
The full custom-memory ConPTY run remains unverified; no launcher memory error
was observed. No claim about peak memory use or smaller allocations is made.

## 2026-10-04: UEFI event, watchdog, and capability safety

The entry point disables the boot watchdog before configuration or UI work.
Unsupported watchdog service is accepted; other errors stop startup. EFI TCP4
callbacks now only mark completion and enqueue a preallocated intrusive node.
The single application-processor dispatcher drains notifications at application
TPL. It rejects reentry, waits only on plain timer events, and owns the sole
WaitForEvent call site. Input loops, DHCP waits, and token waits yield through
that dispatcher. Completion events are closed and queued nodes are drained
before their storage is freed. Pending tokens remain live through cancellation.
The application does not start additional processors or firmware threads.

IPv4 Config2 is optional for firmware with an existing TCP4 address. When
available, address setup precedes TCP child creation. OVMF retained an unresolved
mapping when TCP children were configured before DHCP; preparing the interface
first corrected that behavior. Address mapping has a deadline and child Poll
calls. Missing protocols produce explicit diagnostic reports rather than
preventing local UI use.

Startup and /caps probe TCP4, IPv4 configuration, and EFI RNG protocols.
NOT_FOUND is reported as an absent capability. RNG probing uses bounded
algorithm enumeration and explicit SP800-90 DRBG requests. Raw entropy and
unknown defaults do not qualify as an application cryptographic DRBG. The
probe never prints sample bytes or substitutes a weak generator. These are
capability checks, not entropy certification or an SSH implementation.
docs/uefi-safety.md defines the execution, callback, lifetime, watchdog,
protocol, and future SSH randomness rules.

Linux KVM and Windows WHPX passed actual TCP4 host reads, streamed model replies,
resize, cancellation, obsolete response refusal, truncated-tool refusal, and
the complete file tool loop at 128 MiB. Native SimpleText/OVMF passed partial
header/body cancellation, local FAT read/edit/write/list, traversal rejection,
and persisted-file checks. Linux no-NIC boot reported zero network interfaces
and no EFI RNG protocol. After 310 seconds the guest still responded to /help
and exited through /quit. Windows with a virtio RNG device booted without a NIC
and reported no usable recognized DRBG from that OVMF image. A device's presence
alone therefore did not qualify cryptographic capability.

Linux and Windows passed 23 tests, host Clippy, UEFI Clippy, and release builds.
Structured randomized completion-queue tests cover batches of 1, 2, 31, 64, and
257 nodes, duplicate callback notifications, delayed dispatch, and repeated
draining against an independent readiness oracle. Physical firmware/NIC behavior
and an RNG image with a successful recognized DRBG request remain unverified.

## 2026-10-04: Session commands and API configuration checks

`/exit` and `/quit` use the same exit path. Session commands are /help, /clear,
/caps, /effort, /status, /quit, and /exit. File slash commands have been removed. The
agent uses read/write/edit tools against one configured workspace; VM host
routing and native FAT routing are implementation details. The unused list RPC
operation and separate host-command entry point were removed. read still lists
directories when its path refers to a directory.

The launcher and relay reject missing or blank API base, key, and model before
startup. URL scheme/host and reasoning effort are checked as well. Before a
prompt enters model history, the guest sends a model_config preflight request.
Failed preflight reports an error without entering model-wait state. Each model
call also validates configuration before opening the HTTP request. Native mode
still needs a reachable relay; connecting to a relay is transport work, not an
API inference request.

Linux CLI checks refused missing configuration, a blank key, and a file URL in
less than one second before accessing the workspace or starting QEMU. Windows
also refused missing configuration before QEMU boot. KVM and WHPX SSE smoke
tests refused the removed file commands and passed cancellation, streamed text,
and the actual file tool loop. Native OVMF passed model configuration preflight,
partial-frame cancellation, and local FAT tools. Linux tmux and Windows ConPTY
passed Unicode prompt editing, multiline input, /exit, and Ctrl+C restoration;
Linux also passed resize and bracketed paste with an embedded /exit line.
Terminal checks use test configuration and unknown slash commands to exercise
the editor without issuing API requests. File behavior is verified through the
agent tool-loop tests. Builds, Clippy, and the existing 23 tests passed.

## 2026-10-04: Idle connection keepalive and recovery

HostBridge no longer applies its 180-second read timeout to an idle connection.
It waits for the first byte without a timeout, then bounds the rest of the frame.
The guest sends a ping after 30 seconds without an RPC request, once its first
connection has been established. Ping replies and model configuration checks
have five-second receive deadlines. Connection and transmit deadlines remain
separate. Idle polling retains editor input, cancellation, and resize handling.
Only ping is automatically retried; files and model calls are never replayed.
Failed heartbeats discard the transport and partial framing, reconnect, and retry
once; further recovery runs on subsequent intervals. A successful heartbeat
restores the connection error status to Ready.

QEMU guestfwd uses an explicit socket chardev with reconnect-ms=1000. Its prior
implicit TCP chardev did not reconnect when the host service restarted, even
when the firmware created a new TCP connection. TCP child retirement also sends
an abortive Close token before local reset and destruction, with the same token
lifetime and event-dispatch rules as other asynchronous operations.

Windows QEMU 11.1.0/WHPX passed an idle interval of 190 seconds followed by a new
streamed model reply. Restarting HostBridge while the guest was idle also passed
heartbeat recovery and a subsequent reply without rebooting QEMU. ConPTY checks
passed Unicode/multiline editing, /exit, and Ctrl+C restoration. Native OVMF
passed cancellation and local file tools. Linux and Windows passed 23 tests,
including socket ping and fragmented frame checks, builds, and Clippy.

The installed Linux QEMU 7.2 rejects reconnect-ms. It passed the firmware
heartbeat and file/tool regression before the chardev change, but the final
launcher requires a current QEMU with that option. No deprecated reconnect
option or compatibility fallback was added. The README states this requirement.

## 2026-10-04: Effort, caps, and status commands

Windows native builds and all 24 core/launcher tests passed. Clippy passed with
warnings denied for core/launcher (all targets) and x86_64-unknown-uefi.
QEMU/WHPX with OVMF passed the updated scripts/smoke_vm.py checks. The guest
accepted /caps, rejected the removed /capabilities command, and sent a new
effort value (future-budget) to the local HTTP provider across tool rounds.
Slash commands did not enter model history.

The initial /status showed zero API attempts and unavailable token/cache data.
The final status matched seven real SSE usage reports from the test provider:
731 input and 419 output tokens per report, with totals of 5117 input, 2933
output, and 8050 combined tokens. Each report had 73 cached input tokens and
19 reasoning tokens. The displayed cache ratio was 9.99%. Nine API attempts
included one HTTP quota error and one truncated stream without usage reports.
/clear reduced history to the system message while preserving effort and usage
totals. The boundary test distinguishes a reported zero cache count from a
missing field, retains per-field report counts, and removes stale last-request
usage when a later request supplies no report.

The test provider supplied deterministic usage values. These checks verify
transport, accounting, and display; they do not verify a third-party provider's
effort support or billing. Exact current history tokens and the model's context
window remain unavailable and are labelled as such. Native hardware was not
retested for these commands.

## 2026-10-04: Reasoning display and Responses API

`EFI_AGENT_API_FORMAT` defaults to chat_completions and accepts responses as an
explicit alternative. Responses requests use reasoning.summary=auto, store=false,
and include reasoning.encrypted_content. The adapter retains completed output
items for subsequent requests, maps function call IDs to tool results, and
normalizes only supplied usage fields. Failed, incomplete, and truncated streams
do not authorize tool execution. The common SSE reader retains bounded event
and wire sizes.

Windows core/launcher tests passed (29 tests), and all-target Clippy plus UEFI
target Clippy passed with warnings denied. Windows launcher, UEFI application,
and boot-image builds passed. QEMU/WHPX checked both API formats with a local
HTTP provider: model text streaming, tool correlation, quota and truncated-stream
errors, reasoning state retention, effort forwarding, and actual reported usage.
The guest showed reasoning text for Chat Completions and summaries for Responses.
Reasoning and tool panels started collapsed. SGR mouse presses expanded and
collapsed reasoning and failed tool results; release events did not toggle them.
The UI test covers wrapped headers, nonzero viewport origins, scrolling, resize,
keyboard toggling, and clearing old hit targets. Decoder tests include malformed
mouse coordinates, motion events, mouse release, wheel events, and paste mode.

Windows ConPTY checks passed Unicode/multiline input, /exit shell restoration,
and Ctrl+C restoration. The launcher disables terminal mouse modes during cleanup.
Native mode uses optional absolute/relative firmware pointer protocols and a
visible pointer cell; Tab and Enter remain available without pointer hardware.
Physical pointer devices and a live external Responses provider were not tested.
The HTTP provider supplies deterministic text and usage, so these checks do not
establish model availability, provider effort support, or billing accuracy.

## 2026-10-05: Preserve reasoning state through stream and tool rounds

Chat Completions now retains extension fields on tool calls and function
objects through streaming, bridge serialization, agent history, and the next
request. Nested opaque signatures keep their JSON values; conflicting scalar
metadata reports an error instead of concatenating state. Reasoning detail
nulls remain null. Text fragments still concatenate, and a later null delta
does not erase existing text.

Responses collects reasoning output-item snapshots and indexed text parts.
Completed output remains authoritative, with absent reasoning items restored
by output index and absent fields filled from stream snapshots. Item IDs prevent
duplicate replay. Unindexed text can fill a unique reasoning item; unresolved
identity reports an error before tools run. Failed and truncated streams still
do not enter model history or execute tools.

Windows passed all 35 core/launcher tests, all-target Clippy and UEFI-target
Clippy with warnings denied, launcher/UEFI builds, and boot-image packaging.
The independent sequence oracle covers all eight subsets of three omitted
reasoning items and both forward/reverse snapshot arrival orders. Tests also
cover RPC round trips, nested signatures, nulls, text fragmentation, final
snapshot precedence, ambiguous identity, and no duplicate replay.

QEMU/WHPX passed both API modes using the local HTTP provider. Chat Completions
tool rounds required exact signature metadata and null reasoning detail fields
in the next HTTP request. Responses tool rounds deliberately omitted reasoning
from final output; the next HTTP request still contained the stream item's
encrypted content, null signature, nested extension data, and correct ordering.
Existing cancellation, quota/truncation errors, file changes, folded panels,
mouse toggles, effort, and usage checks also passed. These are local protocol
checks; no live third-party API or physical firmware call was made.

## 2026-10-05: HostBridge disconnect diagnostics

Typed connection resets, aborted connections, broken pipes, and disconnected
sockets now retire a bridge connection without reporting a service error.
This also covers asynchronous model reply writes after the peer leaves.
Timeouts, partial-frame graceful EOF, and malformed frames remain errors.
VM diagnostics wait until terminal restoration; standalone `serve` reports
diagnostics immediately.

Windows passed 36 core/launcher tests, all-target Clippy with warnings denied,
and launcher/UEFI/image builds. `smoke_disconnect.py`, run with Windows uv,
forced real TCP resets before a frame, inside its header, and inside its body.
Each reset produced no diagnostic and a subsequent connection returned pong.
Graceful EOF inside a header and zero frame length each retained an error.

QEMU/WHPX and Windows ConPTY passed `/status` followed by `/exit` and Ctrl+C,
shell and alternate-screen restoration, and QEMU process cleanup. `/exit`
stderr contained QEMU platform warnings but no launcher/HostBridge error.
Ctrl+C used native console handles and the restored screen had no bridge error.
PowerShell stderr redirection changes native pipeline cancellation, so that
Ctrl+C check uses pane capture instead. Capture markers distinguish executed
sentinel output from echoed commands and allow omitted status spaces.

## 2026-10-05: VM text selection and clipboard

The launcher mirrors guest ANSI output in a terminal cell grid and emits
complete screen differences. This lets selection overlays coexist with ANSI
commands and Unicode split across TCP reads. Left-button dragging highlights
visible cells; a click without dragging still reaches guest disclosures.
Selections copy to the host system clipboard with Ctrl+C, Ctrl+Shift+C, or
right-click. Ctrl+C without a selection still exits. Ctrl+V, Ctrl+Shift+V,
and right-click without a selection paste through the sanitized bracketed
paste path. Esc clears selection. Cell changes and real resizes retire it;
idle hide-cursor commands and duplicate resize events do not.

Windows passed 40 core/launcher tests, all-target Clippy with warnings denied,
and launcher/UEFI/image builds. Selection checks cover reverse ranges, both
halves of wide Unicode glyphs, combining marks, multiple rows, idle controls,
and cell changes. Every chunk size of an ANSI/Unicode fixture reconstructs
the expected visible text and cursor mode without partial terminal commands.

`smoke_clipboard.py` used QEMU/WHPX, Windows ConPTY, native mouse INPUT_RECORDs,
and the real system clipboard. It verified drag highlighting, exact copied
text, Ctrl+C copy without exiting, right-click copy, Unicode multiline Ctrl+V
paste without submission, right-click paste, explicit submission, and exit.
`smoke_windows.ps1` also passed actual clipboard paste, Unicode editing,
`/status`, `/exit`, Ctrl+C cleanup, and alternate-screen restoration. Tests
restore the original clipboard text. Physical mouse hardware and clipboard
shortcuts intercepted by a particular terminal host were not tested.

## 2026-10-05: Stable transcript selection and edge scrolling

Guest frames now end with bounded selection metadata: transcript revision,
frame dimensions, viewport offset, and selectable message-body ranges.
Headers, disclosure controls, welcome text, prompt chrome, status text, and
padding are excluded. Body indentation and empty lines are retained. The
host commits complete frames, composes the highlight before emitting a diff,
and updates only changed cells. It no longer restores the full unselected
screen before each drag update. Metadata and ANSI can span arbitrary TCP reads.

Selection anchors refer to logical transcript rows. Holding the left button
at either viewport edge repeats scroll requests every 90 ms, including when
the pointer stops moving. Visited rows stay available for clipboard copying.
Mouse release stops repeats. Scrolling retains selection; transcript changes
or resizing reset it. Dragging over a control does not toggle its disclosure.
Committed frame dimensions handle older frames arriving after a host resize.

Windows passed 44 core/launcher tests and launcher/core plus UEFI-target Clippy
with warnings denied. Byte-by-byte overlay checks show that expansion never
removes the existing highlight and emits no full-screen erase. Tests also
cover fragmented frame markers, Unicode continuation cells, reverse ranges,
control exclusion, code indentation, empty body lines, cached offscreen text,
both scroll limits, release, transcript changes, and in-flight resize frames.

Real QEMU/WHPX and ConPTY tests passed clipboard copy/paste and stationary
dragging at both edges. Each direction copied all 90 numbered Chinese text
rows across multiple viewports without losing rows or including UI chrome.
The psmux smoke passed Unicode editing, multiline clipboard paste, `/status`,
`/exit`, Ctrl+C, shell restoration, and process cleanup. The local-provider VM
smoke passed Chat Completions streaming, tool rounds, cancellation, quota and
truncation errors, disclosure toggles, effort forwarding, and usage reports.

An existing user VM held `target/debug/efi-agent.exe` open. It was left running;
the verified launcher is `target/x86_64-pc-windows-msvc/debug/efi-agent.exe`.
The ESP firmware and `artifacts/efi-agent-vm.img` were rebuilt with the matching
guest protocol. Run the standard build after the old process exits to replace
the default launcher path. Physical mouse hardware was not tested.

## 2026-10-05: Selection anchors in blank transcript space

Body-row indentation, trailing blank cells, and empty transcript rows can
start a selection. Anchor columns retain their actual coordinates instead of
snapping to the first or last glyph. Copying and highlighting still intersect
only message-body text ranges. Non-body text and areas outside the transcript
remain controls. Drag endpoints use transcript coordinates even on blank rows.

Windows passed 45 core/launcher tests and all-target Clippy with warnings denied.
The real ConPTY clipboard smoke copied exact text after starting from both the
left margin and right padding, and passed existing copy/paste, stationary top
and bottom autoscroll, 90-row Unicode selection, and exit checks. The default
launcher path was rebuilt after the previous user process released its file.

## 2026-10-05: One prompt marker for multiline input

Only the first logical input line starts with `› `. Continuation lines contain
the input text directly. The cursor-height calculation uses the same prefix
rule, including when the editor cursor is before a later newline.
Windows Clippy and launcher/UEFI/image builds passed. The ConPTY clipboard
smoke asserted that multiline pasted input has one prompt marker and an
unprefixed second line, then passed copy/paste, blank selection anchors,
stationary autoscroll in both directions, cross-screen copying, and exit.

## 2026-10-05: Select visible input text

Guest frame metadata includes visible input ranges, input revision, and the
editor cursor cell. Input and transcript selections have separate coordinate
domains. Input selection accepts blank starting cells, excludes the prompt
marker, cursor and borders, and remains within the input when dragged outside
it. It does not scroll the transcript. Input edits or cursor movement clear
the input selection; unrelated transcript changes preserve it.

Windows passed 46 core/launcher tests, launcher/core and UEFI-target Clippy with
warnings denied, and launcher/UEFI/image builds. The ConPTY smoke selected two
lines of actual Chinese input from blank cells, copied the exact text without
the marker or cursor, and verified that copy neither submitted nor exited.
Existing blank anchors, clipboard paste, 90-row Unicode selection, stationary
edge scrolling in both directions, and exit checks also passed.

## 2026-10-05: Ctrl+C copy and confirmed VM exit

Ctrl+C copies the current selection and Ctrl+V pastes the host clipboard.
With no selection, the first Ctrl+C shows an exit hint; a second press within
one second exits. The exact one-second boundary is included; later presses
start a new confirmation. Copy and other keyboard input disarm confirmation.
Keyboard repeat events do not count as a second press. The host consumes the
gesture instead of forwarding Ctrl+C as a guest quit byte. Console signals
use the same guard, including during VM boot. An opposite-source delivery
within 100 ms is treated as the same physical press.

Windows passed 47 core/launcher tests, all-target Clippy with warnings denied,
and both launcher output builds. The ConPTY smoke passed exact input/transcript
copy, Ctrl+V multiline paste, autoscroll, single-press survival, confirmation
expiry, and two-press exit. Native psmux console Ctrl+C also displayed the hint
and exited on the second press, with shell restoration and QEMU cleanup.

## 2026-10-05: Terminal-handled Windows paste

The previous Ctrl+V smoke sent the shortcut directly to the application. It
missed terminal hosts that consume the shortcut and inject ordinary character
and Enter records, causing multiline prompts to submit each line. The launcher
now holds matching multiline clipboard input before forwarding newlines and
uses the bracketed-paste path when the match completes. A mismatch or 150 ms
idle restores ordinary typing; if a newline already matched, buffered text
remains paste text even after a mismatch or delay. Detection is limited to the
current system clipboard and 64 KiB, matching the prompt limit.

The ConPTY regression reproduces the old behavior by injecting plain key
records, including Chinese, multiple CR newlines, and a trailing newline. The
fixed launcher retains all text in the editor until a separate Enter arrives.
Existing Ctrl+V, selection copy, right-click, autoscroll and double-interrupt
checks pass. Core/launcher tests (48) and all-target Clippy pass. An active user
VM locks the default exe; the updated tested launcher is built under
`target/x86_64-pc-windows-msvc/debug/efi-agent.exe` without stopping that VM.

## 2026-10-05: Edit drafts during model output

The guest previously queued editor input during provider requests and applied
it only after the request ended. Request input now goes directly to the editor;
only Enter submissions are deferred. Empty-editor Enter still toggles the
focused disclosure panel. Escape still cancels the active request.

UEFI-target Clippy with warnings denied and the UEFI build passed. The firmware
and VM images were updated. Both Chat Completions and Responses QEMU smoke
tests paused a local provider after its first streamed text, pasted a multiline
Unicode draft, and verified the visible draft and Backspace result before
allowing the response to finish. The draft was then deleted without submission
or cancellation. Both full smoke tests passed their existing protocol, tool,
usage, cancellation, and disclosure checks.

## 2026-10-05: Up/Down draft navigation

Keyboard arrows previously scrolled the transcript instead of moving the editor
cursor. With draft text present, Up/Down now move between newline-separated
lines. Navigation retains the display column across shorter and empty lines,
uses grapheme boundaries, and resets the target column after horizontal moves
or edits. Empty-editor arrows still scroll the transcript. Mouse wheel events
have separate scroll keys so they scroll output with a draft present.

Core and launcher tests passed (50 total), including wide-character boundaries,
combining characters, short and empty lines, first/last line boundaries, and
12,000 structured random edits with vertical moves. Launcher all-target and
UEFI-target Clippy passed with warnings denied. The UEFI build and VM images
were updated. Both Chat Completions and Responses QEMU smoke tests moved up
from a pasted draft, edited the first line, moved down, and edited the second
line while the provider was paused mid-response. Both full smoke tests passed.

## 2026-10-05: Keep the Linux clipboard owner alive

Copy previously created a temporary arboard handle and dropped it immediately
after writing text. On X11, this discarded the selection owner and could print
an arboard warning directly into the active terminal. Copy, paste, and Windows
injected-paste detection now share a lazily initialized clipboard handle owned
by the VM session. The handle is released after terminal restoration.

Linux and Windows launcher tests and all-target Clippy passed. An explicit
Linux/X11 integration test wrote multiline Unicode text, returned from the
clipboard helper, and launched a separate process that read the exact text
from the still-running owner. The test passed using the WSLg X11 display.
The Windows launcher build also passed. The Linux regression is ignored by
default because it needs an X11 display and temporarily changes the clipboard:

```sh
cargo test -p efi-agent -- --exact \
  vm::tests::clipboard_contents_survive_copy_until_another_process_reads --ignored
```

## 2026-10-05: XWayland and native Wayland clipboard paths

Enabled arboard's Wayland data-control feature. A Linux clipboard wrapper keeps
the desktop owner alive and uses wl-clipboard when desktop initialization fails
in a Wayland session. This covers compositors without data-control even when
DISPLAY is absent. The helper write uses UTF-8 text and the read requests text
without an added newline. Helper diagnostics cannot write into the active TUI.
The wl-copy daemon owns the copied selection after its parent exits; waiting
for a captured daemon pipe to close would block, so the launcher waits only for
the initialization process.

WSLg clipboard integration passed with WAYLAND_DISPLAY removed (XWayland),
DISPLAY removed (native Wayland via wl-clipboard), and both variables present
(automatic selection). Each case checked exact multiline Unicode text through
a separate launcher test process and the independent wl-paste utility. XWayland
to Wayland clipboard bridging passed. WSLg does not expose the data-control
protocol required by arboard, so the native arboard path itself was not exercised
on a data-control compositor. Standalone Xorg was not available in this session;
the X11 protocol path was tested through XWayland. Linux and Windows launcher
tests and all-target Clippy passed, and the Windows launcher build passed.

## 2026-10-05: Combine tool calls and results in one disclosure

Each tool start now creates one collapsed panel containing its arguments. The
corresponding finish updates that same panel with the result and completion
status, preserving its expanded state. Arguments and Result labels distinguish
the two sections when expanded. Running, successful, and failed executions use
yellow, green, and red headers. Separate calls with the same tool name retain
their own panels.

All 26 core tests, UEFI-target Clippy with warnings denied, and the UEFI build
passed. The regression verifies completion while expanded, both sections hidden
when collapsed, and two calls with the same name and distinct results. Both
Chat Completions and Responses QEMU smoke tests expanded the failed edit panel,
asserted that its actual arguments and error appeared together, then collapsed
it and checked that both disappeared. Both full smoke tests passed. The firmware
and VM images were updated.
## 2026-10-05: User-reported physical firmware protocol failure

A user booted the application on physical UEFI firmware. A normal prompt
failed with `TCP4 service binding: UEFI Error NOT_FOUND: ()`. The capability
probe reported zero TCP4 interfaces, zero IPv4 configuration interfaces, and
no firmware RNG protocol. The board, NIC, firmware settings, and boot method
have not yet been identified. This is a reported failure, not a passing
bare-metal verification.

Source inspection confirms that the application locates existing TCP4 service
bindings; it does not call ConnectController to connect network drivers.
DNS uses TCP4 too, so this failure can occur before any provider connection.
TLS uses the firmware RNG protocol and has no alternative random source.
Missing network protocols and missing RNG are separate blockers.

On firmware with a UEFI Shell, run `connect -r`, then launch the application
again and repeat `/caps`. A positive TCP4 count after this step distinguishes
unconnected installed drivers from the original state. If the count remains
zero, inspect UEFI network-stack settings and NIC driver support. Enabling a
network stack does not guarantee that the firmware supplies a driver for the
attached NIC, especially for USB and wireless adapters. This check has not yet
been performed on the reported machine. Missing RNG needs a separate usable
cryptographic source; changing DNS, API credentials, or certificates cannot
supply the missing protocols.

## 2026-10-05: Bundle UEFI Shell and connect installed drivers

Native packaging now boots EDK II UEFI Shell 2.2 from pbatard/UEFI-Shell 26H1.
The fixed x64 binary, upstream release URL, SHA-256, and BSD-2-Clause-Patent
license are in vendor/uefi-shell. The release API digest matched the download.
Both package trees carry the Shell license and source record. Native startup
runs connect -r, selects the common 80x25 text mode, then starts AGENT.EFI using
homefilesystem. Esc skips startup; /exit returns to the resident Shell.
The VM boot entry remains Agent. The launcher also selects AGENT.EFI when
preparing a private disk from a generated native package.

Agent startup recursively connects installed controller drivers before opening
long-lived protocols. Missing TCP4 reports network-stack/NIC guidance instead
of the raw NOT_FOUND error. /caps reports Shell protocol presence. These changes
cannot supply absent network drivers, TCP4 stacks, or cryptographic RNG.
Model-driven Shell commands are not implemented. The Execute interface and
output, working-directory, permissions, status, and cancellation constraints
are recorded in docs/shell-execution.md.

Actual Shell boot exposed console defects that direct boot did not reproduce:
file-backed Shell stdout does not implement cursor-addressed drawing; empty
stderr sinks can advertise a mode; the Shell logger can retain dimensions that
do not match all underlying console sinks; cursor hiding can return UNSUPPORTED.
Native console selection now checks actual cursor movement at the far edge,
prefers firmware console splitters, and uses keyboard protocols with wait
events. UEFI's optional cursor visibility does not stop the interface. The
startup script's common 80x25 mode lets the display and serial sinks agree.
System-table console pointers are not changed.

Windows-local Release builds passed for UEFI and launcher. Host tests passed
(40 core, 11 launcher), and host all-target plus UEFI Release Clippy passed with
warnings denied. Windows UEFI Debug linking failed in the existing poly1305
and polyval dependencies with an LLVM "Do not know how to split the result of
this operator" error. Release linking passed; no workaround was added.

scripts/smoke_shell.py passed under actual Windows QEMU/OVMF with TCG in three
cases: one FAT volume, Agent on fs1: with a decoy fs0:, and no NIC or RNG device.
Each case booted packaged Shell into Agent, sent /caps through QMP keyboard
input, checked display pixels for the Agent interface, returned with /exit,
executed a Shell echo command, and checked its UTF-16 output in the real FAT
image with an independent parser. The no-device case reported TCP4 zero and
missing RNG while the Shell remained usable. Display screenshots were also
inspected. Results are in artifacts/shell-smoke.

The Windows WHPX direct HTTPS Chat Completions smoke passed streamed text,
draft editing, tools, usage, truncation, cancellation, and the next request.
Actual FAT bytes and absence of the truncated tool's output passed. The test
now uses dissect.fat instead of the Linux-only mtype dependency and writes its
screen record as UTF-8. Results are in artifacts/shell-direct-smoke.

Updated artifacts/esp, artifacts/native-esp, and both disk images. Native
packaging reused the existing generated provider config without exposing its
key and retained the native workspace. Physical firmware has not yet been
retested; the user's missing network and RNG protocols remain unresolved until
the new package is tested there.

Run scripts/smoke_shell.py with --launcher, --efi, --code, --vars, --qemu, and
--output paths. The script runs native Windows or Linux tools and needs no
provider credentials. Windows paths must remain local to Windows.

## 2026-10-05: Standard driver activation and extra driver loading

Reviewed UEFI 2.11 chapters 3, 7, 11, 24, 28, and 35. The official HTML
endpoints returned HTTP 403 both directly and through the configured proxy;
their text was read through r.jina.ai. Sources and section links are recorded
in docs/firmware-drivers.md. EDK II DriverSupport.c and protocol definitions
were also checked against the standard. There is no portable switch to enable
a missing firmware network stack. ConnectController selects loaded driver
handles; GetDriverPath describes images that a separate component must load.
DriverOrder/Driver#### are boot-manager variables, not a runtime component
enable API. SNP Start/Initialize initialize an existing packet interface; they
do not provide TCP/IP. Private setup variables and PI DXE services are not used.

Added optional EFI/AGENT/DRIVERS.JSON startup loading through LoadImage and
StartImage. The manifest specifies ordered x64 PE32+ boot-service/runtime
drivers on Agent's boot volume. Applications and other architectures are
rejected before executing their entry points. Paths are restricted to
EFI/AGENT/DRIVERS. Bounds are 64 KiB for the manifest, 32 paths, 8 MiB per
image, and 32 MiB total. Firmware retains image verification/signing policy.
Successful driver images stay resident. Matching full loaded-image paths are
skipped when Agent is restarted. A list error stops later drivers; existing
drivers are still connected and the UI reports the error through /caps.

Recursive ConnectController passes now continue until handle and Driver
Binding sets are stable. Connection errors are retained in /caps along with
driver loading results. Added SNP, MNP service binding, IP4 service binding,
and Driver Binding counts. Protocol discovery uses the standard GUIDs.
Packaging creates the optional driver directory and retains user-supplied
driver files. No NIC/network/RNG driver binary is bundled by this change.

Windows-local UEFI and host Release builds, 42 core tests and 11 launcher
tests, host all-target Clippy and UEFI Release Clippy passed. Parser checks cover
EFI driver versus application subsystems, every truncated length of a valid
header, large PE offsets, cross-volume paths, traversal, and case aliases.

Built scripts/fixtures/driver-probe as an actual EFI boot-service driver with
the linker subsystem set to efi_boot_service_driver. It installs a resident
Driver Binding protocol and writes an entry-point counter on its own boot
volume. Initial QEMU loading found an ACCESS_DENIED error because Agent kept
its filesystem protocol open during StartImage. The loader now releases the
filesystem and device-path references before starting drivers. The successful
fixture confirms its LoadedImage boot-device association and filesystem access.

Seven Windows QEMU/OVMF TCG cases passed in artifacts/driver-smoke: one volume,
Agent on fs1:, no NIC/RNG, driver startup and restart, application rejection,
manifest path rejection, and a missing driver. Successful startup adds exactly
one Driver Binding instance. A second Agent invocation reports the driver as
resident, adds no new binding, and leaves the FAT counter at one execution.
All cases retain the displayed interface, capability reporting, and working
Shell after Agent returns. Independent FAT parsing checks actual bytes.

An extra ordered-list test in artifacts/driver-stop-smoke put an application
first and a valid fixture driver second. The loader rejected the application;
the later driver's FAT marker did not exist. This confirms stop-on-error rather
than silently executing later entries.

The Windows WHPX direct-boot HTTPS Chat Completions smoke also loaded the
fixture without a resident Shell. It passed DNS, verified TLS, streamed text,
draft edits, tools, token usage, truncation, cancellation, and the next prompt.
Actual FAT file bytes and the driver counter passed. Results are in
artifacts/driver-direct-smoke. This confirms that activation is in Agent startup,
not dependent on Shell startup.nsh. The smoke accepts --driver for this case.

Updated both ESP trees and images with the verified Release firmware, retaining
existing provider configuration and native workspace. Real motherboard driver
availability, network-stack activation, and RNG support still need physical
retesting. Secure Boot rejection and real third-party driver compatibility were
not exercised by these unsigned OVMF fixture tests.

Build the fixture on Windows with:

```powershell
cargo.exe rustc --manifest-path scripts/fixtures/driver-probe/Cargo.toml --target x86_64-unknown-uefi --target-dir artifacts/driver-probe-build --release -- -C link-arg=/subsystem:efi_boot_service_driver
```

Then supply its generated .efi with --driver to scripts/smoke_shell.py. Select
--case reject-application to check stop-on-error, or omit --case for all cases.
