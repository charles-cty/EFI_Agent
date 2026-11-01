# Rules

You are a senior software engineer working on this project, EFI Agent.

Do not edit README.md for your memory. Edit [the Current state section here](#current-state)
instead. Keep README.md concise and clean.

Maintain this document `./docs/verification.md` by yourself, too.

Keep `artifacts` for the documented build outputs and dependency inputs.
Put temporary verification files and staging packages in the host OS temporary
directory. Deliver updated packages at the standard paths before reporting
completion. Verification of a staging package does not verify those paths.

# Current state

The workspace limit is now 128 MiB and the full boot-tree limit is 160 MiB.
Images reserve a 192 MiB FAT partition and have a 194 MiB logical size.
Windows marks new images sparse before extending them; Linux retains the
existing hole-based allocation. Both standard images were rebuilt from their
standard trees. Their actual Windows allocations are approximately 59.75 MiB
(VM) and 57.19 MiB (native), with sparse attributes confirmed. Launcher compile
checks and the Release build pass. Verification for this update was limited
to compilation, formatting and delivered image properties/allocation; VM and
capacity-boundary suites were not run. Native config bytes are preserved.
Evidence is in docs/verification.md.

The standard build now includes jq 1.8.1 with bundled Oniguruma 6.9.10, built
from unchanged official release sources. Both standard trees and images contain
1,151 runtime files, 95 command aliases and 12 notices. Guest startup defaults
are configurable through EFI_AGENT_ENVIRONMENT or the config environment map;
PYTHON_BASIC_REPL=1 is the default. Children retain normal inheritance and
explicit replacement behavior. Prebuilt Python and all 537 upstream library
files retain their distribution bytes. The initial workspace/boot-tree limits
were 64/96 MiB, after actual Python cache writes exposed the old export limit.
Before the capacity expansion, all 272 host tests, host/UEFI Clippy, Release builds,
jq/environment/Python/session/installation integration and actual 256 MiB
Windows WHPX acceptance pass. The VM exported more than 48 MiB successfully;
all 1,151 declared saved files and both result files match expected bytes.
Standard packages were independently rechecked; native config bytes are
preserved. C:\Temp\work was not used or updated for this task. Full network
regression, Linux builds and hardware boot were not repeated. Evidence and
intermediate fixes are in docs/verification.md and jq details are in docs/jq.md.

Interactive ash now starts without its BusyBox banner or surrounding blank
lines. Native output and retained-session checks pass. Both standard trees
and images contain the rebuilt ash. After VM exit, C:\Temp\work\ash.wasm was
updated; all 1,150 other host files retain their bytes. Native configuration
bytes are preserved. No VM run was repeated for this change. Evidence and
intermediate issues are in docs/verification.md.

The second BusyBox extension installs 11 more commands, for 89 command names:
ed, ar, cpio, lzop, ps, pgrep, kill, pkill, vi, less and more. Process queries
read actual guest /proc records. Positive-PID signals 0/INT/KILL/TERM check or
terminate one process; children survive and are adopted. Signal handlers,
process groups and resource accounting remain unavailable. The shared terminal
supports raw keys, /dev/tty, nonblocking pipes, a bounded VT screen and window
dimensions. vi and less poll for size changes. Ctrl+C leaves vi insert mode;
Ctrl+Q cancels raw-terminal commands. Model tools retain captured output and
have no controlling terminal. Command details are in docs/busybox-applets.md.
Native Windows builds, Rust checks, Clippy, real WASM integration and actual
256 MiB WHPX terminal acceptance pass. The VM run covers editor saves/discard,
paging, pipe input, resizing, cancellation and process commands, with exact
exported host file bytes. Both standard trees and images match the current
1,150 runtime files, 89 names, EFI entries and ten notices. Native configuration
bytes are preserved. Evidence and intermediate fixes are in docs/verification.md.

QuickJS exposes the zx-style shell template as ash. The public dollar global
is absent. Model instructions, tool descriptions and maintained examples use
ash. Real session/REPL checks cover quoting, configured templates, output
iteration and reset/cancel behavior. All 256 host checks, host/UEFI Clippy and
Release builds pass. Both standard trees and images were refreshed and checked
against all 1,150 declared runtime files, EFI entries and ten notices. Native
configuration bytes are preserved. Actual Windows WHPX at 256 MiB passes new
BusyBox arithmetic/truncate, ash templates and independently checked saved
host bytes. This focused run does not repeat the full network/application
combination or native hardware boot. Evidence is in docs/verification.md.

The QuickJS ash adapter now follows zx's default output policy: command stderr
is reported by default, `quiet` suppresses it, and `verbose` reports the command
plus captured output on stderr. Output is emitted after bounded command capture
completes. The QuickJS module, both standard boot trees and both standard images
were refreshed. The live session integration suite passes the default, quiet and
verbose cases; no VM run was repeated.

The first BusyBox extension installed 78 command names, including 27 new
file/text, comparison, arithmetic, checksum and compression applets.
The full selection and deferred commands are in docs/busybox-applets.md.
The new integration suite and the existing text, gzip and mutation suites
pass with real WASM guests and independent saved-byte checks.

The 2026-11-01 writable vvfat trial failed acceptance on Windows WHPX.
Guest-successful writes did not reliably reach host filenames or lengths;
directory renames also failed in QEMU. The runtime proposal was withdrawn.
The VM still stages a private FAT disk and exports workspace changes on exit.
See docs/verification.md for evidence and the retained proposal location.

Dependency notices are prepared under artifacts/wasm-guests/licenses and
distributed under EFI/AGENT/LICENSES in both boot trees and disk images.
programs.json declares only command modules, aliases and libraries. Installation
and runtime discovery do not copy or require license files in the user workspace.
Both standard packages were rebuilt and independently checked on 2026-11-01:
each workspace contains 1,150 runtime files, with ten notices outside work.
Removed ten unchanged installed notices from C:\Temp\work after backing them
up in Windows temporary storage. Other file bytes remain unchanged. Focused
installation checks passed; no VM was started for this change.

Direct interpreter input is available through /enter (default: shell),
/enter shell and /enter js. These use the same ash/QuickJS state as model tools.
All direct text goes to the interpreter; application slash commands apply only
in conversation input. Shell exit, QuickJS quit() and idle empty Ctrl+D end
the interpreter and return to conversation. Source EOF runs ash EXIT traps
and preserves the last exit status. QuickJS displays completion
values through its upstream value printer. Direct input now has a separate
terminal view with continuous ordered stdout/stderr, no tool-result wrappers,
per-language command history and live line input. Enter sends stdin to running
commands; Ctrl+D sends EOF. Manual source, stdin and output stay outside model
history. Ctrl+J inserts a newline; Esc/Ctrl+C cancels cooked execution or clears idle
input. Cancellation shows a reset notice and keeps terminal mode. Interpreter
exit returns to conversation. The other interpreter and independent roots
survive. Model tools retain finite empty stdin. /clear clears local terminal
history, resets both interpreters and returns to conversation. The launcher
forwards terminal control keys and retains Ctrl+C copy for selected text.
Terminal verification and its limits are recorded in docs/verification.md.
On 2026-11-01, fixed a delivery omission: the updated launcher had been paired
with an old artifacts/esp. Both standard trees and images now contain the
current EFI and guest modules. Moved task-specific verification directories and
duplicate packages out of artifacts to the Windows temporary directory.
Updated the unchanged installed ash.wasm and quickjs.wasm in C:\Temp\work
after saving their previous bytes in Windows temporary storage. The exit-routing
update passed native checks and Release builds; earlier WHPX verification
predates it. The complete application regression also predates these input
changes. Native API settings remain build-only placeholders.

Earlier, the main Windows workspace completed a clean rebuild on 2026-10-31.
Removed target, prepared guests, packages, historical task outputs, EDK II build
outputs and the earlier Linux verification checkout/logs. Retained installed
tools, firmware, fixed source/archive inputs and the official Shell binary.
The full scripts/build.ps1 entry point rebuilt all shipped applications and
both Rust programs, then produced both 130 MiB VM and native EFI images.
Each exported workspace matches all 1,160 declared files byte for byte; boot
entries and mode markers match their inputs. All 22 recorded source inputs,
Cargo.lock and the user workspace are unchanged. The native package contains
build-only API placeholders that must be replaced before use. This rebuild
covered compilation and packaging; no VM boot or runtime suite was run.
Results for that clean rebuild are recorded in docs/verification.md.
Historical artifact references below can refer to outputs moved or removed by cleanup.

BusyBox 1.37.0 is maintained as complete source under vendor/busybox, with a
separate pristine upstream import commit. The former build-time source
rewrites are applied directly to the maintained source. The fixed feature
header and 186-source compilation list are under guests/busybox. The musl
dependency consists of its timezone rules file and license under
vendor/musl-timezone. The native Python build drivers serve both Windows and
Linux entry points; packaging prepares every shipped application and refreshes
its catalog.
Set WASI_SDK_PATH and BINARYEN_PATH or pass the toolchain directories.
The Windows direct build and a Git-only clean source build under a path with
spaces produce byte-identical ash.wasm and timezone assets. Eight native
BusyBox/ash/session suites, parsing checks and VM-only packaging pass.
The earlier artifacts/busybox-vendor evidence was removed during cleanup;
docs/verification.md retains the results.

Both root build entry points now use scripts/build_guests.py. It prepares the
fixed CPython binary and builds BusyBox, QuickJS, wolfSSL, curl and WolfSSH,
with wolfSSL built before its consumers. It uses native Windows or Linux tools,
and the individual PowerShell entry points select components in the same driver.
The selected guest output directory also supplies the wolfSSL installation.
Rust builds use Cargo.lock with --locked. Windows discovers and initializes the
MSVC toolchain with vswhere and Launch-VsDevShell. WASI SDK, Binaryen and other
host tools remain installation prerequisites. The earlier dependency audit is
recorded in docs/verification.md; artifacts/dependency-audit was removed.

A prior complete native Windows Release build passed in an isolated Git checkout
under artifacts/full-clean-build/source. It started without artifacts, target,
guest source caches or prepared application assets. The root entry point fetched
all missing inputs, built all applications and both Rust programs, validated the
catalog, and generated both VM and native boot trees and 130 MiB disk images.
The 18 relevant changed source files match the working tree; Cargo.lock is
unchanged. Installed host tools and user-level Cargo/uv caches were reused.
The native package uses clearly labeled build-only API placeholders. This run
covered Windows compilation and packaging without VM or runtime tests.
The isolated checkout and artifacts/full-clean-build evidence were removed
during cleanup. The result remains recorded in docs/verification.md.

A prior complete native Linux Release build also passed under WSL2 in the local
checkout /home/ubuntu/projects/efi-agent-linux-build-20261031/source. It used
Linux WASI SDK 34 and Binaryen 133 without Windows executables. The first online
attempt stopped at a GitHub connection timeout; the recommended host proxy also
timed out. The successful retry reused the fixed source/archive inputs from the
Windows verification and the pinned Shell Git checkout. No prepared WASM module,
compiled library or Rust output was copied. The root command exited 0, built
both Rust programs, validated the catalog and produced both 130 MiB images.
Both exported workspaces match all 1,160 declared files byte for byte.
Cargo.lock is unchanged. A make clock-skew warning was checked by relinking
curl: the bytes stayed identical and both make up-to-date queries then passed.
No build source change was required. Online provisioning, VM execution and a
fresh-machine installation remain outside this successful Linux result.
The Linux checkout/logs and artifacts/linux-clean-build were removed during
cleanup. Its external toolchains remain; docs/verification.md records the run.

The standard curl 8.22.0 build now leaves all 4,406 upstream archive files
unchanged. With WASI SDK 34, _WASI_EMULATED_SIGNAL and -mexception-handling
replace the header edits while configure leaves signal/sigaction/sigsetjmp/alarm
disabled. The existing external POSIX/cwd adapters and wolfSSL library remain.
Build-setting changes trigger configure and clean objects automatically.
Upstream TLS error codes are preserved: invalid signatures return 35, missing
issuers return 77, and wrong-IP or expired certificates return 60. Six native
integration tests and the actual 256 MiB Windows WHPX combination pass HTTP,
TLS, DNS, held connections, cancellation and QuickJS/Python-to-ash-to-curl
chains. The rebuilt standard VM package matches all 1,160 declared files and
both EFI entries in an independent FAT check. The previous workspace and
artifacts/curl-no-patches evidence were removed during cleanup.
docs/verification.md retains the results.

scripts/fetch_curl.ps1 fetches curl 8.22.0 from the official release URL into
artifacts/guest-sources/curl-8.22.0. The build and fetch entry points use the
same native Python fetch helper, scripts/fetch_guest_source.py.
Existing source directories are reused without a network request. Download
and extraction use a temporary sibling directory; only a complete source tree
is moved into place. The source remains an ignored build cache.

QuickJS 2026-06-04 includes all 63 upstream distribution files under
vendor/quickjs, with pristine import commit 83713b0. The three former build-time
changes are maintained directly: disable threaded Atomics under WASI, include
malloc.h on WASI and remove dtoa's unused setjmp.h include. The build compiles
these sources directly. scripts/fetch_wolfssl.ps1 and scripts/fetch_wolfssh.ps1
fetch official stable-tag archives for wolfSSL 5.9.4 and WolfSSH 1.6.0 into
ignored artifacts/guest-sources caches. Their build scripts call fetch
automatically, reuse existing sources and apply no upstream source rewrites.
Download and extraction use temporary sibling directories. Build wolfSSL before
curl or WolfSSH. Curl's shared configure/make commands are maintained in
scripts/build_curl.sh; Windows uses MSYS2 and Linux uses native Bash/make.

UEFI Shell has no local source patches. The outer submodule matches
ae263171ee125bd8d0be5aff3882649cd920a7f8 (26H1); its EDK II checkout matches
b7a715f7c03c45c6b4575bf88596bfd79658b8ce. Tracked files match those revisions,
and recursive submodule revisions match their recorded commits. Windows Git
reports the original EmulatorPkg/Unix/Host/X11IncludeHack WSL symbolic link as
dirty because it cannot read that link; its target still matches upstream.
The upstream workflow uses PCD build settings, and Agent packaging supplies
startup.nsh outside the Shell source. Standard native packaging now fetches the
official 26H1 shellx64.efi into artifacts/uefi-shell-26H1. EFI_AGENT_SHELL selects
a custom binary. Only the outer submodule is needed for its license and source
notice; standard packaging does not compile EDK II. VM-only builds skip Shell
preparation. See docs/verification.md.

The shared persistent namespace now supports mutations, permissions, hard and
symbolic links, anchored cwd, actual capacity and allocation, and recoverable
FAT save/reload. Shell, Python, model files and module lookup use the same tree.
Virtual /bin, /dev, /tmp and read-only /proc provide installed executables,
actual devices/process information and bounded shared temporary inodes.

All 48 required commands use upstream BusyBox applets in one shared WASM image.
PATH lookup, explicit executable paths, shebangs and manifest-installed aliases
agree across ash, env/xargs and Python subprocess. BusyBox date now uses upstream
musl timezone rules with validated TZif buffers and packaged IANA 2026e data.
C stat/lstat/fstatat preserve retained cwd and original link/parent components.

The model tools are read, write, edit, shell, js and reset_js. Each conversation creates
ash and QuickJS interpreters on first use and retains each started interpreter.
JavaScript global declarations, functions and closures persist. Top-level await,
print, console.log/error, quit and asynchronous ash shell commands are available.
The QuickJS WASI adapter uses vendored zx@8.8.5-lite quoting, interpolation and
ProcessOutput. Supported options and port limits are in vendor/zx/PORT.md.
BusyBox ASH_BASH_COMPAT is enabled. Bash extensions cross fresh shell tasks,
including serialized &> redirections. Process substitution remains unavailable.
Python runs as an ordinary command through ash scripts or quoted heredocs and
exits after each invocation. Source/completion frames use a private bounded
channel separate from finite empty command stdin.
Shell state, environment and cwd persist; JavaScript globals persist.
reset_js releases only the JavaScript session, preserving shell state, files
and independent roots. Exit/trap/cancel report resets; clear resets both sessions. Unfinished
descendants are reaped without disturbing
independent roots. Each tool call has one readable collapsed disclosure.
An unresolved top-level JavaScript await with no event source resets the
session to prevent an earlier request from resuming during a later call.

CPython remains the unchanged prebuilt 3.14.7 WASI SDK 24 binary. All 537 upstream
library files match the distribution bytes; private site startup adapters supply
process and filesystem integration. Dynamic extensions, native threads, Python
socket creation, subprocess timeouts/signals and process credentials/groups
remain unsupported. BusyBox timezone support does not change Python's time module.

All 21 current native BusyBox suites, 17 CPython cases, session/cwd/mount/proc
regressions, host tests, both Clippy targets and formatting pass. Both 512 MiB
Windows WHPX APIs pass expanded filesystem/date tools and installation, including
independent saved FAT comparison of every declared library and license. Evidence:
artifacts/environment-final-filesystem-* and environment-final-assets-*.
The complete 512 MiB WHPX application combination passes in
artifacts/environment-applications-whpx-idle, including independent saved bytes,
TCP/DNS/TLS/SSH, concurrent Python, pipelines, descriptors and here-documents.
After the memory changes below and before the JavaScript-tool change, the complete application combination passes at
198 MiB in two independent full runs; 197 MiB fails twice at Python subprocess
startup, with explicit ENOMEM on repeat. This is the measured 1-MiB boundary for that fixed sequence, including
explicit module registrations and concurrent Python, rather than a general
Python/ash memory requirement. Evidence: artifacts/network-memory-boundary.
The previous 256-MiB pass provides headroom. The VM launcher defaults to
256 MiB; --memory-mib selects another size. Both model API session flows
pass at 192 MiB, including an additional Python process, clear and saved bytes.
The 192 MiB cancellation flow preserves an independent background root.

Before these memory changes, WHPX shell-only RAM checks passed the fixed workload
at 97 MiB with only ash installed; 96 MiB fails in a pipeline and 64 MiB fails
the first ash instance. Use 128 MiB for headroom in that installation. With the
full shipped package installed but no Python execution, 160 MiB disconnects
and 192 MiB passed. Those measurements included validation of unused installed
modules and must be repeated to establish the new ash minimum. Evidence is under
artifacts/ash-memory-probe and docs/verification.md.

The runtime now ignores custom sections, uses compact indirect dispatch and
exact linear-memory growth, and reads unused modules only when requested.
Each file image owns an Engine; weak revision caches share it across active
instances and release its code on final exit. Explicit byte registrations remain
snapshots. Eager compilation and the unchanged Python bytes are preserved.
The former retained Python session used about 55.3 MiB. The retained QuickJS
session uses about 3.59 MiB, with about 25 KiB left after exit/cancel before reap.
Repeated Python/curl/SSH and pipeline
process exits leave about 25 KiB after reap without growth across cycles.
Pipeline failures close pending endpoints, free serialization buffers and
cancel/reap partial child trees. Subtree cancellation stops parents first so
suspended waits cannot reap children mid-cleanup. See docs/memory-analysis.md
and artifacts/memory-resource-lifecycle for allocation evidence.

The VM-only build is `scripts/build.ps1 -VmOnly`; its outputs are artifacts/esp
and artifacts/efi-agent-vm.img. It requires API settings only when launching.
The earlier standard package passed actual Windows WHPX at 256 MiB through the
native console launcher and the Responses shell/JavaScript session flow,
including reset/cancel isolation and independently checked saved FAT bytes.
Those runtime results predate the current Windows rebuild.
The VM launcher currently stages a private FAT disk and exports workspace
changes on exit. Live host-directory sharing remains an outstanding requirement.
Detailed scope, acceptance evidence and remaining port limits are in
[docs/environment-work.md](docs/environment-work.md),
[docs/verification.md](docs/verification.md) and
[docs/port-transparency-audit.md](docs/port-transparency-audit.md).

## Architecture

The UEFI app now embeds the no_std `efi-agent-wasm` crate with wasmi 2.
Each process has an independent Store and memory. A single APPLICATION-TPL
dispatcher resumes retained call stacks after explicit yield, child wait,
or fuel exhaustion. `/wasm` loads registered command modules and starts,
runs, lists, cancels, or reaps their processes. Module paths accept slash or
backslash separators. Relative and absolute module paths use the configured
workspace namespace. Guest binaries sit directly in the workspace,
so `/wasm load python.wasm` loads Python without a separate directory.
`/wasm load PATH` infers the registered name from the final filename, removing
a case-insensitive .wasm suffix. `/wasm load NAME PATH` selects an explicit
alias. Both forms resolve paths against the same configured workspace/root.
Basic WASI arguments, finite
stdin, stdout/stderr, exit and yield are implemented. The `efi_process`
imports provide getpid/getppid, spawn and waitpid without fork. Children
inherit selected standard descriptors. Descriptor tables, 64 KiB pipes,
dup2, close, renumber and blocking reads/writes retain continuations;
last-endpoint closure supplies EOF or EPIPE and exit closes descriptors.
WASI poll_oneoff suspends across descriptor readiness, EOF/HUP and errors;
an immediate relative timeout allows nonblocking checks. Timed waits still
require an available clock type. The firmware provider now supplies realtime
through EFI GetTime and its reported resolution. Timed relative/absolute
poll and WASI clock_time_get/clock_res_get use that provider. Firmware
monotonic time now uses a 64-bit UEFI Timestamp counter, or PI CPU timer 0
when Timestamp is absent. Constant-rate conversion retains fractional ticks
and handles 64-bit wrap. Relative realtime polls use monotonic elapsed time;
absolute realtime polls retain RTC semantics. CPU-time clocks remain unavailable.
The C process and concurrent capture libraries are under guests. Nonblocking
waitpid_nohang lets command event loops drain all child streams. Module
registration eagerly compiles functions; execution remains fuel-scheduled.
Stream fd_seek returns SPIPE. QuickJS 2026-06-04 builds with Windows WASI SDK
34 through scripts/build_quickjs.ps1. It links the engine without quickjs-libc
and exposes print and a Promise-based ash command template, quoting each
interpolated shell word. Commands spawn registered ash instances with -c and
capture stdout/stderr. Host wasmi verified the real engine and command ABI
against both a probe and actual ash builtins. Actual QEMU/OVMF also verified the
engine, top-level await, firmware Date.now and concurrent real ash capture.
The implemented application ports and their remaining limits are described
below and in docs/wasm-world.md.
CPython now uses Brett Cannon's prebuilt 3.14.7 WASI SDK 24 distribution,
linked from PSF webassembly. scripts/prepare_cpython.ps1 copies the binary
unchanged, the full standard library and license; no interpreter build remains.
The standard CLI supports -c, script files and -m; open, FileIO and .py imports
use the workspace. sitecustomize installs a private Python monkey patch over
upstream subprocess process hooks and os functions. WASI open/read/write on
the process-local .efi-process endpoint provide cooperative spawn/wait and
explicit descriptor mapping. The endpoint cannot be inherited. Standard
run/Popen/check_output/os.system, binary/text capture, environments, file
redirections and pass_fds replace the removed public efi module. Native tests
verify duplex output above pipe capacity, capture above the old 512 KiB cap,
concurrency, descriptor boundaries and rejection before effects. Dynamic
extensions and native threads remain unavailable. The prebuilt _socket exists,
but socket creation lacks a backend. -S disables the startup patch.
Timeouts, signals, process groups/credentials and distinct custom argv[0]
are unsupported.
The runtime resolves PATH, explicit executable paths and absolute-interpreter
shebangs. Child cwd and umask use inherited runtime anchors and metadata.
See docs/port-transparency-audit.md and docs/verification.md for evidence and limits.
The former source-built standard-interface correction passes the 512 MiB
QEMU/OVMF application combination with curl/TLS/DNS/POSIX/WolfSSH/QuickJS/ash
arithmetic in artifacts/wasm-python-transparent-applications. The current prebuilt distribution
passes the focused 512 MiB QEMU/OVMF Python/ash/QuickJS combination, including
standard scripts/imports, duplex pipes, concurrent Python and saved FAT bytes,
in artifacts/wasm-python-monkey-long. Its TCG run used a 180-second command
timeout. All 11 native integration cases and an additional site/timeout case
pass. The complete 512 MiB curl/TLS/DNS/POSIX/WolfSSH/QuickJS/ash/math/random
combination also passes with current prebuilt Python, held HTTP/SSH peers,
cancellation and saved-byte checks in artifacts/wasm-python-monkey-applications.
-I retains the adapter; -S disables it. Ordinary file opens have an
inheritable default under WASI libc, recorded in the transparency audit.
The firmware module limit is now 48 MiB for the unchanged 30,522,756-byte binary.
Earlier Windows WHPX verification is recorded in the Current state section.
Normal return, proc_exit, traps and cancellation now drop execution Stores,
linear memory, continuations and pending I/O immediately. Only parent PID,
status and shared output remain until wait/reap. Live children are adopted by
PID 0 and retain inherited output even after their parent is reaped. A counting
allocator test verifies release of at least 60 MiB on each terminal path before
reap; adoption, pipe closure and wait-status behavior pass host checks. The
earlier combined Python concurrency test needed 512 MiB with exited Stores
retained; the revised combined firmware test passes at 256 MiB, including
concurrent Python roots, QuickJS, ash and saved FAT byte checks. One prior run
stopped on a malformed serial view frame before Python; the rerun passed.
That intermittent transport failure remains unroot-caused.
The initial network provider and efi_net.tcp_connect ABI now reserve a socket
descriptor and suspend in Connecting until completion. Connected sockets use
WASI read/write/poll and shared descriptor lifetime. Native run_guest installs
a mio nonblocking TCP backend. Actual Windows TCP tests verify byte payloads,
EOF, refusal, duplicate closure, child progress during network wait and host
cancellation of a blocked read. ABI tests cover validation before effects,
descriptor/port bounds, failed-attempt retry and pending-connect cleanup.
UEFI now installs a persistent-token TCP4 provider in tcp/cooperative.rs.
It reuses configured static IPv4 or starts DHCP without waiting in the guest
call. Mapping/connect states advance per dispatcher poll with a 30-second
deadline. Boxed connect/read/write tokens, packets and buffers remain stable
until completion or confirmed synchronous reset. GET_PROTOCOL access to the
service binding permits independent TCP children on one NIC. Writes accept
owned prefixes up to 64 KiB; reads/poll cache received bytes up to 64 KiB.
The last close requests abortive close then resets and verifies pending token
retirement. Real QEMU TCP transfer, child progress handshake and blocked-read
cancellation with observed peer closure pass. efi_net.tcp_close now drains
accepted sends and queues normal TCP4 close with a 30-second deadline. It
suspends in Closing, consumes the descriptor on completion/error, and closes
the shared connection for duplicates. Native and real QEMU tests verify the
full 196,613-byte payload before EOF and closed-duplicate behavior. Ordinary
fd_close and cancellation remain abortive; half-shutdown is not implemented.
/wasm run redraws changed output while polling. Each top-level run/start has
one default-collapsed disclosure with command, process ID, readable status,
and plain-text standard output/error sections. Stream chunks and completion
update that disclosure without resetting the user's expansion choice.
Child output stays with its root; child exits do not add message blocks.
Background roots remain separate. /clear discards earlier output and later
background updates recreate a collapsed panel without editing reused rows.
The 2026-10-22 output UI checks pass actual QEMU Python/ash capture/direct
stdout/stderr, keyboard disclosure, failure/trap/cancellation, background
isolation and clearing in artifacts/wasm-output-panel-final-all. Ordinary exit
130 is distinguished from cancellation. The adapted base process integration
passes in artifacts/wasm-output-base-latest. All 116 core/wasm tests and Clippy
pass; the full network application combination was not rerun for this UI change.
DNS now uses cooperative TCP queries through efi_net.resolve_ipv4. It accepts
ASCII hostnames, checks the random transaction ID and question, follows bounded
CNAME chains and returns one IPv4 A address. The configured DNS address/port
feeds the firmware world. A 10-second realtime deadline bounds the query;
failures preserve the guest output and terminal paths release the connection.
Native and actual QEMU tests verify fragmented A/CNAME responses, NXDOMAIN,
wrong IDs/questions and invalid compression. Deterministic ABI tests verify
deadline boundaries, cancellation, clock/random failures and range validation.
Socket fd_fdstat_set_flags now supports NONBLOCK. Duplicates share flags;
pending reads/writes return AGAIN without changing guest outputs, zero-byte
operations succeed, and EOF remains a successful zero-byte read. Restoring
blocking mode resumes cooperative waiting; poll readiness is independent of
that flag. Deterministic ABI, actual Windows TCP and QEMU/UEFI tests pass.
efi_net.tcp_open, tcp_connect_fd and tcp_error now separate descriptor creation,
connection and shared SO_ERROR-like reporting. NONBLOCK returns INPROGRESS;
poll write readiness advances connection, and duplicate descriptors share
Connecting/Connected/Failed/Closed state. Pending repeats return ALREADY,
connected repeats ISCONN. Failure drops transport storage but retains the
descriptor; error retrieval clears the shared report without restoring a
connection. Failed sockets can retry. Blocking connect retains a continuation.
Deterministic ABI and real Windows tests verify success/refusal, validation,
error clearing, retry and pending cancellation. Actual QEMU verifies asynchronous
connect/poll/error followed by TCP data and EOF. guests/posix now supplies WASI
Preview 1 socket, connect, getsockopt, send, recv and IPv4 getaddrinfo/freeaddrinfo
wrappers. It uses SDK fcntl/poll/close and the existing cooperative imports.
Numeric services and hosts, byte order, shared nonblocking flags, SO_ERROR
buffer bounds/clearing, errno conversion, data/EOF and hostname DNS pass real
Windows and QEMU checks. Native refusal is ECONNREFUSED; tested QEMU NAT drops
SYNs to the bound non-listening host port, so firmware verifies ETIMEDOUT instead,
with retained packet capture. Unknown options fail explicitly.
Socket options, address queries, listen/accept, peek/waitall and half-shutdown remain
unsupported. Canonical names are available for numeric inputs only. Actual
curl and WolfSSH ports use the implemented client operations described below.
All descriptor kinds now use descriptor-local close-on-exec metadata. Default
spawn skips marked standard descriptors; explicit spawn_fds mappings and dup2/
dupfd clear the copied flag, while renumber and same-fd dup2 preserve it.
Host integration tests cover sockets allocated at fd 0 after stdin closure.
scripts/build_curl.ps1 builds actual curl 8.22.0 with the explicit Windows WASI
SDK and native MSYS configure/make. It now links wolfSSL 5.9.4 built through
scripts/build_wolfssl.ps1 with single-thread user I/O and WASI random_get.
Library/client headers share user_settings.h; generated options.h is disabled.
The build disables threaded resolution, socketpair, IPv6, FTP, TFTP and NTLM.
Real Windows wasmi and
QEMU/OVMF TCP4 checks pass redirect and chunked 131,089-byte downloads plus
196,613-byte uploads with independent byte oracles and saved FAT verification.
Curl DNS A/CNAME downloads and NXDOMAIN/malformed response rejection also pass
native and actual firmware checks. Actual firmware also verifies a held curl
HTTP response while another WASM instance makes progress, then cancellation
with server-observed TCP closure. QuickJS zx-like commands and restricted
CPython both call real ash-to-curl uploads while that curl remains waiting;
independent server bytes and returned SHA256 match. Actual native Release and
QEMU/OVMF verify TLS 1.2/1.3 bytes, trusted CA loading, invalid signatures,
missing issuers, wrong-IP and expired certificates. The upstream curl wolfSSL
backend reports signature failures as 35, missing issuers as 77, and wrong-IP
or expired certificates as 60. Unavailable firmware entropy rejects TLS before
HTTP with 35.
CA trust is explicit through --cacert; no system/default CA bundle is packaged.
WASI path_readlink checks rights/ranges, retains missing/provider errors and
returns EINVAL for existing non-links without output changes. Symlink access
is rejected by providers. WolfSSH 1.6.0 now builds as the actual WASI library
and an exec client through scripts/build_wolfssh.ps1. It requires an explicit
SHA256 host-key pin, reads a bounded password file, attempts password auth
once, sends stdin EOF, drains stdout/stderr with shared-window credit, and
propagates remote exit status after channel close. Missing status fails with
255. Independent native AsyncSSH tests pass binary streams, delayed/missing
status, trust/authentication failures and password/pin boundaries. All crypto
consumers share the WASI 128-bit-integer feature to keep big-integer ABI
consistent. UI loops dispatch input/events after each WASM time slice and skip
the fixed 10 ms idle wait when the scheduler made progress. Real QEMU TCP4
checks pass byte-exact stdout/stderr files, late/missing status, rejected pins
before auth, rejected passwords, simultaneous held SSH and QuickJS/Python
ash-to-WolfSSH chains, and cancellation with observed closure. With RDRAND
disabled, SSH fails before authentication without an entropy fallback.
The complete curl/TLS/DNS/POSIX/WolfSSH/QuickJS/ash/CPython combination passes
with 512 MiB RAM in artifacts/wasm-applications-combined-512. The same full
combination with 256 MiB fails at second concurrent Python initialization
with out-of-memory; the separate 256 MiB WolfSSH combination passes.
See artifacts/wasm-wolfssh-combined, artifacts/wasm-wolfssh-no-rdrand,
artifacts/wasm-curl-tls-final,
artifacts/wasm-curl-tls-no-rdrand and artifacts/wasm-curl-combined-final
and docs/verification.md.
BusyBox 1.37.0 sources are maintained under vendor/busybox. The actual ash
translation unit compiles to a wasm32 object through check_busybox.ps1.
build_busybox.ps1 invokes the shared native build driver, which links and
Asyncify-transforms ash.wasm without generating C source.
Real ash echo, quoting, functions, loops, conditions, exit status and syntax
errors pass in host wasmi. Real ash and concurrent QuickJS shell capture also
pass in QEMU/OVMF. Streams expose filestat; unchanged fd flags are accepted and
unsupported changes fail. Ordinary external commands
now use live exact-name program registry lookup and spawn/wait/reap fresh
instances. Shell function/builtin precedence, temporary exports, repeated
assignment last-value behavior, exec, set -e and statuses 126/127 pass host
checks. External dispatch and QuickJS rejection also pass QEMU/OVMF.
Windows guest builds must use the explicit
WASI SDK compiler path. MSYS2 mingw64 Clang was repaired by upgrading the
Clang/LLVM/compiler-rt family together to 22.1.8-2. Its previous 21.1.8
libraries imported emulated-TLS symbols absent from GCC 16.2.0-3. Native
compiler startup and a threaded C++ call_once program now pass. Guest builds
continue to use WASI SDK 34 explicitly.
guests/busybox/exec.c starts registered programs as fresh instances with explicit
environment, waits/reaps, and propagates their exit status. Its separate real
WASM fixture verifies errors and child status. tasks.inc serializes nodes and
shell state for fresh tasks (512 KiB binary bytes, tree depth 256). Pipelines
spawn all stages before waiting; command substitution drains before waiting.
Variables, export/readonly/unset flags, functions, aliases, options, positionals,
getopts state, $$ and previous status cross the boundary. Host checks cover
nested tasks, background wait, tested errexit, transfer bounds and a 196,609-byte
three-stage pipeline. QEMU/OVMF checks pipelines, substitution, parent state
isolation and QuickJS pipeline capture. Here-documents now expand in the
current shell and use efi_process.input_snapshot, an anonymous read-only
stream capped at 512 KiB. The bytes are copied; descriptor duplicates/spawn
share consumption and EOF. This removes the upstream fork/pipe-writer path.
Native checks pass 4096/65536 boundary sizes, 196613-byte input, literal and
expanded text, tab stripping, multiple inputs, small pipeline inheritance,
300 repeated closes and oversize rejection. Actual QEMU verifies the same
cases against independent saved FAT bytes in artifacts/wasm-ash-heredocs.
The full 512 MiB curl/TLS/DNS/WolfSSH/QuickJS/ash/CPython regression with
these cases also passes in artifacts/wasm-applications-heredocs-512.
Large heredoc syntax trees now cross fresh task boundaries in a mapped binary
snapshot stream, independently of the 64 KiB argv/environment limit. Native
checks pass 30001/65537/196613-byte here-documents through subshells, pipelines,
command substitutions and inherited functions, plus 256 KiB variable state.
The 512 KiB cap covers the entire serialized shell state and syntax tree.
Actual QEMU verifies all 26 here-document cases and malformed binary inputs in
artifacts/wasm-binary-tasks-focused. The full 512 MiB application combination
with these checks, arithmetic, descriptor mapping and both language interfaces
also passes in artifacts/wasm-applications-binary-tasks-512. Staged printf
captures numeric conversion errors before stdio output, which may change errno
even on success; this fixes false status 1 for large firmware output.
The process ABI now offers spawn_mapped_fds with an explicit child-to-parent
descriptor map (up to 127 entries, target fd 3 reserved for the workspace).
Native integration tests verify output swaps, shared binary consumption,
parent-close lifetime, invalid maps without PID allocation, empty maps and
close-on-exec duplication. Actual QEMU checks fd 127, the full map, parent
descriptor lifetime and the preserved filesystem preopen in
artifacts/wasm-descriptor-mapping. Ash tasks and external commands now inherit
open user descriptors through explicit maps. Internal pipes and saved parser/
redirection descriptors are marked close-on-exec and omitted; tasks explicitly
map needed internal pipe ends to stdio. The task snapshot uses an available
child descriptor >=4, preserving user fd 4, and closes before decoding/evaluation.
Native cases verify fd 4/7, shared cursors, pipelines, substitutions, external
commands, nested tasks, output files, child-close isolation, closed stdin and
300 repeated open/task/close cycles. The dupfd wrapper converts POSIX errors
to WASI libc errors, including the EBADF branch for previously unopened targets.
Actual QEMU verifies all ten descriptor cases against saved FAT bytes and
descriptor flags on snapshots/files/pipes/output in the full 512 MiB named
application combination, artifacts/wasm-applications-descriptors-512.
BusyBox's real 64-bit arithmetic and test/[ builtins are now enabled. Native
checks pass precedence/base parsing, assignment, signed wrap, short-circuit,
64-bit test boundaries, file type/size checks, task isolation and repeated
nonlocal error recovery. The staged math patch adds logical short-circuit
suppression with enclosing state restoration; upstream only handled ternary
suppression. Independent expression/side-effect oracles verify nested branches.
The test error helper uses the same Asyncify-safe return semantics as ash.
Actual QEMU verifies these math/test cases against saved FAT bytes in the
complete 512 MiB application regression, artifacts/wasm-applications-math-512.
The dedicated artifacts/wasm-ash-math-languages QEMU run also verifies
QuickJS/Python shell arithmetic (sum of squares 0..20 = 2870) and exact test
error capture. QuickJS awaits the outer Promise so assertion rejection fails
the root process; both languages require status 2 and the actual diagnostic.
Filesystem PATH and process umask use the shared runtime. Ulimit remains
unsupported. The real
upstream read builtin now supports IFS splitting, raw/escaped input, continuation,
EOF, -n/-d and selected input descriptors through WASI read/poll. Host timed
reads pass; UEFI positive timeouts use the installed monotonic clock.
Terminal echo control (-s) returns an explicit error. Host field-oracle checks
and QEMU/OVMF actual read/QuickJS pipeline checks pass.
The world now accepts a host filesystem provider and gives each process a
preopened directory at fd 3, named slash. UEFI uses the configured boot-volume
workspace, or \work when model configuration is absent. File handles support
read/write, create/exclusive/truncate/append, metadata, seek/tell and sync.
Duplicates share file cursors; independent opens have separate cursors.
Directory-relative paths validate each intermediate component. Host WASI libc,
ash redirections, fresh tasks and QuickJS file capture pass. Actual QEMU/OVMF
also verifies file operations and the saved FAT bytes. Directory creation now
reaches the native and firmware providers through WASI path_create_directory.
The model write tool creates missing parents and includes paths in errors.
Native and firmware rename/delete, timestamps and filesystem PATH use the
shared persistent namespace. File fd_write validates every iovec before
effects and writes 64 KiB chunks without the former 512 KiB call limit.
Finite input and captured output report stream types rather than character
devices, so unchanged CPython reads `python -` as a script. Automatic import
bytecode writes are enabled. Native and WHPX tests verify cache creation/reuse,
py_compile, compileall and `-B` behavior.
Model code tools discover root workspace .wasm files, validate their header and
size, and compile on first spawn. Program lookup includes discovered modules.
QuickJS has a qjs alias that shares its compiled module. Existing installations
remain stable when their source files change. A persistent installation manifest,
filesystem PATH and /bin entries still need implementation.
The initial filesystem/tool sequence passes actual 512 MiB WHPX/Responses, including saved FAT byte checks. The 256 MiB sequence
writes 8 MiB but fails later at Python-to-Python spawn; that memory boundary
is unresolved. Allocation failures now map to ENOMEM rather than ENOEXEC,
with a deterministic guest memory-quota check.
The final Release EFI passes the new 512 MiB WHPX filesystem sequence in
artifacts/filesystem-tools-whpx-512-final and the existing 256 MiB WHPX
model-tool regression in artifacts/model-tools-whpx-256-final. All 121
core/wasm tests and host/UEFI Clippy pass. The full environment expansion
remains in progress; docs/environment-work.md retains the completion scope.
File resizing now implements fd_filestat_set_size with writable-descriptor and
resize-right checks, a 48 MiB limit, zero-filled growth and preserved shared
cursors. Firmware reads beyond a truncated EOF return zero bytes without
moving the cursor. Append descriptors resize without moving their cursor and
append at the new EOF. Actual libc checks and a 71-operation resize/overwrite
byte oracle pass on Windows and QEMU/OVMF, including independent saved FAT
content checks. Firmware growth remains synchronous and physical failures can
leave partial growth.
WASI random_get now uses an installed host random provider, shared by processes.
Firmware uses the application's RDRAND-seeded ChaCha20 generator; host examples
use the OS random source. Ranges are checked before generation, empty requests
need no provider, requests above 512 KiB return INVAL, and failed generation
publishes no bytes. Missing RDRAND returns NOTSUP without a fallback. Actual
libc getentropy, guarded buffers and persisted output records pass on Windows
and QEMU/OVMF. A QEMU run with RDRAND disabled verifies the failure path.
Deterministic ABI tests verify boundaries and shared-source continuity; core
tests verify the ChaCha20 known vector and no reseeding. These checks do not
establish curl or WolfSSH ports; separate checks establish CPython execution.
Directory providers now enumerate real host/EFI entries. fd_readdir retains a
sorted list per traversal, supports partial records and continuation cookies,
and refreshes on cookie zero. Limits are 4096 entries and 512 KiB of encoded
directory records. Duplicate descriptors share the traversal list; independent
opens do not. Actual libc opendir/readdir/seekdir/rewinddir and ash Unicode glob
checks pass on the host. Directory NONBLOCK is accepted and reported in fdstat.
Actual QEMU/OVMF also verifies directory traversal, libc positioning, ash glob
and QuickJS glob capture against independent name/type records read from FAT.
The dupfd process import allocates the lowest available descriptor at or above
a minimum. Ash uses it to save redirections at fd >=10, which do not cross spawn.
The real upstream printf builtin and bb_strtonum helpers now build into ash.
Host checks cover formatting, 64-bit integer boundaries, errors, byte escapes,
NUL output, pipes and QuickJS binary capture. Actual QEMU/OVMF verifies signed
and hexadecimal boundaries plus QuickJS NUL capture. Read regression checks
pass with the upstream numeric helpers.
guests/sjlj implements nonlocal jumps using Binaryen 133
Asyncify, needed by ash's setjmp/longjmp recovery. Fixture checks passed with
O0/O2/O3, nested/repeated jumps, volatile stack objects and concurrent instances.
The driver operates entirely inside each guest; host imports retain wasmi
continuations. Ash integration is verified; full nonlocal-jump resource limits
remain pending. The world's Wasm call depth is bounded to 4096 to accommodate
Asyncify's helper frames around bounded guest tree traversal. Its value stack
is limited to 8 MiB per active interpreter stack.
Process startup environments are now copied per instance. Ordinary spawn
inherits the startup list; spawn_env_fds replaces or clears it explicitly.
WASI environ calls expose the list. Arguments/environment share a 64 KiB limit;
names are nonempty and unique, values may be empty or contain '='. C helpers
pack explicit lists for future ash exports. Guest libc environment mutations
require explicit passing; host environment variables are never copied implicitly.

The UEFI application owns direct model HTTP/HTTPS requests, DNS A queries over
TCP, SSE parsing for Chat Completions and Responses, reasoning state, usage
statistics, cancellation, and the read/write/edit/shell/js/reset_js tool loop. There is no relay,
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
Shell protocol. The model shell tool runs WASM ash and does not invoke the
firmware Shell protocol.
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

The WASM work is split into functional commits for the runtime, firmware
providers, C guest helpers, BusyBox ash, QuickJS, CPython, POSIX networking,
curl/wolfSSL, WolfSSH, and combined verification/documentation. The latest
recorded complete regression is artifacts/environment-applications-whpx-idle.
Prepared manifests include guest binaries and libraries in fresh VM/native
workspaces. Dependency licenses are packaged separately under EFI/AGENT/LICENSES.
BusyBox uses checked-in sources with the explicit native
WASI SDK and Binaryen. Other source-built ports require their upstream sources;
CPython uses its unchanged prebuilt.

The 2026-10-12 native allocation investigation found linear-memory capacity
amplification: each Python instance starts at 18.875 MiB, grows to a visible
19.375 MiB, but wasmi_core's Vec reserves 37.750 MiB. The UEFI allocator uses
allocate/copy/free reallocation, so old and new buffers coexist during growth.
The then-current source-built Python also reserves a 16 MiB C stack per instance. Five registered
modules occupy about 32.2 MiB in the native allocation probe. These findings
do not establish the full firmware OOM cause or its minimum RAM requirement.
See docs/verification.md and artifacts/memory-inspection-2026-10-12.log.
Repeated registration also retains old compiled functions in the shared Engine.
The full test loads Python three times and curl twice before Python concurrency;
the same native registration sequence increases module allocations from 32.208
to 62.269 MiB. This is an additional identified retention issue, not a complete
firmware OOM explanation. See artifacts/module-reload-inspection-2026-10-12.log.

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

Slash commands: /help, /caps, /wasm, /effort [value], /status, /clear, /quit, /exit.
A normal prompt runs at most twelve rounds of read/write/edit/shell/js/reset_js tools.
The shell and JavaScript tools retain separate interpreters, loaded automatically
from ash.wasm and quickjs.wasm. Python runs through ordinary shell commands.
Source uses a private bounded channel (60 KiB limit, no NUL) separate from finite
stdin. Results contain readable exit status and stdout/stderr, capped at 64 KiB
per stream with explicit truncation. The existing tool disclosure holds the result.
reset_js releases only the JavaScript session. Clear resets both. Tool completion
or cancellation reaps unfinished descendants while independent roots continue.
The prompt describes Alpine-style BusyBox ash with ASH_BASH_COMPAT and states
its actual WASM limits. Native integration and both provider APIs are recorded
in docs/verification.md with the current acceptance evidence.
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
- Native firmware Shell command execution.
- Complete the WASM world and named application ports in docs/wasm-world.md.
- Permissions and sandboxes.
- Richer tool views and conversation navigation.
- Live resize on physical terminals.

GOP, virtio-fs, and macOS HVF support are outside the current scope.
