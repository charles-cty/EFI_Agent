# Runtime capability audit

Date: 2026-11-01. Implementation baseline: c619bbe.

Later updates add the standard `python3` and `bash` command links. See
[busybox-applets.md](busybox-applets.md) for the current names and
[context-budgets.md](context-budgets.md) for the replacement history budgets.
The 2026-11-02 coding changes replace the write/edit, capture, input and task
contracts described here; see [coding-tools.md](coding-tools.md).
The observations below retain the audit baseline.

This audit checks the reported restrictions against the current source and real
WASM programs. Native checks use the Windows `run_guest.exe` runner. Package
checks use the standard prepared guests, `artifacts/esp/work`, and
`artifacts/native-esp/work`. The native runner was rebuilt from this source.
Firmware verification is recorded separately in `verification.md`.

## Installed data

### share/zoneinfo

`share/zoneinfo` contains IANA timezone rules in TZif files and zone tables.
The BusyBox catalog declares it as an ash library. Installation therefore copies
it into the workspace with ash. These are runtime data files.

`scripts/prepare_timezone.py` takes the files from the pinned `tzdata==2026.5`
package, which supplies IANA 2026e. It excludes Python package source and cache
files. The BusyBox date port uses the maintained musl timezone rules engine,
whose search paths include `/share/zoneinfo`. For example,
`TZ=Asia/Shanghai date -d @0 '+%Y-%m-%d %H:%M:%S %z %Z'` returns
`1970-01-01 08:00:00 +0800 CST`.

The unchanged Python distribution has different default timezone search paths:
`/usr/share/zoneinfo`, `/usr/lib/zoneinfo`, `/usr/share/lib/zoneinfo`, and
`/etc/zoneinfo`. A default `zoneinfo.ZoneInfo('Asia/Shanghai')` fails in the
tested installation. `zoneinfo.reset_tzpath(['/share/zoneinfo'])` makes the same
lookup work. `PYTHONTZPATH=/share/zoneinfo` can set this path at Python startup.
Shipping the BusyBox database does not change Python's `time` implementation.

Sources: `scripts/prepare_timezone.py`, `vendor/musl-timezone/__tz.c`,
`scripts/build_guests.py`, `crates/filesystem/src/installation.rs`.

### Python libraries

`scripts/build_guests.py::prepare_python` uses Brett Cannon's prebuilt
`python-3.14.7-wasi_sdk-24.zip`, from the `v3.14.7` release of
`brettcannon/cpython-wasi-build`. Its cached archive is under
`artifacts/python-prebuilt`. The build copies `python.wasm` and `lib` from this
distribution. The catalog declares `lib/python3.14` as Python's runtime library.

The current interpreter binary and all 537 upstream library files match the
archive bytes in each of the three standard directories. The build adds three
project files under `lib/python3.14/site-packages`:

- `sitecustomize.py`
- `_efi_process.py`
- `_efi_filesystem.py`

The standard library retains the files supplied by this WASI distribution.
Module presence does not establish support for every OS operation. For example,
this distribution omits `multiprocessing`; socket creation and native thread
creation remain unavailable. The installed Python command is `python`.
`python3` is absent from the standard command aliases.

### Startup adapters

1. Normal Python startup imports `site`, which imports `sitecustomize`.
   `sitecustomize` calls `_efi_process.install()`. Initialization errors print
   their traceback and end the process with status 1.
2. `_efi_process` opens `.efi-process` with `os.open`. This is a private runtime
   endpoint associated with the process, rather than a stored workspace file.
   Requests use little-endian integer fields through standard `os.write` and
   `os.read`. The runtime handles them in
   `crates/wasm/src/process_control.rs`. The endpoint cannot pass to children.
3. The process adapter replaces selected `os` and `posix` hooks for pipes,
   descriptor duplication/inheritance, PID queries, wait, and `os.system`.
   It enables the existing POSIX branch of upstream `subprocess`, disables
   `posix_spawn`, and replaces its private `_fork_exec` entry with cooperative
   runtime spawn. Upstream argument handling, `Popen`, pipes, capture, text
   decoding, and exit checks remain in use. No actual fork occurs.
