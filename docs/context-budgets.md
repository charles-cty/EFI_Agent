# Model context and tool budgets

The agent uses separate byte budgets for request encoding, response parsing,
retained model history, and visible history. The former 640 KiB message-history
limit is removed. These limits measure bytes. They do not estimate model tokens.

## Configuration

Set `limits` in `EFI/AGENT/CONFIG.JSON`. The launcher accepts the same object as
JSON in `EFI_AGENT_LIMITS`. Omitted fields use the defaults below. Unknown fields
and inconsistent values cause configuration validation to fail.

| Field | Default | What it measures |
| --- | ---: | --- |
| `request_bytes` | 4194304 | Complete compact UTF-8 JSON request, including model settings, tools, history, and escaping. |
| `compact_at_bytes` | 3145728 | Request size that triggers automatic compaction before a task request. |
| `compact_target_bytes` | 2097152 | Summary input chunk sizing target; protected current-turn data can keep the result above this target. |
| `history_bytes` | 8388608 | Canonical retained message JSON, including array framing. Responses items are counted once. |
| `response_message_bytes` | 4194304 | Completed assistant message, including tool arguments and retained opaque metadata. |
| `event_bytes` | 4194304 | One SSE event's joined data payload, excluding its field and line framing. |
| `response_wire_bytes` | 33554432 | HTTP entity-body bytes, including SSE framing, excluding HTTP headers and chunk-transfer framing. |
| `summary_bytes` | 65536 | Decoded UTF-8 checkpoint text. |
| `ui_history_bytes` | 8388608 | Visible message strings plus fixed row storage, reclaimed at safe execution boundaries. |
| `context_window_tokens` | omitted | Optional provider model token window, displayed as configured. |

Require `4096 <= target < trigger < request` and
`request + response_message <= history`. Summary bytes must be positive and
smaller than the target. Event and message budgets must each be at least 4096;
the response body budget must cover each. UI history must be at least 4096.
Each main resource budget is limited to 128 MiB. A token window, if supplied,
must be positive. Limits are inclusive.

For example, `{"limits":{"context_window_tokens":128000}}` adds model-window
information while keeping all default byte budgets. Put this field in the full
configuration object beside the API settings. Set `EFI_AGENT_LIMITS` to
`{"context_window_tokens":128000}` for launcher-generated configurations.

`/status` displays request/history/parser/UI budgets, the last measured task
request, committed compactions and summary counts, and actual provider usage.
Task and summary request counts are separate. Last-task token fields do not
change when a summary is generated; token subtotals include both purposes.
Missing provider fields remain unavailable. Current retained-history tokens are
unavailable because no exact tokenizer is installed. A configured token window
is information, not a promise that a byte-admitted request fits that window.

## Compaction and recovery

Automatic compaction and `/compact` use the configured model with tools disabled.
The agent summarizes completed older exchanges in bounded UTF-8 chunks and
updates one rolling checkpoint. It preserves the initial system instruction,
the current user request, and the latest assistant/tool group. Retained opaque
reasoning items, signatures, null fields and call IDs stay unchanged. Old opaque
reasoning state is not copied into a checkpoint as executable provider state.
Checkpoint identity is private metadata, independent of its displayed text.

The agent validates call/result pairing in the prefix. It builds a candidate,
requires a size reduction and checks hard request/history budgets, then replaces
the history in one operation. Failure, cancellation, an empty or oversized summary,
or an unreduced candidate preserves the original history. There are at most 128
summary requests per compaction. The target is a goal: protected data can keep the
request above the target. If protected data cannot fit the hard budgets, the
agent reports a budget error and preserves interpreter state.

Compaction does not reset ash or QuickJS. It does not run completed tools again.
`/clear` retains its separate behavior: discard model/UI history and reset both
interpreters. A newly submitted user request rejected before model admission is
not retained in model history; its visible UI entry remains available.

An explicit provider `context_length_exceeded` or `max_context_length` error
permits one compaction and one retry per turn. Other provider failures do not
trigger a retry. Completed tool effects and their correlated results stay in
history during retry.

Before any operation in a tool-call group, the agent reserves the worst-case
escaped results for the whole group. File reads use the full read limit because
a background process can grow a file after metadata inspection. If the group
cannot fit, every call receives an ID-matched `not executed` result. No operation
in that group runs. A summary failure during admission also retires all call IDs
before reporting the error. Request fewer results per group if admission fails.

UI history is independent of model history. Safe boundaries remove old rows or
trim one oversized row at a UTF-8 boundary, with an omission marker. The active
stream or tool can temporarily use space in addition to the UI history budget;
its existing parser/capture limits bound that space. Row-index invalidation uses
the transcript generation, so live background output recreates its disclosure.

## Tool contracts

The tool descriptions state these limits directly. The tool names and parameter
schemas remain `read`, `write`, `edit`, `shell`, `js`, and `reset_js`.

| Tool | Contract |
| --- | --- |
| `read` | Complete UTF-8 file, or sorted names from one directory. Maximum 512 KiB. Oversized or invalid UTF-8 data returns an error and no partial content. Directory results have no metadata or pagination. |
| `write` | At most 512 KiB decoded UTF-8; create missing parents; create or truncate the target. No append mode. An I/O failure can leave parents or partial/truncated content. |
| `edit` | A nonempty, unique exact match, including overlap detection. Original and resulting files must each fit 512 KiB. The save has the same possible partial effects as `write`. |
| `shell` | Retained ash state; at most 60 KiB source, no NUL. Finite empty stdin, no controlling terminal. Plain-text exit status plus separate stdout/stderr; nonzero status is a normal tool result. |
| `js` | Retained QuickJS state and fresh ash children. The source limit is 60 KiB/no NUL. Inner command input/capture has its own 512 KiB limits. |
| `reset_js` | Release only the JS session and its children; preserve shell, files, and independent roots. |

File paths use the shared guest root, independent of shell cwd. `/` is the path
separator, a leading slash is optional, and `..` components are rejected.
Shell/JS tool output retains the first 64 KiB per stream with an explicit marker;
invalid UTF-8 is decoded with replacement. Capture loses cross-stream ordering.
Truncation does not stop execution. Remaining call children are stopped after
completion. Exit, trap and cancellation report interpreter resets.

The 512 KiB file-tool, 60 KiB source, 64 KiB returned-stream, and 512 KiB inner
QuickJS-command limits remain separate from the configurable model budgets.

## Verification

See `verification.md` for the current native, memory, firmware and standard-package
results. The maintained firmware case is `scripts/smoke_vm.py --context-budgets`.
It uses a local provider and checks the exported FAT bytes independently.
