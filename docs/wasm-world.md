# WASM process world

The common `efi-agent-filesystem` crate supplies no_std provider types and an
persistent inode namespace used by firmware and the native guest runner.
The namespace stores logical directories,
links, modes and times in two checksummed metadata generations above raw
backing files. It retains open-file and directory identity across mutations.
WASI unlink/rmdir, rename across directory views, symlink/readlink, lookup
flags and path/descriptor timestamps now call provider methods. Raw providers
can reject unimplemented operations explicitly. Model file tools and module
loading share the mounted namespace. Virtual mounts remain pending.

`namespace::read_snapshot` reads the committed logical tree without recovery
writes or garbage collection. It exposes validated backing paths to host
export code only. The firmware raw provider now implements deletion and volume
capacity. Launcher staging, logical export and host save preserve directory
entries, links and metadata, with host conflicts checked before save effects.
Its integration suite includes an actual FAT save/reload/restage round trip.
Read-only closes defer access-time persistence to explicit file/workspace
flush. Writable closes flush and retain failures for the execution boundary.
Absolute `/wasm load` paths now select the workspace root, matching guest and
model file paths, rather than the raw boot-volume root.

The UEFI application embeds wasmi 2 with `alloc` and no Rust standard library.
`crates/wasm` contains the firmware-independent process world.
`crates/uefi/src/wasm.rs` connects it to the firmware UI. No host process or
UEFI Shell protocol runs guest instructions.

Each process has its own wasmi Store, instance, linear memory, globals, and
retained call stack. The dispatcher runs at UEFI APPLICATION TPL on one thread.
It selects one ready process per step. A guest can yield through
`wasi_snapshot_preview1.sched_yield`. wasmi fuel also bounds a compute loop:
fuel exhaustion retains the stack and queues the process again. The normal
quantum is 50,000 fuel units. Bulk operations can require more fuel than one
quantum; the next step supplies the minimum required by wasmi. A quantum is
therefore not a strict wall-clock deadline.

Module start sections are rejected. A command must export `memory` and a
`() -> ()` function named `_start`. This keeps guest execution out of module
instantiation. Modules are validated. Each instance has a 64 MiB memory limit,
one memory, one table, and at most 65,536 table elements. The world holds at
most 32 processes, including exited processes until they are reaped. Normal
return, proc_exit, traps and cancellation immediately drop the execution
Store, linear memory, pending I/O and continuation, closing all descriptors.
The terminal record retains its parent PID, status and shared stdout/stderr
until wait/reap. Live children keep their inherited output and continue after
their parent exits or is reaped. The
firmware loader accepts modules up to 48 MiB. These are initial resource limits,
not a sandbox or permission policy.

Module registration validates and compiles functions eagerly. Lazy translation
can exhaust fuel while resolving a callee without returning a continuation;
large real C programs exposed this failure. Execution still uses resumable
fuel exhaustion and round-robin scheduling.

## Firmware commands

The shared BusyBox image includes upstream basename, dirname, pwd, env,
printenv, seq, yes, true and false in addition to the upstream stream,
regex/editor and sleep applets. Shell builtins and functions retain their
normal precedence; /bin-prefixed names select registered external applets.
Pwd enables its upstream -L/-P parser locally. Env supports clearing/unsetting
variables, NUL output and cooperative execution with explicit environments.
Seq retains upstream floating-point formatting and sequence semantics.
Yes checks stdout errors after each line, including errors inside fputs, so
multiword pipelines terminate after the reader closes. WASI returns EPIPE
without SIGPIPE delivery; upstream yes reports Broken pipe before exiting
with failure. The pipeline retains the final stage's status.

Mkdir, rmdir, rm and touch also use their upstream BusyBox implementations.
Mkdir preserves existing modes under -p and applies explicit modes to new final
directories. Recursive rm uses lstat and does not descend through symbolic
links. Interactive prompts consume finite stdin. Directory open/read/close
errors fail with diagnostics; failed removal does not print a success line.
Verbose output failures affect status. Touch supports -c/-h/-a/-m/-r/-d/-t,
including nanosecond reference times and link-local timestamps. A new-file
timestamp failure is reported. Time parsing uses upstream libbb; CPU-time
clocks, signals and clock setting remain outside these applets.

Date uses upstream BusyBox UTC parsing and formatting, RFC/ISO output,
nanosecond fields and file reference times. Invalid -D input tails, unavailable
clock reads, oversized formatted output and closed stdout fail explicitly.
Setting the system clock fails with ENOTSUP. WASI libc supplies UTC only;
date rejects other TZ values and accepts -u to select UTC explicitly.

The model receives `shell(command)` and `python(code)` in addition to the
read/write/edit file tools. Shell code runs as `ash -c`; Python code runs as
`python -c`. Source is passed as one unchanged argument, including whitespace,
quotes and newlines. Source must be at most 60 KiB and contain no NUL bytes.
The tools load workspace `ash.wasm` on first use, and `python.wasm` on first
Python use. A registered name is reused, including a module loaded manually.
Python requires the prepared standard library and startup adapter in `lib`.
Each call starts a fresh process with empty finite stdin. Workspace files
persist across calls; shell variables and interpreter globals do not.

Tool results contain readable exit status and separate stdout/stderr sections.
Capture retains the first 64 KiB of each stream, drains further bytes and marks
truncation explicitly. Text uses UTF-8 with replacement for invalid bytes.
The existing tool disclosure shows the result; execution adds no WASM panel.
Esc cancels execution. Completion, cancellation and terminal-refresh failures
cancel and reap all remaining descendants of that tool root. Independent
background roots remain active. Original root IDs survive parent adoption.

The model prompt describes an Alpine Linux-style BusyBox ash environment and
states its actual WASM limits. POSIX shell syntax is available. Bash-only
syntax, a Linux kernel, `apk` and arbitrary Linux binaries are unavailable.
External commands still require registered WASM programs.

```text
/wasm load PATH
/wasm load NAME PATH
/wasm start NAME [ARGS...]
/wasm run NAME [ARGS...]
/wasm ps
/wasm kill PID
/wasm reap PID
```

`load PATH` registers the final filename without its `.wasm` suffix as the
program name. The suffix is case-insensitive; other suffixes remain in the
name. For example, `tools/ash.wasm` registers `ash`. Use `load NAME PATH` to
select an explicit name, such as a separate alias for another module version.
An empty filename or stem is rejected.

