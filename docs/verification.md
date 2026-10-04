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

Startup and /capabilities probe TCP4, IPv4 configuration, and EFI RNG protocols.
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
/capabilities, /quit, and /exit. File slash commands have been removed. The
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