4. `_efi_filesystem.install` supplies standard methods for modes, umask,
   stat/fstat/scandir metadata, cwd, unlink/rmdir, hard links, and filesystem
   statistics. It updates `os` and `posix`, with the relevant capability sets.
   Standard file I/O, symlink operations, and imports also use the runtime's
   WASI filesystem implementation.
5. `-S` skips these startup adapters. `-I` retains them. Process timeouts,
   credentials/groups, pre-exec callbacks, and distinct custom argv[0] remain
   restricted. Python `os.kill` is explicitly unsupported, even though the
   BusyBox `kill` command supports a limited set of process operations.

The interpreter binary and upstream `subprocess.py` remain unchanged. The
adapter depends on private CPython hooks and is tied to the pinned distribution.
`os.scandir` entries expose the expected methods through Python proxy objects;
their type identity differs from the built-in `os.DirEntry` type. Ordinary
opened files also retain the documented WASI default-inheritance difference.

Sources: `guests/python/sitecustomize.py`, `guests/python/_efi_process.py`,
`guests/python/_efi_filesystem.py`, `crates/wasm/src/process_control.rs`,
`docs/port-transparency-audit.md`.

## Filesystem support

The shared namespace supports file and directory creation/removal, overwrite,
rename, retained open-file identity after rename/unlink, directory-fd anchors,
seek/truncate, timestamps, allocation reporting, and persistent metadata.
Shell commands, Python, model file tools, and module lookup use the same tree.

Hard links share the same inode and backing object. `ln SOURCE DEST`, `cp -l`,
and `os.link` use this implementation. Symbolic links retain their target text;
`ln -s`, `readlink`, `realpath`, `os.symlink`, `os.readlink`, stat/lstat, and
cross-mount link traversal are supported. Directory hard links are prohibited.
Hard links and atomic rename across different mounts fail with EXDEV. BusyBox
`mv` can use its copy/remove fallback. Link traversal has a shared 40-link bound.

`chmod`, including symbolic and recursive modes, is installed. Python exposes
`os.chmod`, `os.fchmod`, `os.umask`, and mode/allocation-aware stat methods.
Access checks use a single virtual owner with UID/GID 1. A mode-000 file denies
reads with EACCES. Normal chmod follows a symlink; changing the symlink's own
mode with `follow_symlinks=False` is unsupported. This is a bounded filesystem
with selected POSIX behavior;
it does not implement a complete Linux filesystem or multiuser security model.

The current limits include 48 MiB per file, a shared 32 MiB temporary mount,
4096 directory entries per listing, 4096-byte paths and 1024-byte components.
The launcher limits workspace import/export to 128 MiB and the complete boot
tree to 160 MiB. These limits have different scopes. Available filesystem
capacity can impose a smaller effective limit.

The persistent inode graph, link groups, modes, and link targets are retained
through namespace metadata and launcher save/export. `/tmp`, `/dev`, and `/proc`
are virtual mounts and are excluded from persistent workspace export. The VM
still uses a private staged FAT disk and exports the workspace on exit.

Sources: `crates/filesystem/src/namespace`, `crates/filesystem/src/lib.rs`,
`crates/wasm/src/filesystem.rs`, `crates/wasm/src/filesystem/walk.rs`,
`guests/python/_efi_filesystem.py`, `crates/launcher/src/pack.rs`.

### /bin

Installed command entries are virtual read-only views with mode 0555. However,
`/bin` also merges ordinary children from an existing backing workspace `bin`
directory. The whole directory is therefore not uniformly read-only.

In the standard tree, the backing `bin` directory is absent. Creating a new
`/bin/audit-new-command` still fails with ENOENT, and `touch` prints
`No such file or directory`. That reported diagnostic is reproducible. It
does not establish that all of `/bin` is a read-only mount. Overwriting an
installed command is rejected separately as read-only.

Custom commands can use another workspace directory on PATH or `/tmp`.
`ln -s /bin/python /tmp/python3` works. `/tmp` aliases last for the mounted
runtime lifetime. The mount-layout tests also cover pre-existing workspace
`bin` children merged with installed command entries.

### /proc