Module paths accept `/` and `\` separators. Relative paths use the configured
workspace; paths that start with either separator use the boot volume root.
For example, `python.wasm` and `./python.wasm` load `\work\python.wasm` with the
default workspace. `/work/python.wasm` and `\work\python.wasm` select that file
explicitly. Parent traversal and volume prefixes such as `fs0:` are rejected.
Copy guest binaries directly into the host workspace before starting the VM:

```powershell
Copy-Item .\artifacts\wasm-guests\*.wasm .\workspace\
Copy-Item .\artifacts\wasm-guests\lib .\workspace\ -Recurse -Force
```

Then load and run Python in the firmware UI:

```text
/wasm load ash.wasm
/wasm load python.wasm
/wasm run python -c print(6*7)
```

Each `run` or `start` creates one output disclosure, collapsed by default.
Its header shows the command, process ID and a readable running/completed/
failed/cancelled status. Expand it with a click or Tab/Enter to read the
command, status, standard output and standard error as plain text. Stream
updates and completion edit the same disclosure and preserve its expanded
state. Descendant output belongs to the same root command; child exits do
not create separate notifications. Independent background roots have their
own disclosures. Clearing the conversation discards earlier output; later
background output creates a new collapsed disclosure.

`start` returns to the UI while the process runs. `run` polls the UI and all
ready processes until its root process exits; Esc cancels that root with status
130. Background processes also advance during model network polls. `kill`
cancels one process, not a process group. Command arguments currently split on
whitespace. They do not support quotes. Guest `spawn` preserves empty arguments
and Unicode. `argv[0]` selects a program already loaded into the world.

## Guest imports

The current WASI Preview 1 imports are `args_sizes_get`, `args_get`,
`environ_sizes_get`, `environ_get`, `fd_read`, `fd_write`, `fd_close`,
`fd_renumber`, `fd_fdstat_get`, `fd_fdstat_set_flags`, `fd_filestat_get`,
`fd_filestat_set_size`, `fd_seek`, `fd_tell`, `fd_sync`, `fd_datasync`,
`fd_readdir`, `fd_prestat_get`, `fd_prestat_dir_name`, `path_open`,
`path_filestat_get`, `path_readlink`, `random_get`, `poll_oneoff`,
`clock_time_get`, `clock_res_get`, `proc_exit`, and `sched_yield`.
Firmware roots start with an empty environment. Descriptor 0 reads finite input and
returns EOF when the input is empty. Descriptors 1 and 2 append to distinct
output buffers, each bounded to 512 KiB between host drains. Full buffers
suspend writes until the host drains them; available space permits a short
write. Stream requests above 512 KiB also use short writes, with all iovecs
validated before any transfer. Empty writes succeed even with a full buffer.
The private process-control endpoint retains its atomic frame-size limit.
Child processes
inherit selected input and output objects. Invalid iovecs return WASI FAULT
before input is consumed or output is appended. Selected unsupported operations
are registered with explicit error results; unknown imports fail linking.
Streams return SPIPE (70) from `fd_seek` without writing a position.
Regular files support seek and tell through the host file provider.

The `efi_process` namespace adds:

| Import | Wasm signature | Result |
|---|---|---|
| `getpid` | `() -> i32` | Current PID |
| `getppid` | `() -> i32` | Parent PID, or 0 for a root or orphan |
| `program_exists` | `(i32 ptr, i32 len) -> i32` | Exact registered-name lookup: 1, 0, or negative errno |
| `input_snapshot` | `(i32 ptr, i32 len) -> i32` | Copied read-only input descriptor, or negative errno |
| `spawn` | `(i32 ptr, i32 len) -> i32` | Child PID, or negative errno |
| `spawn_fds` | `(i32 ptr, i32 len, i32 input, i32 output, i32 error) -> i32` | Child PID with selected standard descriptors |
| `spawn_env_fds` | `(i32 argv_ptr, i32 argv_len, i32 env_ptr, i32 env_len, i32 input, i32 output, i32 error) -> i32` | Child PID with explicit environment and selected descriptors |
| `spawn_mapped_fds` | `(i32 argv_ptr, i32 argv_len, i32 env_ptr, i32 env_len, i32 mappings_ptr, i32 count) -> i32` | Child PID with explicit environment and descriptor table |
| `pipe` | `(i32 result_ptr) -> i32` | Writes read/write descriptors; returns 0 or negative errno |
| `dup2` | `(i32 source, i32 target) -> i32` | Target descriptor or negative errno |
| `dupfd` | `(i32 source, i32 minimum) -> i32` | Lowest available descriptor at or above minimum, or negative errno |
| `get_fd_flags` | `(i32 descriptor) -> i32` | Per-descriptor close-on-exec bit 0, or -9 |
| `set_fd_flags` | `(i32 descriptor, i32 flags) -> i32` | Set close-on-exec bit 0; returns 0, -9 or -22 |
| `waitpid` | `(i32 pid, i32 status_ptr) -> i32` | Reaped child PID, or negative errno |
| `waitpid_nohang` | `(i32 pid, i32 status_ptr) -> i32` | Reaped child PID, 0 while live, or negative errno |

`spawn` reads UTF-8 arguments separated by NUL, including a final NUL. The
buffer is limited to 64 KiB. It creates a new instance; it does not copy an
existing process. Failure returns ENOENT (-2), EAGAIN (-11), ENOEXEC (-8),
EFAULT (-14), or EINVAL (-22). There is no `fork` import.

Arguments and environment share a 64 KiB byte limit, including terminating
NULs. Size overflow returns E2BIG (-7). Ordinary spawn inherits a copy of the
parent's startup environment. `spawn_env_fds` replaces it with its explicit
NUL-separated UTF-8 entries. A zero-byte list clears the environment. Each
entry must have a nonempty name followed by `=`; values can be empty or contain
more `=` characters. Names must be unique. Invalid entries return EINVAL,
invalid memory returns EFAULT, and failed validation creates no child or PID.
WASI environ_sizes_get/environ_get return this process-local startup list.
Shell exports and guest libc mutations require explicit environment passing;
they do not update the host's stored list automatically.

Hosts can create roots with `World::spawn_with_environment`; no Windows or
Linux host environment is copied implicitly. The C helper
`efi_process_spawn_argv_env_fds` packs both lists into caller-owned scratch
space. BusyBox can use it with the actual shell export list when dispatching
external commands as fresh instances.
`efi_process_spawn_argv_env_mapped` shares the same bounded argv/environment
packing and accepts the complete selected descriptor map. Ash uses this helper.

Each process has up to 128 descriptors. Only descriptors 0, 1 and 2 cross
`spawn`; `spawn_fds` selects the source descriptor for each. Other descriptors
stay in the caller unless selected by `spawn_mapped_fds`. Its mappings are
little-endian pairs `{child target, parent source}`. Targets must be unique
and below 128; fd 3 is reserved for the workspace preopen. A map can contain
0..127 entries. All sources refer to the unchanged parent table, so output
swaps and repeated sources are well-defined. Explicit duplication clears the
child's close-on-exec flag without changing the parent. An empty map inherits
no standard streams. The workspace preopen remains independent of the map.
Close-on-exec state is separate from the shared stream object for every
descriptor kind, including files, pipes, snapshots and sockets. Closing drops
the state before reuse. Renumber preserves it; duplication clears the new copy,
except same-fd dup2, which leaves it unchanged. Unknown flag bits fail with -22.
Invalid targets/duplicates return -22, missing sources -9, oversized count -7,
and invalid memory -14; failed validation starts no child and consumes no PID.
Anonymous pipes hold up to 64 KiB. Reading an empty pipe
blocks while a writer exists. Writing a full pipe blocks while a reader
exists. Reads consume available bytes; writes can return partial progress.
The caller must loop to transfer all bytes. Closing the last writer supplies
EOF after buffered bytes drain. Closing the last reader supplies WASI PIPE
(64) to a nonempty write. Zero-byte operations complete immediately after
descriptor direction and memory validation. Exit, traps and cancellation
close all process descriptors and wake blocked peers. Duplicates and inherited
descriptors each retain an endpoint until closed.

`poll_oneoff` accepts read and write subscriptions and returns only ready
events, with the original 64-bit userdata. It reports available byte counts,
EOF/HUP, and per-event descriptor errors without consuming bytes. If no
subscription is ready, the process retains its Wasm continuation in `Polling`
state. Pipe changes, endpoint closure and host output drains allow the next
dispatcher pass to resume it. Multiple subscriptions can watch child stdout
and stderr together. An immediate relative clock subscription implements a
nonblocking readiness check. Other clock subscriptions return NOTSUP (58)
when their clock backend is unavailable. With a provider, relative waits
compute a checked deadline once at syscall entry. Relative realtime waits
use the monotonic source; RTC granularity and wall-clock corrections cannot
change an elapsed duration. Absolute waits compare the supplied deadline
against the requested clock directly. Clock errors resume a blocked call with an errno;
overflow returns OVERFLOW (61). Clock polling does not advance the host clock.

Hosts install `clock::Clock` through `World::set_clock` before spawning any
process. Readings and resolutions use nanoseconds. The firmware provider
uses EFI GetTime, UTC conversion, nanoseconds and GetTime capabilities for
the realtime clock. Monotonic readings use a 64-bit UEFI Timestamp counter,
or PI CPU timer 0 when Timestamp is absent. The constant counter frequency or
femtosecond tick period supplies a rational nanosecond conversion. Fractions
are retained across reads; 64-bit wrap uses the unsigned tick delta. Sources
must be sampled within one full counter period. Narrow Timestamp counters,
zero/unknown rates, unavailable protocols and failed readings return explicit
errors. `/caps` reports the selected source and nominal resolution. CPU-time
clocks remain unavailable.
`run_guest` supplies host SystemTime and Instant for realtime and monotonic
clocks, and sleeps briefly when all processes wait. Missing or failed clock
providers return explicit errors rather than zero timestamps.

`guests/include/efi_process.h` declares the C process imports.
`guests/lib/process.c` packs argv into caller-provided scratch memory and
starts new instances. `scripts/build_wasm_c_fixture.ps1` requires a separate
Windows WASI SDK via `-WasiSdk` or `WASI_SDK_PATH`. It builds the C capture
fixture with freestanding Clang and wasm-ld from that SDK.

`waitpid` accepts one direct child and suspends the parent until that child
exits. It writes a signed 32-bit exit code in little-endian form, then reaps the
child. This value is a direct code, not a POSIX packed wait status. A guest trap
maps to code 128 for this call and remains a separate `Trapped` host status.
An invalid child returns ECHILD (-10). An invalid status pointer returns
EFAULT (-14) without reaping. Exiting and cancelled parents leave their children
running, reparented to the host (PID 0). The host must reap those children.

`waitpid_nohang` applies the same checks and reaping rules. A live child returns
zero without writing status or blocking the caller. The syscall yields to the
dispatcher, allowing command event loops to check exits and drain other tasks.

## Restricted QuickJS port

QuickJS 2026-06-04 is built from the official source archive
https://bellard.org/quickjs/quickjs-2026-06-04.tar.xz. The Windows build uses
WASI SDK 34 with an explicit sysroot. No MSYS2 compiler is invoked.

```powershell
./scripts/build_quickjs.ps1 -WasiSdk artifacts/toolchains/wasi-sdk-34.0-x86_64-windows
./scripts/build_wasm_c_fixture.ps1 -WasiSdk artifacts/toolchains/wasi-sdk-34.0-x86_64-windows
cargo.exe build -p efi-agent-wasm --example run_guest
uv.exe run scripts/smoke_quickjs.py --runner target/debug/examples/run_guest.exe --quickjs artifacts/wasm-guests/quickjs.wasm --probe artifacts/wasm-fixtures/ash_probe.wasm
```

The build copies and patches engine sources under the output directory:
disable threaded Atomics for WASI, include the allocator declaration, and
remove dtoa's unused setjmp header. It links quickjs, dtoa, libregexp,
libunicode and cutils. It does not link quickjs-libc. The output includes
the upstream license. Run `quickjs -e SCRIPT`; top-level await is supported.
The engine has a 32 MiB allocation limit and a 512 KiB JS stack limit.

`$` is a tagged command template. Each substituted value is single-quoted;
arrays expand to distinct quoted words. Empty strings, single quotes and
Unicode are preserved. NUL is rejected. Literal segments remain shell syntax.
The native `__shell` function starts `ash -c COMMAND` as a fresh process with
empty stdin and separate capture pipes. It returns a Promise. Success yields
`{exitCode, stdout, stderr, captureError}`; nonzero exit or capture failure
rejects with the same result. Outputs are JS strings, each limited to 512 KiB.
Excess bytes are drained and the child is reaped, then capture reports failure.
`print` supplies diagnostic output. No std/os modules, module loader, Node
process/require, filesystem API or network API is installed.

`guests/lib/shell.c` is the C capture layer for QuickJS.
It polls both output streams for all active commands and uses
nonblocking wait to collect exits. This prevents stderr-before-stdout
backpressure and early stream closure from blocking other commands.

The command ABI smoke runs the actual QuickJS engine with `ash_probe.wasm`
registered temporarily as `ash`. The probe checks the exact command bytes;
it does not parse shell syntax. This proves quoting and process integration,
not BusyBox ash execution. `scripts/smoke_ash.py --quickjs` separately verifies
the real ash module, concurrent shell capture, parameter quoting and exit
rejection. The firmware smoke accepts `--ash` to run the real shell too.

## CPython

The port uses Brett Cannon's prebuilt CPython 3.14.7 WASI SDK 24 distribution,
linked from the PSF webassembly project. This is an upstream maintainer build.
The archive is `python-3.14.7-wasi_sdk-24.zip` from the
`brettcannon/cpython-wasi-build` GitHub release `v3.14.7`.
Run `./scripts/prepare_cpython.ps1`. Use `-Archive PATH` for a local archive,
or `-Output PATH` to select the output directory. Preparation copies the binary
unchanged and includes the full upstream `lib/python3.14` and license.
It requires no CPython source tree, compiler, WASI SDK, MSYS2 or build Python.

Copy both `python.wasm` and `lib` into the workspace. The upstream CLI supports
`python -c CODE`, `python script.py`, and `python -m MODULE`.
Standard `open`, FileIO and filesystem imports use that workspace.
`lib/python3.14/site-packages/sitecustomize.py` installs the private Python
adapter `_efi_process.py` at normal interpreter startup. Adapter failure exits
with an error. `-S` disables site initialization and therefore disables the
adapter. The packaged standard library and `subprocess.py` remain unchanged.
`-I` retains site initialization and the installed adapter.
The adapter disables bytecode writes. Dynamic extensions and native threads
remain unavailable. `_socket` and `_decimal` are present in the prebuilt binary;
socket creation still lacks a runtime backend and reports an error.

Use standard `subprocess.run`, `check_output`, `Popen` and `os.system`.
The startup adapter patches the process hooks used by upstream `subprocess.py`
and `os.system`. It uses ordinary WASI open/read/write on the internal
`.efi-process` control endpoint to spawn fresh instances, map descriptors and
wait cooperatively. That endpoint is process-local and cannot be inherited.
Application code uses standard Python APIs. `shell=True` selects registered `ash`; argument
lists with `shell=False` preserve individual words. Standard return codes,
`CalledProcessError`, binary capture, text encoding/error policies, environment
overrides, pipes, file redirections and `pass_fds` are supported. Python capture
has no separate 512 KiB limit; available process memory still bounds it.

For example, put this in `workspace/demo.py`, register `ash` and `python`,
then run `/wasm run python demo.py`:

```python
import subprocess

