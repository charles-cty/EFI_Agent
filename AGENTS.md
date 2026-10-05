# Rules

You are a senior software engineer working on this project, EFI Agent.

Do not edit README.md for your memory. Edit [the Current state section here](#current-state)
instead. Keep README.md concise and clean.

Maintain this document `./docs/verification.md` by yourself, too.

# Current state

You can use this section as your memory. Feel free to edit this section.

## UI/UX

Slash commands control the session: `/help`, `/clear`, `/caps`, `/effort`, `/status`,
`/quit`, and `/exit`. File operations are agent tools, not slash commands.
A normal prompt calls the provider configured on the host. The agent can read/write files,
replace one exact occurrence with edit, ... It returns tool results to the model
and stops after at most twelve rounds of executed tools. All tools use one
configured workspace: the launcher workspace in VM mode or the boot-volume
workspace in native mode. Slash commands are
separate from model history. `/clear` resets the conversation.

The TUI shows model and tool status, tool arguments, bounded result previews,
and the latest conversation output. Up/Down move the draft cursor between
newline-separated lines and retain its display column across short lines.
With an empty editor, Up/Down scroll older/newer output. The mouse wheel scrolls
output even with a draft present. These controls also work during model waits.
Esc cancels the active request and returns to Ready.
In VM mode, Ctrl+C copies a selection or uses the two-press exit gesture.
VM dimensions and draft edits update during network waits. Typing, pasting,
deleting text, and moving the editor cursor take effect immediately.
Enter submissions wait until the current operation ends, with a limit of
65,536 queued submission events.

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
Esc can cancel the wait; the VM exit gesture remains available. Heartbeats are
retried automatically; file operations and model requests are never replayed.
The host applies its read timeout only after a frame starts, so normal inactivity
does not close a session. Peer resets and broken pipes retire the connection without
an error log. In VM mode, other HostBridge diagnostics are buffered until the host
terminal is restored, then written to stderr. Standalone `serve` reports them
immediately. Connection setup and frame transmission retain their own bounded
timeouts; the five-second deadline applies to the heartbeat reply after transmission.

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

In VM mode, drag the left mouse button to select visible terminal text.
Ctrl+C copies an active selection. With no selection, two presses within one
second exit; an expired confirmation requires a new pair. Copying and other
keyboard input cancel an armed exit confirmation.
Ctrl+Shift+C also copies. Ctrl+V or Ctrl+Shift+V pastes system clipboard text
into the prompt without submitting it. Right-click copies an active selection
or pastes when there is no selection. Esc clears a selection. A single left
click still toggles a disclosure panel. These clipboard controls run in the
host launcher, not in bare-metal firmware.

Some Windows terminals handle Ctrl+V themselves and inject clipboard text as
ordinary key records. The launcher buffers a matching multiline clipboard
prefix and routes the content through bracketed paste before forwarding any
matched newline. A partial match without a newline returns to normal typing
after 150 ms without input. This detection requires the system clipboard to
still contain the pasted text; native paste events and application-handled
Ctrl+V use the explicit paste path.

Selection includes both end cells and copies message bodies or input text with 
Unicode, code indentation, and blank lines. Interface titles, disclosure controls,
input borders, status text, and terminal padding are excluded. Expand a folded
panel before selecting its contents. Hold the left button at the top or bottom
of the message viewport to scroll automatically; the selection retains text
that moves offscreen. Releasing the button stops automatic scrolling.
Selection rendering updates only changed cells. A changed transcript or window
size clears the selection to prevent stale copies; scrolling and idle cursor
controls keep it.

Input selection excludes the prompt marker, editor cursor, and borders. A drag
that starts in the input remains within it and does not scroll the transcript.
Editing the input clears its selection; copying leaves the input unchanged.
Dragging can start in the blank space before or after a body line, or on an
empty transcript row. Blank coordinates remain selection anchors; copied text
still excludes terminal padding and controls.

Clipboard failures appear in the terminal footer and do not end the session.
The launcher keeps its clipboard handle alive for the VM session. On Linux/X11,
this lets other applications read copied text while the launcher owns the
selection. Retaining that text after exit depends on a clipboard manager.
Linux builds enable native Wayland data-control support. When the compositor
does not provide data-control, XWayland can supply the desktop clipboard.
If no desktop backend can connect and `WAYLAND_DISPLAY` is set, the launcher
uses `wl-copy` and `wl-paste` instead. Install `wl-clipboard` for this native
Wayland path (`sudo apt install wl-clipboard` on Ubuntu/Debian). The helper owns
copied text independently of the launcher. Access through the core Wayland
protocol depends on the compositor's focus policy. No helper output is written
into the active terminal, and paste retains the exact text and line endings.

## HostBridge

The launcher and relay validate API configuration before starting. Missing or
blank base URL, key, or model produces an immediate error. The base must be an
HTTP or HTTPS URL. Each prompt goes directly to the model request. There is no API
availability probe. Connection failures and provider HTTP errors, including
authentication, quota, and billing errors, are reported from the actual request.

Frames contain a four-byte big-endian length and JSON, with a 1 MiB limit.
Requests carry an ID and a tagged operation: `ping`, `read`, `write`, `edit`, or
`complete`. Responses carry the same ID and a result. A model request can send
text progress frames with a `delta` field before its final response. The guest
uses request IDs to discard both progress and final frames from cancelled calls.
Files are UTF-8. Paths are relative to the configured workspace. Parent traversal
and resolved paths outside that workspace are rejected. Concurrent filesystem changes
are not isolated; this service is intended for a local, trusted workspace.

The launcher forwards guest TCP `10.0.2.100:7420` to a loopback service through
QEMU user networking.

Responses API streams answer text, maps function calls and results, and retains
the returned output items (including opaque encrypted reasoning state) across
tool rounds. It uses `store: false` and requests `reasoning.encrypted_content`.
The selected provider/model must support these parameters. No API-format retry
or fallback is performed. `/status` shows the selected format and endpoint.

Reasoning is enabled by default with `reasoning_effort: "medium"`.
`EFI_AGENT_REASONING_EFFORT` accepts any non-empty value without whitespace.
Omit it to use `medium`. The host has no fixed list of supported values.
Use `/effort` to show the setting, or `/effort <value>` to change it for future
requests on the current bridge connection. The command sets a request parameter;
it does not confirm provider support or set an exact token budget.
On reconnect, the setting returns to the host environment default.
The selected provider and model must support the requested value and the
selected API's reasoning parameter. Unsupported settings return an API
error. The host does not retry with reduced reasoning. For Linux, use
`export EFI_AGENT_REASONING_EFFORT=high` before starting the launcher or relay.

Model replies use SSE streaming. Assistant text appears as it arrives in VM
and native mode. The host assembles tool IDs, names, and JSON arguments across
chunks. Tools run only after a complete, successful model reply. Truncated
streams and `length` or `content_filter` finish reasons report an error and do
not execute partial tool calls. Partial text remains visible after an error
or cancellation but does not enter model history. Returned `reasoning_content`
is retained in assistant history for subsequent tool rounds. Reasoning display
uses only provider-supplied text: Chat Completions `reasoning_content`, textual
`reasoning`, `reasoning_summary`, and `reasoning_details` entries of type
`reasoning.text` or `reasoning.summary`; Responses reasoning text and summary
items are also shown. Encrypted reasoning is preserved but never displayed as
text. Unknown formats are not decoded. If no reasoning text or summary is
returned, no reasoning panel appears. Reasoning panels appear after successful
completion of each model round; answer text still appears as it arrives.
The host limits stream wire data to 8 MiB and each assembled
message to less than 1 MiB. The existing 120-second API timeout applies to the
whole stream.

Chat Completions API retains extension fields on function calls and their function
objects, including nested signatures. Text fragments are concatenated; opaque
metadata keeps its JSON values. Conflicting opaque values return an error rather
than being guessed or concatenated. Null reasoning detail fields remain null;
null text deltas do not erase text already received.

Responses retains reasoning items from output-item stream events, plus indexed
reasoning text parts. The final output takes precedence, and missing reasoning
items are restored in output-index order without duplicate IDs. Missing fields
can be filled from stream items; existing final values are not replaced.
Reasoning text received without an item/part index is added only when its target
is unambiguous. An unresolved association reports an error instead of silently
dropping state while continuing tool execution.

Reasoning and reasoning summaries use their own panels. Each tool execution
uses one panel with arguments and its corresponding result. All detail panels
start collapsed with `[+]` headers. Click a header to expand it and click it
again to collapse it. Tool results update the existing panel and preserve its
expanded state. Expanded arguments and results show their full stored text,
with no preview truncation. Running tools are yellow; completed tools are green
and failed tools are red. The header shows running, done, or failed. Reasoning
is magenta.
The VM launcher enables terminal mouse reporting and forwards clicks and wheel
scrolls to the guest. Native mode uses optional firmware pointer protocols and
marks the pointer cell. Relative firmware pointers use eight movement units per
text cell; speed can vary by firmware. When no pointer is available, use Tab to
select a visible detail header, then Enter with an empty prompt to toggle it.

`/status` shows the configured API endpoint and model, key configuration state
(never the key), reasoning effort, and host requests in flight. It shows the
current model history message count and its exact JSON byte size against the
640 KiB local history limit. This byte limit is not a model token limit. Current
history tokens and the model context window are unavailable because this app
has no model tokenizer or provider-supplied context limit.

In Chat Completions API, the host requests `stream_options.include_usage`.
Responses mode reads the completed response's usage object. `/status` shows the last
finished host request's reported input, output, total, reasoning, and cached
input tokens. Output tokens already include reasoning tokens. Each cumulative
subtotal gives the number of requests that supplied that field. Missing fields
show `unavailable`; they are never replaced with zero. The cache ratio is reported
cached input tokens divided by input tokens. These statistics cover the current
bridge connection, including tool rounds and cancelled requests that finish on
the host. `/clear` clears model history but keeps these statistics. A reconnect
starts new statistics. Last prompt tokens describe the submitted prompt, not
the current history after replies and tool results. Providers that reject
`stream_options.include_usage` return their actual API error.

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

# Work remaining

- Physical bare-metal verification.
- Arbitary command execution.
- Permissions and sandboxes.
- Richer tool views and conversation navigation.
- Live resize.

GOP, virtio-fs, and macOS HVF support are outside the current scope.