`/proc` publishes runtime process records. Its numeric directories follow the
current guest PIDs. They are not a fixed set such as `1` and `24`. It provides:

- `/proc/self`, linked to the calling process.
- `/proc/<pid>/status`: name, runtime state, PID, parent PID, UID and GID.
- `/proc/<pid>/cmdline` and `environ`: NUL-separated startup values.
- `/proc/<pid>/cwd` and `exe`: links to runtime locations.
- `/proc/<pid>/fd/<n>`: current public descriptors, with supported alias I/O.
- `/proc/mounts`, `/proc/<pid>/mounts`, and `/proc/uptime`.

The process files are read-only snapshots taken at open. Their reported file
size can be zero despite readable content. Zombie records release execution
data, and reaping removes their directories. Private control descriptors are
excluded. CPU/RSS accounting and Linux files such as `/proc/meminfo` are absent.

`ps`, `pgrep`, `kill`, and `pkill` are installed. Signal 0 checks a positive PID.
INT, KILL, and TERM end the selected process with status 128 + signal. Children
survive and are adopted by PID 0. Signal handlers, process groups, and general
Linux signal delivery remain unavailable.

Source: `crates/wasm/src/filesystem/process.rs`,
`guests/busybox/process_support.c`, `docs/busybox-applets.md`.

### /dev

The current entries are `null`, `zero`, `random`, `urandom`, `tty`, `fd`,
`stdin`, `stdout`, and `stderr`.

`null` reads EOF and discards writes. `zero` reads zero bytes and discards writes.
`random` and `urandom` use the configured cryptographic random provider; writes
discard bytes without adding entropy. These four devices report character
metadata and mode 0666. Random reads can return a short result of 64 KiB.
Their isatty result is false. Mode/time mutations fail with EROFS; explicit
sync and truncate fail with EINVAL.

`fd` and the standard-stream names route through `/proc/self/fd`. `/dev/tty`
requires a controlling terminal. Model tools have none and opening it fails
with ENXIO, number 60. Direct `/enter` execution supplies a terminal, including
raw keys and dimensions for vi/less. Device inventory is a supported subset;
it does not include a general Linux block-device or mount-management API.

Sources: `crates/wasm/src/filesystem/device.rs`,
`crates/wasm/src/filesystem/process.rs`, `crates/wasm/src/io.rs`,
`crates/wasm/src/session.rs`.

## LLM file and shell return behavior

These are the project's model tools. Guest Python APIs and the host audit
runner have separate interfaces and limits.

### Read

- The only argument is `path`. There are no offset, byte-count, line-range,
  or pagination arguments. A file read returns its complete UTF-8 text.
- The file-content limit is 524288 bytes, inclusive. A larger file fails with
  `Read '<path>': file exceeds 524288 bytes`; no content prefix is returned.
  A file that grows past the limit during reading also fails. Invalid UTF-8
  fails with `Read '<path>': file is not UTF-8`.
- A directory returns sorted immediate entry names, one per line. It omits
  `.` and `..`, metadata, type markers, and recursion. An empty directory and
  an empty file both return an empty string. A directory listing above
  524288 UTF-8 bytes, including line separators, fails as a whole with
  `Directory listing exceeds 512 KiB`.
- The shared text-tool path validator changes backslashes to slashes and
  removes leading slashes. It rejects `..` components, control characters,
  `:*?"<>|`, paths above 4096 UTF-8 bytes, and components above 1024 bytes.
  Empty paths and `/` select the root directory.
- File tools use the shared mounted root, independently of the retained ash
  process's cwd. After shell `cd scripts`, `read("x.py")` still selects root
  `x.py`; `read("scripts/x.py")` selects the file in that directory.

Sources: `crates/filesystem/src/text.rs`,
`crates/core/src/agent.rs::Read` and `tool_definitions`,
`crates/uefi/src/main.rs`, and `crates/uefi/src/environment.rs`.

### Shell

- The return value is plain text with a status line, then `Standard output:`
  and `Standard error:` sections. Empty sections remain. Nonzero shell exit
  codes are ordinary tool results; execution or storage failures produce
  `Tool error: ...`. The caller must inspect the exit code or trap text.
  The status belongs to the complete shell evaluation. Earlier command
  failures can be followed by a successful command and an overall zero status.
