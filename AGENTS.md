# Rules

You are a senior software engineer working on this project, EFI Agent.

Do not edit README.md for your memory. Edit [the Current state section here](#current-state)
instead. Keep README.md concise and clean.

Maintain this document `./docs/verification.md` by yourself, too.

# Current state

## Architecture

The UEFI application owns direct model HTTP/HTTPS requests, DNS A queries over
TCP, SSE parsing for Chat Completions and Responses, reasoning state, usage
statistics, cancellation, and the read/write/edit tool loop. There is no relay,
HostBridge, RPC model service, or `serve` command. All file tools use the boot
volume's configured workspace through EFI_SIMPLE_FILE_SYSTEM_PROTOCOL.

The firmware reads EFI/AGENT/CONFIG.JSON. Fields: api_base, api_key, model,
api_format (chat_completions by default), reasoning_effort (medium), dns_address
([1,1,1,1]), dns_port (53), optional static ipv4 address/mask/gateway,
workspace (\work), and optional ca_certificate
(absolute UEFI path to an additional DER CA). HTTPS uses no_std Rustls with
RustCrypto, bundled public CA roots, RDRAND-seeded ChaCha20, and firmware wall
time. A session generator takes one 32-byte hardware seed on first use and
does not reseed. Missing RDRAND or failed seed collection rejects TLS. EFI RNG
is not used. When
static ipv4 is configured, TCP4 children use that address and add the default
route without requiring IPv4 Config2.
Certificate-chain, hostname and expiry checks are mandatory. No format retry,
reduced reasoning retry, or automatic model replay is performed.

The host launcher starts QEMU and relays terminal input/output only. It accepts
a compiled .efi file, an ESP tree, or a GPT/FAT32 image. It creates a private
writable disk, copies the host workspace into \work, and injects provider
configuration from EFI_AGENT_API_BASE, EFI_AGENT_API_KEY, EFI_AGENT_MODEL and
optional API format/effort/DNS/private-CA environment variables. No external
service is required. It saves changed workspace files after QEMU stops; host
files remain unchanged during execution. Conflicting concurrent host edits
reject the save and retain the private disk for recovery. The launcher reports
save failures and returns an error. Guest tools currently cannot delete files.
Boot/workspace packaging is bounded to 48 MiB, 4096 entries, 32 directory levels,
and FAT-supported names; links and special files are rejected.

VM.TXT selects serial input/output and shutdown behavior only. Without the
marker, the app uses firmware SimpleText input/output and optional pointers;
/exit returns to its parent (Shell or firmware). Startup recursively connects
installed drivers before opening long-lived protocols. /caps also reports the
Shell protocol. Model Shell command tools are not implemented yet.
Startup optionally loads the ordered EFI/AGENT/DRIVERS.JSON list via standard
LoadImage/StartImage. Only x64 PE32+ boot-service/runtime drivers under
EFI/AGENT/DRIVERS are allowed (32 files, 8 MiB each, 32 MiB total). Filesystem
references are released before driver entry points. Matching full loaded-image
paths are skipped on restart. Recursive connection repeats until firmware
handle and Driver Binding sets are stable. /caps retains startup loading and
connection errors and adds SNP, MNP, IP4 and Driver Binding counts. No generic
BIOS network-stack switch exists; private setup variables are not changed.
See docs/firmware-drivers.md for UEFI 2.11 sources and driver setup.
Physical motherboard/NIC/USB support is unverified. A user-reported physical
boot has zero TCP4/IPv4 Config2 interfaces and no RNG protocol; driver connection
and firmware settings have not yet been checked (see docs/verification.md).

## Build and packaging

Daily builds use Cargo separately for the launcher and x86_64-unknown-uefi
application. `cargo build` alone selects only host default members. The VM
launcher can boot Cargo's .efi output directly; no script is needed for that.

Packaging scripts default to Release for both programs:
`bash scripts/build.sh` or
`./scripts/build.ps1`. Select Debug with --profile debug
or -Profile debug. They call the launcher's package command and produce
artifacts/esp, artifacts/efi-agent-vm.img, artifacts/native-esp, and
artifacts/efi-agent-native.img. The native tree/image includes CONFIG.JSON and
the workspace, without VM.TXT. Existing native workspace files are retained.
Native BOOTX64.EFI is the EDK II Shell 2.2 (26H1) build supplied through the
shallow `vendor/uefi-shell` submodule; its adjacent startup.nsh
runs connect -r, selects the common 80x25 console mode, then starts
EFI/AGENT/AGENT.EFI using homefilesystem (not an assumed fs0:). Esc skips the
startup script. Both trees include EFI/TOOLS/SHELLX64.EFI and its license/source
record. Agent remains the VM boot entry; the launcher selects AGENT.EFI when
preparing a private disk from a generated native package. The Shell binary is
selected automatically from the submodule's `edk2/Build/Shell/*/X64` output;
`EFI_AGENT_SHELL` overrides that path. It is not stored in Git.
Both scripts and VM launch use the same EFI_AGENT_* environment settings and
configuration parser. CONFIG.JSON is generated in the boot tree; no user config
file is needed. EFI_AGENT_CA_CERTIFICATE is a host DER path copied into the
boot tree automatically. Native packages contain API credentials; VM
credentials are written only into the private temporary disk.

## UI and model behavior

Slash commands: /help, /caps, /effort [value], /status, /clear, /quit, /exit.
A normal prompt runs at most twelve rounds of executed read/write/edit tools.
Edit requires one exact match, including overlap checks. Tool results enter
model history; slash commands do not. /clear resets history, retaining local
usage statistics. /status shows actual configuration (never the key), exact
history JSON bytes against 640 KiB, and reported last/cumulative token counts.
Missing counts are unavailable, not zero; output includes reasoning tokens.

Assistant text streams immediately. Reasoning uses only provider-supplied text
or summaries. Opaque/encrypted state remains in history. Tool calls execute only
after a complete successful stream. Stream bytes are capped at 8 MiB; events
and assembled messages are capped below 1 MiB; files are limited to 512 KiB.
The request response/handshake deadline is 120 seconds. Esc cancels network
waits and stops further tools. Already executed firmware file operations cannot
be undone. Each request uses its own TCP/TLS connection; a cancelled request is
dropped and does not block the next prompt.

Draft edits, cursor moves, scrolling, resize and cancellation remain responsive
during network waits. Enter submissions are deferred until the current operation
ends (up to 65,536 events). Grapheme-aware editor: Left/Right, Home/End, Delete,
Backspace; Up/Down retain the display column across draft lines. With an empty
editor, Up/Down scroll transcript. Mouse wheel always scrolls transcript.
Ctrl+J and reported modified Enter insert a newline; Enter submits. Prompts are
limited to 64 KiB and six visible editor lines. Bracketed paste preserves line
breaks, normalizes CRLF, and removes terminal controls.

Reasoning and per-tool argument/result panels start collapsed. Click or
Tab/Enter toggles them. Tool completion preserves expanded state. Running,
completed, and failed tools use yellow, green, and red; reasoning is magenta.

VM clipboard and selection run in the host launcher. Drag selects message or
input text, excluding UI controls/padding. Ctrl+C copies a selection; without
one, two presses within one second exit. Ctrl+Shift+C copies; Ctrl+V or
Ctrl+Shift+V pastes; right-click copies a selection or pastes. Windows injected
clipboard key records are buffered to avoid accidental multiline submission.
Selection includes Unicode, indentation, blank lines, and scrolled content.
Transcript changes/resize clear it; input changes clear input selection.
The clipboard handle persists for the session. Linux supports XWayland and
Wayland data-control; wl-copy/wl-paste provide a fallback when unavailable.
Clipboard failures appear in the footer, not as an application crash.

## Verification commands

Run builds/tests on each platform in its own local checkout. With a prepared
VM ESP and matching OVMF firmware:

```sh
uv run scripts/smoke_vm.py --launcher target/debug/efi-agent \
  --code artifacts/firmware/OVMF_CODE_4M.fd \
  --vars artifacts/firmware/OVMF_VARS_4M.fd --esp artifacts/esp \
  --output artifacts/direct-smoke
```

Add --api-format responses for Responses. Add --tls-failure untrusted,
hostname, or expired for negative TLS tests. The test starts an independent
local HTTPS provider and DNS server, then boots actual QEMU TCP4/TLS/SSE/file
tools. It needs no API credentials and performs no third-party provider call.

```sh
uv run scripts/smoke_launcher.py --launcher target/debug/efi-agent \
  --code artifacts/firmware/OVMF_CODE_4M.fd \
  --vars artifacts/firmware/OVMF_VARS_4M.fd --esp artifacts/esp \
  --output artifacts/launcher-smoke
uv run scripts/smoke_native.py --launcher target/debug/efi-agent \
  --efi artifacts/esp/EFI/BOOT/BOOTX64.EFI \
  --code artifacts/firmware/OVMF_CODE_4M.fd \
  --vars artifacts/firmware/OVMF_VARS_4M.fd --output artifacts/native-smoke
```

Linux launcher smoke uses a real tmux PTY and checks workspace persistence.
Native smoke uses QMP keyboard input and actual SimpleText, not VM serial.
scripts/smoke_shell.py checks Shell-first native boot on one and two volumes,
display pixels, /caps, /exit, and Shell command output in actual FAT files.
It also checks native boot without a NIC or firmware RNG. It uses native
Windows or Linux tools; select QEMU with --qemu and output with --output.
Windows ConPTY checks use scripts/smoke_windows.ps1; select the launcher path
explicitly when using Release. See docs/verification.md for tested limits.

# Work remaining

The `vendor/ipxe` submodule pins iPXE at
6262f1081fe185564e8ec8365a1d23597ec6e6f5. An unsigned x64 SNP application
was built in an isolated Linux workspace as `artifacts/ipxe/snp.efi` for
physical network diagnosis. It uses iPXE's own stack over existing SNP
interfaces; it does not provide Agent's TCP4 protocol. It is not packaged
into the Agent boot trees. See docs/verification.md.

- Physical bare-metal and third-party provider verification.
- Arbitrary command execution.
- Permissions and sandboxes.
- Richer tool views and conversation navigation.
- Live resize on physical terminals.

GOP, virtio-fs, and macOS HVF support are outside the current scope.