result = subprocess.run("echo hello", shell=True, capture_output=True, text=True, check=True)
print(result.stdout, end="")
```

Executables must be registered. The backend maps `/bin/sh` to `ash`, and accepts
registry names with `/bin/`, `/usr/bin/` or `./` prefixes. Filesystem PATH
execution and a distinct custom `argv[0]` are unavailable. `cwd`, pre-exec
callbacks, sessions/groups, credentials and per-child umask changes raise `ENOTSUP`.
Ordinary children inherit the current directory and process umask. `os.chdir`
and `os.umask` update that state through the private startup adapter. Current
directory tracking is still path based and does not follow ancestor rename.
Subprocess timeouts and process signals still raise `ENOTSUP`; adapter support
for those operations remains pending. Python `time.sleep` and monotonic clocks
use the runtime clock provider. `subprocess.DEVNULL` uses the virtual `/dev/null` device.
Descriptor 3 is reserved for the workspace, and user descriptors end at 127.
See [the transparency audit](port-transparency-audit.md) for remaining gaps.

`scripts/test_cpython.py` runs the actual interpreter and ash through the
native wasmi runner. It checks file imports, standard CLI entry points, binary
and text capture, argument boundaries, exit errors, environments, concurrent
children, descriptor inheritance, duplex input/output and rejection before effects.
`--test METHOD` selects a test. Firmware integration uses
`scripts/smoke_wasm.py --python PATH --ash PATH`; it verifies saved FAT bytes
and two Python roots that wait for each other's Shell-created files.
`--memory` selects QEMU RAM in MiB (default 256).
`--command-timeout` selects the per-command wait in seconds (default 45).
The prebuilt distribution's verified TCG run used 512 MiB and 180 seconds.
Execution memory is released at exit; process slots and captured
output remain until reap.

## Nonlocal jumps for the BusyBox port

BusyBox 1.37.0 source is staged from
https://busybox.net/downloads/busybox-1.37.0.tar.bz2. Its ash implementation
uses setjmp/longjmp for nested error recovery, even with NOFORK enabled.
WASI SDK's native implementation needs Wasm exception handling, which the
current wasmi engine does not expose. The guest runtime under `guests/sjlj`
uses Binaryen 133 Asyncify to save and restore Wasm frames instead.

```powershell
./scripts/build_wasm_sjlj.ps1 -WasiSdk artifacts/toolchains/wasi-sdk-34.0-x86_64-windows -Binaryen artifacts/toolchains/binaryen-version_133
uv.exe run scripts/smoke_sjlj.py --runner target/debug/examples/run_guest.exe --directory artifacts/wasm-fixtures
```

The application calls through `efi_sjlj_run`. A setjmp capture unwinds to
that driver, saves the frame bytes and C stack pointer, then rewinds and
returns zero. Longjmp abandons the current frames and restores the selected
capture. Zero jump values become one. The capture saves local Wasm values;
it does not roll back linear memory or replay entry side effects. Volatile
C stack objects therefore retain modifications made after capture.
Driver and capture functions have explicit noinline boundaries. The build
disables sibling-call optimization and transforms the module after linking.
Regular process imports keep wasmi's existing resumable continuations; they
do not initiate Asyncify unwinds. No Asyncify support is added to the host.

Each instance has its own runtime state, up to 64 saved environments and a
1 MiB unwind buffer. Shallower captures and longjmp discard deeper saved
frames. Invalid targets and resource exhaustion trap. The runtime currently
has fixture and real ash syntax-error verification. Maximum-depth/resource
verification remains pending. Jumping to a returned C function is invalid.
Signal-mask aliases do not supply asynchronous POSIX signals.

## Full objective and pending work

The real BusyBox ash translation unit now links into an Asyncify WASM module:

```powershell
./scripts/check_busybox.ps1 -WasiSdk artifacts/toolchains/wasi-sdk-34.0-x86_64-windows
./scripts/build_busybox.ps1 -WasiSdk artifacts/toolchains/wasi-sdk-34.0-x86_64-windows -Binaryen artifacts/toolchains/binaryen-version_133
uv.exe run scripts/smoke_ash.py --runner target/debug/examples/run_guest.exe --directory artifacts/wasm-guests --quickjs
uv.exe run scripts/smoke_ash_exec.py --runner target/debug/examples/run_guest.exe --directory artifacts/wasm-fixtures
```

`prepare_busybox.py` generates the initial ash feature configuration and
copies the upstream source/license to the output directory. The initial
configuration enables aliases, getopts, command, internal glob, echo and NOFORK. The full
applet set and arithmetic/builtin configurations still need integration.
The port header declares the WASI and BusyBox utility boundaries. Several
declared utilities remain unimplemented. Real ash echo, quoted expansion,
functions, loops, conditions, exit status and syntax errors pass in host wasmi.
Real ash echo and concurrent QuickJS shell capture pass in QEMU/OVMF.
File operations and redirections now use the provider described below.
Valid stream descriptors return NOTDIR for path operations; missing descriptors
return BADF. Unsupported fd flag changes return NOTSUP.

The generated source replaces direct fork with fresh shell tasks implemented
in `guests/busybox/tasks.inc`.
Error helpers use noinline boundaries rather than C noreturn promises, since
Asyncify must return through those helpers while unwinding. Explicit temporary
return values keep that unwind path defined. The completed terminal program
bridge in `guests/busybox/exec.c` sends argv and the shell's explicit environment
to a new registered instance, waits/reaps it and exits with its status. Spawn
failures return mapped WASI-libc errno values to ash. The registry currently
selects programs by argv[0]; filesystem PATH resolution remains pending.
Ordinary external commands now call `efi_ash_run` and wait/reap without leaving
the parent shell. The exec builtin exits after the fresh child finishes. Neither
path replaces instance memory. Shell nodes use the separate task snapshot path.

`efi_process.program_exists(NAME, LENGTH)` queries the live registry, yields to
the dispatcher and returns 1 or 0 without creating a process. Length excludes
NUL. Invalid UTF-8, embedded NUL or empty names return EINVAL; invalid memory
returns EFAULT and names over 64 KiB return E2BIG. Ash checks builtins and
functions before this query. Registered commands use exact names independently
of filesystem PATH. `command` can bypass a shell function. This registry dispatch
does not replace the pending filesystem PATH implementation.

The shell sends only exported and command-local variables to the child. Local
assignments override exports; repeated assignments use the last value. The
parent's variables are restored when the command ends. Spawn errors map to shell
status 127 for missing programs and 126 for other failures. Direct guest i32
exit codes are reduced to eight-bit shell statuses. The parent continues after
ordinary commands; `set -e` and `exec` retain shell exit behavior.

```powershell
uv.exe run scripts/smoke_ash_external.py --runner target/debug/examples/run_guest.exe --ash artifacts/wasm-guests/ash.wasm --quickjs artifacts/wasm-guests/quickjs.wasm --child artifacts/wasm-fixtures/ash_child.wasm
uv.exe run scripts/smoke_ash_tasks.py --runner target/debug/examples/run_guest.exe --ash artifacts/wasm-guests/ash.wasm --quickjs artifacts/wasm-guests/quickjs.wasm --child artifacts/wasm-fixtures/ash_child.wasm
```

Shell tasks transfer typed node fields, length-prefixed strings and shell state,
with no pointers or memory cloning. The complete binary state is limited to
512 KiB and a node traversal depth of 256. A read-only snapshot stream carries
it on an available child descriptor >=4, with decimal byte length and descriptor
in the private `ash --efi-task SIZE FD` invocation. User fd 4 remains available.
This is independent of the 64 KiB argv limit.
The parent closes its source after spawn; the child verifies the exact length
and EOF, then closes the state descriptor before decoding/evaluation. Selected stdin remains
independent of the state stream. Hex encoding is removed. The
decoder rejects truncated, malformed, oversized and unknown-version states.
The Wasm engine allows at most 4096 call frames; this leaves room for Asyncify
helpers around the guest depth check. Each interpreter value stack is limited
to 8 MiB and grows as needed.

The snapshot includes variables and their export/readonly/unset flags,
functions, aliases, shell options, $0 and positionals, getopts cursor, $$,
previous exit status and $!. Asynchronous signal delivery and terminal job
control are unavailable. Task instances have new process PIDs and separate
memories, while the shell's $$ value follows subshell semantics. Exit traps
are not inherited. Filesystem working-directory state remains pending.

Pipelines start every stage with selected standard descriptors before waiting
for completion. The parent closes unused pipe endpoints. Job waiting polls
known children cooperatively and stores packed shell wait statuses. Command
substitution reads its pipe to EOF before waiting, and upstream ash performs
trailing-newline removal and field splitting. It clears errexit in the child,
matching this upstream ash configuration. Tested conditions preserve EV_TESTED.
Background tasks support $! and wait. User descriptors cross task boundaries
through explicit maps, and large here-documents use the bounded binary snapshot.
Full failure cleanup and background stdin isolation remain pending.

The build extracts the upstream `shell_builtin_read` implementation with its
license header. WASI read and poll supply cooperative input; upstream code
retains IFS splitting, escaping, continuation and EOF assignment behavior.
`-r`, `-n`, `-d` and `-u` are supported. Invalid readiness descriptors fail.
Positive `-t` timeouts use the installed monotonic source on both the native
runner and firmware. Unavailable sources return an explicit clock error.
`-s` fails because guest terminal echo control is unavailable. Prompt display
continues to require isatty. No termios success is fabricated.

```powershell
uv.exe run scripts/smoke_ash_read.py --runner target/debug/examples/run_guest.exe --ash artifacts/wasm-guests/ash.wasm --quickjs artifacts/wasm-guests/quickjs.wasm --child artifacts/wasm-fixtures/ash_child.wasm
```

The build also links upstream `coreutils/printf.c` and `libbb/bb_strtonum.c`.
The printf builtin supports repeated formats, dynamic width/precision, signed
and unsigned 64-bit conversions, floating-point output and byte escapes.
Invalid numbers and formats report errors. A missing integer argument prints
zero and returns status 1 in this BusyBox version. Output retains NUL bytes
through WASI pipes and QuickJS command capture.

```powershell
uv.exe run scripts/smoke_ash_printf.py --runner target/debug/examples/run_guest.exe --ash artifacts/wasm-guests/ash.wasm --quickjs artifacts/wasm-guests/quickjs.wasm --child artifacts/wasm-fixtures/ash_child.wasm
```

Add `--ash-printf` to the firmware smoke with `--ash-read` to check integer
boundaries and QuickJS binary capture in actual QEMU/OVMF.

## Guest files

`World::set_filesystem` installs a provider before the first process starts.
Each process receives fd 3 as a preopened directory named `/`. The firmware
provider opens files on the boot volume inside the configured workspace. Without
model configuration, its workspace is `\work`. The host example uses its supplied
program directory as the file root. Providers retain actual open file objects.
Descriptor duplicates share the open-file cursor; separate opens do not.

The imports include `fd_prestat_get`, `fd_prestat_dir_name`, `path_open`,
`path_filestat_get`, `fd_filestat_get`, `fd_tell`, `fd_sync` and `fd_datasync`.
Existing read/write/seek/poll imports also handle regular files. Open supports
create, exclusive create, truncate, append and directory handles. Paths are
relative to a directory descriptor. Each intermediate component must exist and
be a directory, including components before `..`. Absolute paths at the root
preopen use the workspace root. Backslashes, NULs and traversal above the
supplied directory return errors. The namespace resolves symlinks and rejects
loops. WASI rights constrain each operation; rights not applicable to the
opened file type are removed from its descriptor. File readiness reports the
bytes remaining at the current cursor and remains ready at EOF.

The firmware implements these calls through EFI_FILE_PROTOCOL. File operations
are synchronous. Reads return at most 512 KiB per call. File writes validate
all iovecs first, then transfer 64 KiB chunks without a 512 KiB call limit.
Short writes return the completed prefix; errors after progress also return
that prefix. An error before progress leaves the result pointer unchanged.
Append seeks
to the current file end before every write. Exclusive create checks existence
before Open; no other guest can run between these steps in the single dispatcher.
The host provider uses native file handles.
`fd_filestat_set_size` requires both resize rights and a writable file
descriptor. It preserves the shared cursor on shrink and growth, including
positions beyond the new EOF. Reads there return zero bytes. New bytes are
zero; shrinking then growing cannot expose previous file contents. The size
limit is 48 MiB. Larger sizes fail with FBIG before mutation. Append descriptors
can resize, retain their cursor, and seek to the new EOF on their next write.
Firmware growth synchronously writes zero chunks because EFI allocation does
not guarantee zero contents. A physical write failure can leave partial growth.
The actual libc fixture checks duplicate cursors, rights, negative/oversized
sizes and 71 structured shrink/grow/overwrite operations. The integration
driver compares saved file bytes with an independently computed bytearray.

`path_create_directory` creates a single directory on both providers. Missing
parents, existing entries, non-directory components and capability violations
return distinct errors. The model `write` tool creates missing parents.
Rename/removal, timestamps and symlinks use the shared persistent namespace
on native and firmware targets. Normal Python imports, py_compile and compileall
can write bytecode caches; `-B` retains its standard meaning. Filesystem PATH
and complete working-directory state remain pending.
Finite input and captured output use WASI UNKNOWN stream type. They are not
terminals. Unchanged CPython `python -` executes redirected scripts and empty
stdin without starting a REPL.

WASI Preview 1 does not encode POSIX mode bits. The private `.efi-process`
endpoint supplies path/fd mode get/set, process umask, inherited cwd and a
directory-fd name snapshot. A scoped creation-mode setting carries the mode
omitted by Preview 1; clients restore it after success or failure. It applies
only to new entries, including targets created through dangling links, and is
not inherited by child processes. Metadata replies retain positive WASI errno values;
process replies retain negative POSIX errors. Snapshot descriptors are private
and close on spawn. BusyBox libc wrappers and Python's private startup adapter
use these operations behind standard stat/chmod/open/mkdir/scandir methods.
Creation applies the process umask; changing a mode affects later opens and
directory traversal while existing file handles retain their granted access.
The single guest owner/group is uid/gid 1. Symlink mode mutation without
following the link returns ENOTSUP. Python scandir entries preserve methods,
bytes paths and fd lifetime, but their private proxy type differs from DirEntry.

Before model code runs, the firmware discovers root workspace `.wasm` files.
It validates command names, binary headers and the 48 MiB module size bound.
Discovered names participate in ash's program lookup before compilation. First
spawn reads and compiles the module; later spawns reuse it. The `qjs` alias
shares `quickjs.wasm`'s compiled module. Explicit registrations take precedence.
Filesystem PATH entries and a persistent installation manifest remain pending.

The `efi_process.dupfd(source, minimum)` import returns the lowest free descriptor
at or above the minimum, or a negative POSIX errno. Ash uses it for saved
redirection descriptors. The CLOEXEC duplication variant marks these internal
copies, so they are omitted from fresh tasks and external commands. Internal
pipe ends are marked too; tasks explicitly map the needed ends as stdio, so
unrelated stages cannot retain writers and prevent EOF. Open user descriptors
retain their numbers and shared cursors in subshells, pipelines, substitutions
and external commands. The filesystem preopen and temporary task snapshot also
enter a fresh task. The snapshot is closed before evaluation.
scripts/test_ash_descriptors.py verifies ten native cases, including fd 4, child
close isolation, inherited output, absent stdin and 300 repeated cycles.
Add --ash-descriptors to the firmware driver to verify independent FAT bytes.
The full 512 MiB application run artifacts/wasm-applications-descriptors-512
passes these cases alongside HTTP/TLS/DNS/WolfSSH, both languages, arithmetic,
binary tasks, large here-documents and process/pipe cancellation regressions.

```powershell
uv.exe run scripts/smoke_wasm_filesystem.py --runner target/debug/examples/run_guest.exe --fixture artifacts/wasm-fixtures/filesystem.wasm --directory-fixture artifacts/wasm-fixtures/directory.wasm --ash artifacts/wasm-guests/ash.wasm --quickjs artifacts/wasm-guests/quickjs.wasm
```

Add `--filesystem` to the firmware smoke to run the actual libc fixture and
optional ash/QuickJS file cases. The smoke reads the FAT image after shutdown
and compares file contents with an independent byte oracle.

`fd_readdir` enumerates real host or EFI directory entries. Each traversal
retains a name-sorted list with at most 4096 entries and 512 KiB of encoded
records, including 24-byte WASI dirent headers. Providers include dot and
dot-dot entries. Names use UTF-8; inode identifiers are unavailable and zero.
The dirent type identifies directories, regular files and host symlinks.
Opening or following a symlink is still unsupported.

Cookie zero starts a fresh traversal and sees current directory contents.
Nonzero cookies select an entry in the retained list. Duplicated descriptors
share that list; separately opened directories retain independent lists.
A cookie beyond the list, or a nonzero cookie without an active traversal,
returns INVAL. A short buffer can contain a partial header or name; the caller
retries from the previous complete entry's cookie with a larger buffer.
An empty buffer succeeds with zero bytes and does not start a traversal.
Guest buffer and count pointers are checked before enumeration or writes.
NONBLOCK is supported for directories because enumeration never waits for
new entries; fdstat retains the requested flag.

The directory fixture uses raw short buffers, duplicate descriptors, creation
during traversal and the actual libc directory functions. Seeded names cover
Unicode, spaces, punctuation, hidden entries and long records. Artifact checks
compare the complete name/type sets against independent host inputs. Actual
ash glob checks compare sorted results and exclude hidden names. Add
`--directories --filesystem` to the firmware smoke for these cases and FAT
artifact checks, including real QuickJS capture of an ash glob.

## Cooperative TCP provider and initial ABI

`network::Network` starts an IPv4 TCP attempt and returns an owned connection.
Every connection method must return without waiting: poll_connect, read, write
and readiness return None while pending. Providers own native sockets or stable
firmware token/buffer storage. Dropping the last descriptor must close the
connection and retire pending operations before freeing their storage.

`World::set_network` installs the provider before spawning. The new import
`efi_net.tcp_connect(address_ptr, port, descriptor_ptr) -> i32` takes four IPv4
octets and an unsigned host-order port from 1 to 65535. It returns a positive
WASI errno on failure, zero on success, and publishes the connected descriptor
only on success. Invalid ranges, ports and exhausted descriptor slots fail
before starting a network attempt. Pending connects retain a continuation in
`Status::Connecting`. A failed attempt closes its reserved descriptor and
permits a retry. Cancellation drops the connection with the execution Store.
The guest declaration is in guests/include/efi_net.h.

Connected descriptors use existing fd_read, fd_write, poll_oneoff, fd_close,
dup2 and renumber. Duplicates share one connection; only the last close destroys
it. fdstat reports SOCKET_STREAM. Reads/writes may return partial counts and
suspend on backpressure. Readiness does not consume bytes; EOF produces a read
event with hangup. Network failures map to WASI errno values.

The native runner installs a mio backend with nonblocking sockets and zero-time
event polls. Actual Windows TCP tests verify asymmetric request/response bytes,
readiness, EOF, duplicate lifetime, refusal and concurrent child progress.
scripts/test_network.py uses the libc fixture network.wasm. Its server waits
for a guest child's saved file before responding; the parent must remain
suspended without blocking the child. Direct saved-byte checks verify the
response file. A separate native test cancels a blocked read and observes EOF
at the real server while another instance completes. ABI tests cover output
bounds, port bounds, descriptor exhaustion and pending-connect cancellation.

UEFI installs the provider in tcp/cooperative.rs. Firmware configuration supplies
optional static IPv4; otherwise it requests DHCP policy and retries TCP mapping
on each poll. Connection mapping/handshake has a 30-second deadline. Independent
TCP children share the NIC service binding with GET_PROTOCOL access. Queued
tokens, packets and byte buffers are boxed before firmware receives their
addresses; they stay owned across guest suspension. Receive caches at most
64 KiB. Writes accept a copied prefix of at most 64 KiB and report that prefix
once; later writes wait for its completion. Writable polling collects completed
transmits. Read polling queues a receive without consuming bytes. Last close
requests an abortive close, resets TCP4 synchronously and verifies completion
before dropping tokens or closing the child protocol. This close can discard
accepted bytes still in flight. Use the explicit normal-close import below
when accepted bytes must be transmitted before closing.

`efi_net.tcp_close(fd) -> i32` waits for accepted sends, then normally closes
the shared connection. It returns a positive WASI errno or zero, consumes the
selected descriptor on completion or transport error, and suspends in
`Status::Closing(fd)` while pending. Invalid/non-socket descriptors return
BADF/NOTSOCK without effects. All duplicates refer to the closed connection
and return NOTCONN on later data operations; they still require fd_close.
Firmware uses a boxed normal close token and a 30-second deadline. Cancellation
or timeout resets and retires it before freeing storage. This is a full close;
half-shutdown is not implemented. Native TCP and real firmware tests verify
the complete payload before server EOF, duplicate semantics and close progress.

`scripts/smoke_wasm.py --network` configures an e1000 NIC and static QEMU NAT
IPv4, starts a local server, runs the actual libc fixture through TCP4, and
checks saved response bytes in the FAT image. The server replies only after a
child's console marker arrives, which verifies child and UI progress while the
parent waits for TCP. `/wasm run` redraws new guest output during polling.
Socket `fd_fdstat_set_flags` accepts zero or NONBLOCK (4). Duplicates share
the flag and fd_fdstat_get reports it, with FD_FDSTAT_SET_FLAGS rights. Unknown
bits return INVAL; other known flags return NOTSUP without changing the value.
Pending reads and backpressured writes return AGAIN when NONBLOCK is set,
preserving buffers and byte-count output. Zero-byte operations succeed and
peer EOF returns a successful zero-byte read. Blocking mode retains the
cooperative continuation. poll_oneoff waits for readiness in either mode.
The convenience connection import still suspends until connected. Separate
socket creation and connection imports also support nonblocking establishment.

`efi_net.tcp_open(flags, descriptor_ptr) -> i32` allocates an unconnected IPv4
stream descriptor. Flags combine WASI NONBLOCK (4) and EFI_NET_CLOEXEC
(0x10000). Invalid output ranges,
flags or a full table leave output unchanged. No network attempt starts here.
`efi_net.tcp_connect_fd(fd, address_ptr, port) -> i32` starts an attempt on that
socket. Blocking mode suspends in Connecting; NONBLOCK returns INPROGRESS
(26) while pending. An immediate success returns zero. Repeated pending and
connected attempts return ALREADY (7) and ISCONN (30). Validation precedes
network effects. All duplicates share the connection state and flags.

Write poll readiness advances pending connects and reports success or the
transport error. `efi_net.tcp_error(fd, error_ptr) -> i32` publishes and clears
the shared pending connection error, corresponding to SO_ERROR. It can advance
a pending attempt; zero while pending does not establish success. Invalid
output memory neither progresses the attempt nor clears its error. Connect
failure immediately releases the transport but retains the socket descriptor.
Clearing the error leaves data operations NOTCONN. A new connect can reuse a
failed socket; closing or cancelling drops all shared state at last ownership.
The convenience tcp_connect import still removes its reserved descriptor on
failure.

## Guest POSIX networking

WASI Preview 1 guest ports can put `guests/posix/include` before the SDK include
directories and link `guests/posix/network.c` with `guests/include`. This layer
adds socket, connect, getsockopt, send, recv, getaddrinfo, freeaddrinfo and
gai_strerror. It uses the SDK sockaddr layouts, byte-order helpers, errno,
fcntl, poll and close. Domain failures return -1 with errno for socket calls,
or EAI values for resolver calls. TCP ports and IPv4 addresses use network
byte order in sockaddr_in. Host-import errno values are converted to the SDK's
WASI errno values directly.

Only AF_INET/SOCK_STREAM and protocol zero/IPPROTO_TCP are supported. Socket
creation accepts SOCK_NONBLOCK and SOCK_CLOEXEC. getsockopt
supports SOL_SOCKET/SO_ERROR and SO_TYPE. Short option buffers fail before
clearing the shared error. send supports zero flags or MSG_NOSIGNAL; recv
supports zero flags. Unsupported flags and non-socket descriptors fail.
Nonempty NULL buffers return EFAULT. Data uses cooperative WASI read/write;
the SDK fcntl changes shared NONBLOCK and poll waits for connect/data readiness.
Setsockopt, address queries, listen/accept, half-shutdown and recv peek/waitall
remain required for wider socket support.

getaddrinfo returns one allocated IPv4 TCP address. Numeric hosts avoid DNS;
ASCII names use the cooperative resolver. Service strings must be decimal
ports from 0 to 65535. A missing node selects loopback, or ANY with AI_PASSIVE.
AF_UNSPEC/AF_INET, AI_NUMERICHOST and AI_NUMERICSERV are supported. AI_CANONNAME
copies numeric input; for hostnames it returns EAI_BADFLAGS because the
address-only DNS ABI does not publish canonical CNAME targets. Unknown flags,
families, protocols and service names fail explicitly. NXDOMAIN maps to
EAI_NONAME, malformed DNS to EAI_FAIL, and temporary/timeout failures to
EAI_AGAIN. Other provider failures return EAI_SYSTEM with errno.

The fixture builder includes posix_network.wasm. Run scripts/test_posix_network.py
with --runner and --fixture for actual Windows socket and DNS checks. Firmware
checks use `scripts/smoke_wasm.py --network --dns --posix-network`. The bound
non-listening host port's expected NAT error is configurable with
`--posix-connect-error timeout|refused`; the tested Windows QEMU NAT sends no
RST for this port and times out. A separate native test requires ECONNREFUSED.
Firmware runs also save network.pcap for independent packet inspection.

Half-shutdown, POSIX guest integration,
curl and WolfSSH builds and further firmware network checks remain required.

## Guest DNS

`efi_net.resolve_ipv4(name_ptr, name_len, address_ptr) -> i32` resolves an
ASCII hostname to one IPv4 address. It returns zero or a positive WASI errno
and preserves all four output bytes on failure. Input and output memory ranges
are checked before random generation or networking. Names are lowercase after
normalization; one trailing dot is accepted. Labels must contain ASCII letters,
digits or internal hyphens, with 63-byte label and 253-byte total limits.
IDNA, IPv6, search domains, caching and DNSSEC validation are not implemented.

`World::set_dns_server` selects an IPv4 TCP DNS server before processes start.
Firmware uses CONFIG.JSON dns_address/dns_port, defaulting to 1.1.1.1:53.
The native runner accepts `EFI_WASM_DNS=address:port` with the same default.
Queries need the network, realtime clock and cryptographic random providers.
Each query uses a fresh random transaction ID and its own TCP connection.
It suspends in `Status::Resolving`; connection, partial writes and framed reads
advance through dispatcher polls. The deadline is 10 seconds from submission,
measured by the realtime provider. Changes to wall time can affect that deadline.

Responses must match the transaction, question, type and class. Truncated
messages, invalid compression, unrelated answers and conflicting CNAME data
are rejected. CNAME traversal is limited to 16 visited names and records to
1024. NXDOMAIN or a missing matching A record returns NOENT; SERVFAIL returns
AGAIN. Completion, failure and cancellation release the query connection.
No retry, UDP fallback or provider call replay is performed.

`scripts/test_dns.py` uses an independent Windows TCP server and actual guest
libc calls. `scripts/smoke_wasm.py --network --dns` runs those checks through
actual UEFI TCP4 in QEMU. Both cover A/CNAME, NXDOMAIN, wrong transaction IDs,
wrong questions and invalid compression; the server sends the frame one byte
at a time. These checks establish the DNS ABI, not a curl or WolfSSH port.

## Guest random source

`World::set_random` installs a cryptographic byte provider before processes
start. All root and child processes retain the same provider. The WASI
`random_get` import validates the memory range before calling it. Empty valid
ranges succeed without a provider. Nonempty calls without an available source
return NOTSUP; I/O failures return IO. Requests above 512 KiB return INVAL.
Generation is synchronous and bounded. Bytes are staged and copied only after
complete success, so a provider failure cannot publish partial output.

The firmware uses the existing application-wide RDRAND-seeded ChaCha20
generator. It collects one 32-byte seed on first use and retains its state
across calls and processes. Missing RDRAND or seed collection failure returns
NOTSUP. There is no predictable or firmware RNG fallback. The native example
uses `getrandom` with the OS source.

The actual libc fixture tests `getentropy`, its 256-byte limit, guarded output
and repeated requests. The integration driver reads saved records from FAT;
different records check for repeated output, not cryptographic quality. Add
`--random` to the firmware integration driver. Add `--no-rdrand` to verify
failure with the CPU feature disabled. The host file integration driver accepts
`--random-fixture artifacts/wasm-fixtures/random.wasm`.

## Actual curl HTTP port

`scripts/build_curl.ps1` builds upstream curl 8.22.0 with Windows WASI SDK 34.
MSYS2 provides configure and make only. The client uses the guest POSIX IPv4
TCP layer and wolfSSL 5.9.4 for TLS. Threaded resolution, socketpair wakeup,
IPv6, FTP, TFTP and NTLM are disabled. Advertised other protocols are not verified.
The build guards signal/setjmp headers when the configured functions are absent
and builds only the library and command targets. Configuration changes clean
objects because the upstream dependency files do not track the config header.
The external POSIX archive always causes a command relink.

`efi_net.tcp_open` accepts WASI NONBLOCK (4) and EFI_NET_CLOEXEC (0x10000).
NONBLOCK is shared socket state; close-on-exec belongs to each descriptor.
Default spawn omits marked standard descriptors. Explicit spawn_fds mappings,
dup2 to another fd and dupfd clear the copy's flag. Same-fd dup2 and WASI
renumber preserve it. Explicit spawn_mapped_fds also clears the copy's flag
and permits additional selected descriptors.

`scripts/test_curl.py` runs the real CLI against an independent HTTP server.
`scripts/smoke_wasm.py --network --curl artifacts/wasm-guests/curl.wasm` runs
the same byte oracle through actual UEFI TCP4. Both verify redirect and chunked
131,089-byte downloads and a 196,613-byte upload. Firmware checks read saved
FAT files after QEMU exits. Native binary stdout explicitly uses `--output -`
because curl protects character-device output from implicit binary writes.
Actual native and firmware curl DNS checks also verify A/CNAME HTTP downloads
and refusal of NXDOMAIN, wrong IDs/questions and cyclic compression. DNS answers
use independent local servers; successful files match the full byte oracle.
The combined firmware driver reaps each checked DNS/curl root after validating
its exit status so completed metadata does not exhaust the process table.
The actual native CLI also verifies `--max-time` on a held response and
server-observed connection closure. The firmware integration holds a real
curl response, runs another WASM instance, checks real QuickJS-to-ash-to-curl
and CPython subprocess-to-ash-to-curl uploads, then cancels the held curl.
The independent server verifies every upload byte and the callers verify its
SHA256 response. The held connection must remain open through peer/application
progress and close after cancellation. These checks exercise named applications
through the cooperative process/pipe/network interfaces without fork.
`scripts/build_wolfssl.ps1` builds the portable static crypto/TLS library with
Windows CMake/Ninja and WASI SDK. Run it before the curl builder. Client and
library share `guests/wolfssl/user_settings.h`; the generated options header
is disabled so it cannot reset the selected ABI/crypto features. Entropy comes
from WASI random_get, and failures become RNG_FAILURE_E. Single-thread user
I/O uses curl's BIO callbacks over guest socket descriptors. TLS 1.0/1.1,
RC4 and 3DES are excluded. The build preserves upstream COPYING.

Native Release and real firmware tests verify TLS 1.2 and TLS 1.3 negotiated
versions on an independent OpenSSL server and compare every response byte.
`--cacert` loads a guest PEM trust file; no default trust bundle is packaged.
Untrusted signatures, absent CA issuers, wrong IP SAN and expired certificates
return curl error 60 before HTTP. The staged upstream backend patch classifies
signature and issuer failures as peer verification errors; it does not bypass
verification. UEFI tests with RDRAND disabled return TLS error 35 and send no
HTTP request. There is no entropy fallback. Add `--curl-tls` to the firmware
driver; add `--random --no-rdrand` for that negative path.

WolfSSL certificate path handling also imports WASI path_readlink. Directory
rights now allow that read-only operation. It validates rights, output/path
ranges and traversal before inspecting metadata. Missing/provider errors are
retained; existing non-links return INVAL without writing output. Providers
reject symbolic-link access/traversal. The boundary test covers denied rights,
missing paths, file/directory types, buffer edges and unchanged outputs.

`scripts/build_wolfssh.ps1 -WasiSdk <Windows SDK path>` builds upstream
WolfSSH 1.6.0 as a static WASI library and the `wolfssh.wasm` exec client.
Build wolfSSL first. This client uses the existing cooperative POSIX TCP
callbacks, one SSH session/exec channel, and no threads or native socket
implementation from WolfSSH. `WOLFSSH_SHELL` enables remote exit-status
handling; it does not add a local shell. The build preserves upstream
LICENSING and treats linker warnings as errors.

```sh
wolfssh --host 192.0.2.1 --port 22 --user user \
  --host-key-sha256 <64 hexadecimal digits> --password-file password.txt \
  -- 'remote command'
