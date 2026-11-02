# Coding task execution

The model can run until it completes or the user cancels. `limits.max_tool_rounds`
defaults to `null`. Set a positive integer to pause a request after that many
tool groups. Pending calls receive error results when the budget is used. A
subsequent user request can continue with the retained results.

## Persistent commands

Use `task` for builds, servers, watchers and commands that require input.

| Action | Required fields | Result |
| --- | --- | --- |
| `start` | `command` | Start a fresh ash root and return its task id. |
| `poll` | `id` | Report status and output previews. |
| `input` | `id`, optional `input` and `eof` | Send UTF-8 input and optionally close stdin. |
| `cancel` | `id` | Stop that task's entire process tree. |
| `list` | none | List retained task handles. |
| `forget` | completed `id` | Release the handle and keep its output files. |

`wait_ms` is 0 to 10000. Zero returns after one dispatcher step. Poll again for
longer waits. Cancelling a wait leaves the task running. Task cancellation
preserves the retained shell and JS sessions. `/clear` stops tasks and resets
those sessions. Set `terminal=true` at start for a controlling terminal. Its
output merges stdout and stderr and can contain escape sequences. Other tasks
keep the streams separate. Input queues hold 64 KiB; send smaller chunks as the
command consumes them. `eof=true` supplies EOF.

Each task starts with the configured guest environment and root cwd. Set cwd
or environment in `command`. It has separate shell state from `shell`. Tasks
survive model rounds and user requests. Completion includes all descendants.
Completed process records are reaped; handles and logs remain until released.

The retained `shell` and `js` tools support stateful short evaluations. Their
cancellation can reset their interpreter. Use a task for cancellable commands
when interpreter state must survive. All commands execute in the UEFI WASM
runtime with its process, memory and filesystem capacities.

## Complete output

Model shell/JS and task output is saved under `/.agent-output/<sequence>/stdout`
and `stderr`. Results report paths and byte counts. Previews retain the first
and last 32 KiB per stream, with an omission marker. Retrieve full logs with
ranged `read`; use hex for exact binary bytes. Terminal tasks write their
merged stream to stdout.

Logs consume persistent workspace capacity and remain after handle release.
Remove old logs with shell file operations when finished. Storage failures are
reported. A task whose output cannot be stored is stopped, and its result
states that output may be incomplete.

Direct `/enter` evaluations use the terminal history and do not create these
model-output log files.

## File tools

`read` without range arguments reads complete UTF-8 files or sorted directory
names up to 512 KiB. Set `offset` and `limit` to read a file by byte range.
`limit` is 4 to 524288 bytes, default 65536. Results contain `content`,
`encoding`, `offset`, `next_offset`, `total_bytes` and `eof`. UTF-8 pages end at
a character boundary; continue from `next_offset`. The starting offset must
be a character boundary. Set `hex=true` for arbitrary binary ranges.

`write` creates missing parents and stages replacement content in an exclusive
sibling file. Rename commits after writing and flushing. A staging failure
preserves the original. Regular-file destinations are required; use explicit
shell operations to choose a symlink target. Replacement uses a new inode and
preserves existing permission bits. `append=true` appends chunks and can leave
partial appended data on failure. The former 512 KiB write cap is removed;
model request size and the 48 MiB filesystem file limit apply.

`edit` requires a nonempty, unique exact match, including overlap checks. It
reads files up to 48 MiB and uses the same atomic replacement path. The request
budget bounds the old and replacement text arguments.

## Provider recovery and model windows

`limits.provider_retries` defaults to 2 and accepts 0 to 10 for task requests. Transient HTTP
408/429/500/502/503/504 and recognized transport failures can be retried with
exponential delay. Partial visible responses are not automatically replayed.
Completed tool effects remain in history and are not executed again.

`limits.provider_timeout_seconds` defaults to 120. It controls the initial and
idle read wait after TCP connection. Receiving transport data refreshes it.
DNS, address mapping and TCP connection have separate deadlines. Expiry is
latched because UEFI CheckEvent consumes a signalled event. An independent RTC
deadline also stops waits when firmware event delivery is delayed.

If `context_window_tokens` is configured, scheduling estimates the complete
request tokens and reserves `response_token_reserve` (default 8192). Exceeding
the estimated window triggers compaction, then rejects admission if protected
data still cannot fit. The estimate uses ASCII/non-ASCII text and structured
request overhead. Byte budgets remain independent memory bounds. `/status`
reports these settings. Exact provider tokenization remains unavailable.