- Each stream retains its first 65536 bytes for that call. Further bytes are
  discarded from the returned capture, and the affected section gains
  `[Output truncated at 64 KiB]`. This limit does not stop the command or
  truncate files written by it. The call waits for completion or cancellation.
- stdout is returned before stderr. Order within each captured stream remains,
  but cross-stream order is lost. `command 2>&1` merges the streams at the
  producer and uses one 64 KiB output budget.
- Captured bytes are decoded with `String::from_utf8_lossy`. Invalid UTF-8,
  including a character cut by the byte limit, can become U+FFFD. Use textual
  encoding when the caller needs to reconstruct binary output.
- One ash interpreter is retained per conversation. Variables, cwd, and shell
  options affect later calls. `exit`, a trap, or cancellation resets that
  session and adds a notice. Remaining child processes are stopped when the
  call ends, with a count notice; `command &` does not create a lasting model
  tool background job.
- Model execution has finite empty stdin and no controlling terminal. Explicit
  pipes or redirections supply input. The `command` source itself must contain
  no NUL and be at most 61440 UTF-8 bytes; this is separate from output limits.

Sources: `crates/uefi/src/wasm.rs::ToolOutput`, `run_code`, and
`stop_tool_tree`; `crates/wasm/src/session.rs`;
`crates/core/src/agent.rs::Agent::turn`; `guests/busybox/session.inc`.

### Shared history budget

The audit baseline used a 655360-byte serialized-message limit. The subsequent
context-budget update removes that limit. Complete requests now default to
4 MiB, with automatic compaction at 3 MiB and an 8 MiB canonical history cap.
The parser, summary and UI budgets are separately configured. The 512 KiB
read/write limits remain unchanged. These byte budgets are separate from a
provider's token context window. `/compact` preserves interpreter state;
`/clear` resets it. See [context-budgets.md](context-budgets.md) for the current
contract and `verification.md` for the update's own evidence.

Sources: `crates/core/src/budget.rs`, `request.rs`, `history.rs`, and `agent.rs`.

## Reported restrictions