```

Host trust requires the SHA256 of the raw SSH public-key blob, encoded as
64 hexadecimal digits. It is the digest underlying OpenSSH's base64 SHA256
fingerprint. The callback compares the digest before authentication; no
accept-all callback or trust-on-first-use path exists. Password files are
limited to 4096 bytes, reject NUL, and remove one final LF or CRLF. Password
authentication is attempted once. The client supplies no stdin and sends
channel EOF after opening exec. It drains stdout and stderr independently,
replenishes their shared receive window, and waits for channel retirement
after peer EOF so a late exit status is retained. Missing/out-of-range status
or a local failure returns 255; valid remote status 0..255 is propagated.

wolfSSL CMake detects 128-bit integer support on WASI. Shared user settings
now declare the same feature to all consumers; otherwise WolfSSH declares
big-integer digit arguments as i32 while the archive implements i64. This
is an ABI requirement, not an optional algorithm choice.

`uv run scripts/test_wolfssh.py --runner <Release run_guest.exe>
--guest <wolfssh.wasm>` uses an independent AsyncSSH server. Native checks
cover 262181-byte stdout interleaved with 196613-byte stderr, stderr-only
window replenishment, remote nonzero status, exit status after EOF, missing
status, wrong pins before authentication, incorrect passwords, malformed
pins, and password-file boundaries.

The firmware UI skips its fixed 10 ms idle wait after a WASM time slice
makes progress. It still polls input and dispatches firmware events each
iteration. With no progress, it retains the idle wait. This lets background
cryptographic guests continue computation between network waits.

Add `--wolfssh <wolfssh.wasm>` to `scripts/smoke_wasm.py` with `--network`
and `--ash`. Actual QEMU/OVMF checks the same binary byte oracles from saved
FAT files, trust/authentication failures, late/missing status, and a held SSH
session while other ash/QuickJS/Python instances execute. `/wasm kill` must
close that session as observed by the independent server and reap status
130. `--random --no-rdrand` checks failure before authentication without an
entropy fallback. Results are in artifacts/wasm-wolfssh-combined and
artifacts/wasm-wolfssh-no-rdrand.

The complete curl/TLS/DNS/POSIX/WolfSSH/QuickJS/ash/CPython firmware combination
passes with `--memory 512`; see artifacts/wasm-applications-combined-512.
With 256 MiB, that complete combination fails when the second concurrent
Python root initializes. The separate WolfSSH combination passes at 256 MiB.
The minimum RAM for the complete set has not been measured.

The target is a single-threaded, cooperative UEFI world for CPython, BusyBox ash
with NOFORK, curl, WolfSSH, and QuickJS or QuickJS-NG. Shell commands must create
fresh instances without `fork`. Python uses standard `subprocess` and `os.system`
interfaces. QuickJS provides the requested zx-like command API.

The process ABI also provides `efi_process.input_snapshot(bytes, size)`.
It copies at most 512 KiB into an anonymous read-only stream and returns a
descriptor or negative POSIX errno. Duplicates and inherited descriptors
share consumption; exhaustion returns EOF immediately. It cannot be written
or sought. Source memory can be changed or released after the import returns.
Memory bounds and a free descriptor are checked before the copy is installed;
invalid requests do not consume descriptor slots.

Actual ash here-documents now use that stream after upstream expansion in
the current shell. No fork or background pipe writer is required, and input
can exceed pipe capacity. This preserves quoted literal text, unquoted
parameter/command expansion, `<<-` tab stripping, multiple redirection order,
and EOF. Native integration verifies byte-exact sizes on both sides of
4096/65536 boundaries and 196613 bytes, plus repeated descriptor cleanup and
expanded input over the 512 KiB cap. Run scripts/test_ash_heredocs.py with
--runner, --ash and --child. Actual QEMU verifies these same cases with
`--ash-heredocs --ash-child <ash_child.wasm>` and compares saved FAT output
and stderr bytes; see artifacts/wasm-ash-heredocs. Fresh shell task serialization
now uses mapped binary snapshots with a 512 KiB total-state cap. Native tests
also verify 30001/65537/196613-byte here-documents through subshells, pipelines,
command substitution and inherited functions, as well as 256 KiB variables,
inherited input cursor sharing, 300 repeated tasks and malformed binary states.
The complete 512 MiB
application combination with here-document cases enabled also passes in
artifacts/wasm-applications-heredocs-512.
The updated 26-case set and malformed binary task checks pass actual QEMU in
artifacts/wasm-binary-tasks-focused and the full application combination in
artifacts/wasm-applications-binary-tasks-512. The staged upstream printf source
keeps numeric-conversion errno separate from successful stdio output, preventing
false status 1 after large writes; output errors still use ash's ferror check.

The actual BusyBox 1.37.0 arithmetic module and test/[ builtin now build into
ash. Both use 64-bit integers; arithmetic enables base notation and explicit
signed wrapping (`-fwrapv`). The staged math source adds logical short-circuit
evaluation: every &&/|| stack entry saves its enclosing disabled state, then
restores it before combining operands. This preserves nested ternary/logical
branches and suppresses assignments and division in skipped branches.
The staged test error helper keeps an Asyncify unwind epilogue, as ash does;
syntax/number errors return test status 2 and permit subsequent shell commands.
Guest supplementary groups are empty; no host identity database is exposed.

scripts/test_ash_math.py compares real command outputs with independent
integer and recursive tree oracles. It verifies precedence/base notation,
64-bit limits/wrapping, assignments, short-circuit side effects, loops,
test string/numeric/file cases, fresh task state, divide/syntax/recursion/
readonly errors, and 300 successive failing/successful test invocations.
Add --ash-math with --ash-child to the firmware driver to check saved FAT
output/error bytes. This feature does not enable Bash [[ syntax.
Actual QEMU verification passes all math/test oracles within the complete
512 MiB application regression in artifacts/wasm-applications-math-512.
The dedicated 512 MiB artifacts/wasm-ash-math-languages run also passes both
language interfaces: QuickJS concurrently captures the arithmetic loop and
a failing test command; CPython checks the same loop and captured nonzero status.
Both require the exact sum-of-squares output CALC:2870 and test status 2 with
the exact upstream diagnostic. QuickJS awaits the outer Promise so rejected
assertions propagate to the root's status.

The current code runs the process dispatcher and actual BusyBox ash, QuickJS,
CPython, curl and WolfSSH ports. The recorded full 512 MiB QEMU combination
passes, including application chains, network/TLS and concurrent processes.
The full 256 MiB combination fails during second concurrent Python startup.
The remaining scope is:

- Complete shared mounts, anchored cwd tracking and CPU-time clocks.
- Socket options, address queries, listen/accept, peek/waitall and half-shutdown.
- Reproducible source acquisition and packaging for the existing guest builds.
- BusyBox ash remaining builtins and filesystem PATH lookup.
- Complete shell task failure cleanup and nonlocal-jump resource verification.
- Further SSH operations beyond the pinned-host-key/password exec client.
- Remaining curl protocols and transport operations beyond the verified HTTP/TLS client.

Small WAT tests prove dispatcher behavior only. The named application claims
use separate native and actual QEMU checks recorded in docs/verification.md.