| Item | Current result |
| --- | --- |
| 6: each LLM write is limited to 512 KiB | True for the model `write` tool: decoded content is limited to 524288 UTF-8 bytes, inclusive. Larger content returns `File exceeds 512 KiB`; the reported Python `OSError [Errno 28]` is not the current tool-layer error. Each call creates or truncates the target file, so repeated chunked calls to the same path overwrite rather than append. Python file-write probes are separate evidence and do not refute this tool limit. |
| 8: model write requires existing parents | False. `text::write` creates missing parent directories and reports path-specific errors. Ordinary Python open retains its normal parent-directory requirement. |
| 9: listed common commands are absent | False. All listed commands and zcat resolve under `/bin`. The standard catalog has 89 BusyBox names and 95 command aliases in total. `python3` remains absent; use `python` or add an alias. |
| 13: every call starts a new shell and interpreter | False for model shell and js. Each conversation retains its ash and QuickJS session. Shell variables/functions/aliases/cwd/umask/fds and JS globals survive calls. Each ordinary Python command starts a new Python process. Each JS ash template starts a separate ash child. Exit, cancellation, and clear have explicit reset rules. |
| 16: errno uses WASI numbers | True. EINVAL=28, EPERM=63, ENOTCAPABLE=76, EACCES=2, and errorcode[1]=E2BIG. Comparing with this guest's errno constants is correct. Testing only EPERM misses EACCES/ENOTCAPABLE/EROFS because they are different errors, not because errno.EPERM uses Linux numbering. |
| 17: Python socket is unavailable | True for standard socket creation: ENOTSUP=58. `_socket.getaddrinfo` is absent. Existing curl/WolfSSH and runtime TCP/DNS support still provide networking through their supported interfaces. |
| 18: threads and multiprocessing are unavailable | True for this distribution. Thread.start and ThreadPoolExecutor fail with `can't start new thread`; importing multiprocessing raises ModuleNotFoundError. Standard subprocess can run cooperative children, including Python children. |
| 19: os.statvfs is absent | False with normal site startup. statvfs/fstatvfs, shutil.disk_usage, and df use the runtime's actual provider. Native and firmware capacities differ. |
| 20: tempfile is unusable and deletion is absent | False. `/tmp` supports nested directories, exclusive temporary files, permissions, links, and cleanup. TemporaryDirectory and NamedTemporaryFile work and remove their entries. |
| 21: stdin is finite and empty | True for model tools without explicit redirection/input. sys.stdin.read returns empty and input raises EOFError. Pipes, subprocess input, and heredocs work; 70001 bytes pass unchanged. Direct `/enter` supplies live input and EOF. |
| 22: stdout.isatty is always true | False. All three standard streams are false in model capture. Direct terminal execution can report true. |
| 23: empty executable and reported Python paths | True. sys.executable is empty; sys.path includes `/lib/python314.zip`, `/lib/python3.14`, lib-dynload, and site-packages. sysconfig uses `//lib/python3.14`. A path-list entry does not establish that a zip or dynamic extensions exist. Use the installed `python` command for child launches. |
| 24: explicit flush is always required | False as a blanket rule. Normal Python exit flushes stdout/stderr. Explicit flush or -u controls prompt delivery during a running command. It cannot recover interleaving after separate stream capture. |
| 64 KiB per output stream | True for each model shell/js result, with an explicit truncation marker. Runtime queue capacity, JS command capture, pipes, and file limits are separate bounds. Python subprocess capture can exceed 512 KiB. |
| read on a directory lists names only | True. Entries are sorted, `.`/`..` are omitted, and no size/mode columns are supplied. Use ls/stat for metadata. |
| stdout/stderr lose cross-stream order in tool results | True. The tool result contains stdout first and stderr second. Order within each stream remains. Merge at the producer, such as `command 2>&1`, when a single ordered byte stream is required. |
| heredoc and <<< both work | Partly false. Quoted heredocs work. `<<<` returns status 2 and `syntax error: unexpected redirection` in current ash. |
| /proc has a few fixed entries and no useful process commands | False. Numeric entries are live runtime records; supported files and process commands are listed above. |

For Python code, requests/urllib cannot use standard socket networking, and
thread pools/multiprocessing remain restricted. tempfile, disk_usage, and
subprocess calls to installed ls/rm are supported. input() requires an explicit
input source or direct terminal execution.

## Verification

Native verification passed 244 Rust checks. The two default-ignored checks
were then run explicitly with the built ash image and both passed, for 246
passing Rust checks in total. All 17 CPython cases passed. The session, ln, df,
mutations, process files, character devices, temporary mount, mount paths,
and mount-layout integration suites also passed. Single guest Python calls to
`os.write`, unbuffered FileIO.write, and buffered write succeeded at 524287,
524288, 524289, 525312, and 8388608 bytes. The independent namespace reader
checked the saved 8 MiB Python-written file byte for byte after the runtime
exited. The LLM `write` size limit and overwrite behavior were checked in
`crates/core/src/agent.rs` and `crates/filesystem/src/text.rs`; no actual model
tool size-boundary test was run. The dedicated probe records actual error types,
errno values, Python paths, command resolution, buffering, and timezone lookup.

Both standard trees match all 1151 prepared runtime files. Read-only checks of
`C:\Temp\work` also match the current catalog, ash/Python/QuickJS/jq binaries,
and the three Python startup adapters. No installed workspace was changed.

Detailed commands, evidence paths, firmware results, and intermediate issues
are in [verification.md](verification.md).

Actual Windows QEMU/WHPX at 256 MiB also passed the 30-observation probe through
the Responses model tools. This run used the standard EFI and standard
ash/Python/QuickJS/library inputs. It verified automatic nested model-write
parents, directory read results, retained shell/JS state, per-stream 64 KiB
truncation, cancellation isolation, and an independent saved FAT comparison of
the complete 8 MiB file and tool-written script. Native and firmware results
agree on the tested restrictions. Filesystem capacity, live PIDs, elapsed time,
and the script directory in sys.path vary with the execution environment.
The separate full network/application workload and native hardware boot were
not repeated.
