# Rules

You are a senior software engineer working on this project, EFI Agent.

Do not edit README.md for your memory. Edit [the Current state section here](#current-state)
instead. Keep README.md concise and clean.

Maintain this document `./docs/verification.md` by yourself, too.

# Current state

Null/zero/random/urandom now expose character metadata, distinct inodes and
fixed 0666 modes. Discard writes, seek, explicit sync/truncate failures,
shared descriptor flags and non-terminal unchanged Python stdin pass native
integration. Random reads share the configured cryptographic provider and
stage at most 64 KiB without publishing failed results. WASI fdstat uses UNKNOWN
to avoid libc's character-device-as-terminal assumption; filestat retains the
character type. Upstream BusyBox mount lookup omits its Linux UBI character
device special case, fixing df device and cross-mount link capacity selection.
Host tests, host/UEFI Clippy, formatting, native regressions and Release packaging
pass. Both actual 512 MiB WHPX model API flows and independent saved FAT checks
pass in artifacts/character-devices-whpx-responses-final and
artifacts/character-devices-whpx-chat_completions-final. Descriptor aliases,
actual process files, anchored cwd tracking, manifests and sessions remain.

Mount initialization now rejects existing workspace dev/tmp/proc entries,
including empty directories and dangling links, before publishing a root.
Real bin directories remain merged; non-directory bin entries and installed
command conflicts report their path. Failed replacement preserves the old root
and temporary bytes. Firmware model file tools and module loaders now share
the guest mounted root, and unknown dev/proc paths cannot reach backing storage.
Five Rust layout tests and ten native conflict cases pass with independent
saved-state checks. Both actual 512 MiB WHPX model API flows verify model
write/edit/read in /tmp, shell/Python access, loading a temporary WASM module,
saved source bytes, temporary exclusion and existing mount/link invariants in
artifacts/mount-layout-whpx-responses and
artifacts/mount-layout-whpx-chat_completions. Host tests, host/UEFI Clippy,
formatting, native regressions and Release packaging pass. Complete dev/proc,
anchored cwd tracking, installation manifests and retained sessions remain.

The virtual root resolves absolute/relative links across runtime mounts before
provider dispatch, with link-before-parent ordering, shared 40-link and expanded
4096-byte limits, final-entry mutation policy and searchable ancestor checks.
Root directory fds retain mounts; non-root directory capabilities stay bounded
after rename. Python metadata and child cwd preserve link/parent components.
Links to installed commands reuse compiled modules, verified with a compiled-only
registration. Native Python/ash and both actual 512 MiB WHPX model API flows
pass with independent saved FAT bytes, modes and link targets in
artifacts/mount-walk-whpx-responses and
artifacts/mount-walk-whpx-chat_completions. Host workspace tests, host/UEFI
Clippy, formatting, native regressions and Release packaging pass.
Workspace directory views also retain the routing layer over inode anchors.
Rename/hard-link operations work in both directions with the virtual root
after directory rename; local tmp/dev/proc/bin names remain ordinary
directories and actual cross-device operations fail before effects. Native
regressions and both actual 512 MiB WHPX model API flows pass with independent
saved FAT hard-link groups and bytes in artifacts/anchored-mount-whpx-responses
and artifacts/anchored-mount-whpx-chat_completions. Complete dev/proc, anchored
cwd tracking, manifests and retained sessions remain incomplete.

The shared /tmp mount now reuses the inode namespace over bounded memory
objects, with ephemeral metadata commits, nested directories, relative links,
hard links, permissions/times and directory anchors retained through rename.
The 32 MiB data and 4096 non-root inode/name quotas include detached open
objects. Full directory listings retain their ordinary-name limit in addition
to dot entries. The private process endpoint only intercepts the workspace
preopen. Native integration verifies standard tempfile cleanup, renamed
directory-fd operations, child cwd, cross-device mv fallback, archive extraction
and an ordinary temporary file named like the private endpoint. Exact quota,
last-close and structured sparse-write models pass. Both actual 512 MiB WHPX
model API flows pass with independent saved FAT bytes/metadata and temporary
mount exclusion in artifacts/tmp-inodes-whpx-responses and
artifacts/tmp-inodes-whpx-chat_completions. Host tests, host/UEFI Clippy,
formatting, Release packaging and native command/Python/permission/stream
regressions pass. Absolute/cross-mount link
traversal, collision policy, device/process files, installation manifests and
retained sessions remain incomplete.

Upstream BusyBox tar now builds into the shared image with create/list/extract,
GNU long names/links, PAX names, filters, strip-components and cooperative gzip
streams. Explicit fd maps support closed standard fds and empty environments.
Extraction anchors unlink and hard links as well as creation and metadata;
directory modes/times are restored after children. Staged fixes check PAX
lengths before pointer movement, read skipped data to detect truncation and
propagate metadata, close and output errors. Native integration, independent
saved archive/namespace checks and both actual 512 MiB WHPX model API flows
pass in artifacts/tar-native-final.log, artifacts/tar-whpx-responses and
artifacts/tar-whpx-chat_completions. Host tests, host/UEFI Clippy, formatting,
Release packaging, Python 17-case and native command/permission/stream
regressions pass. Required commands now number 48/48. General mounts/link
traversal, installation manifests, retained sessions and full environment
delivery remain incomplete.

Upstream BusyBox stat now enables filesystem mode with a guest statfs adapter
over the measured capacity provider. Default, terse and custom output cover
workspace/tmpfs/devfs/proc identities. Native guest and actual 512 MiB WHPX
checks pass both model APIs, with independent saved FAT capacity and metadata
checks in artifacts/statfs-whpx-responses and
artifacts/statfs-whpx-chat_completions. The actual shared image passes a
deterministic 2^52-byte capacity check with exact 64-bit output and distinct
free/available counts. Host Clippy and formatting pass. Required commands remain
47/48, with tar pending. General mounts, manifests, retained sessions and full
environment delivery remain incomplete.

Upstream BusyBox df now builds into the shared image. Standard C and Python
statvfs/fstatvfs use measured workspace capacity, distinct free/available bytes,
namespace slot counts and runtime mount statistics. Windows allocation units
come from the volume; firmware capacity comes from EFI_FILE_SYSTEM_INFO.
Shared /tmp storage enforces 32 MiB and 4096 live objects, retaining unlinked
open-file charges until last close. Boundary, rights and quota integration,
native guest regressions, host tests, host/UEFI Clippy, formatting and Release
packaging pass. Both actual 512 MiB WHPX model API flows pass independent
saved FAT capacity and byte checks in artifacts/df-whpx-responses and
artifacts/df-whpx-chat_completions. Required commands now number 47/48; tar
remains. Stat -f, general mounts, manifests, retained sessions and full
environment delivery remain incomplete.

Upstream BusyBox ln now builds into the shared image. File and symlink hard
links share namespace inodes, backing data, permissions, times and link counts.
Alias deletion/promotion, same-inode rename, anchored views, open-file lifetime
and journal recovery pass native integration and a structured mutation model.
Physical host hard links import once and reclaim redundant names after commit.
Python os.link supports directory fds and follow flags through its private
startup adapter; CPython stays prebuilt. Cp -l/-a and du default/-l use the same
identities. Launcher FAT staging/export/save/reload preserves groups and detects
host identity conflicts. Actual 512 MiB WHPX passes both model API flows with
independent saved FAT inode/backing/byte/mode checks in artifacts/ln-whpx-responses
and artifacts/ln-whpx-chat_completions. Host tests, native guest regressions,
host/UEFI Clippy, formatting and Release packaging pass. Required commands now
number 46/48; df and tar remain. General mounts, manifests, retained sessions
and full environment delivery remain incomplete.

Upstream du now builds into the shared image with measured file allocation.
Windows queries file compression/allocation information by handle; UEFI uses
EFI_FILE_INFO PhysicalSize. Namespace files retain these values across rename,
unlink and open handles. Logical directories and links use shared journals
without independently allocated data blocks. Unknown allocation remains
explicitly unsupported. Private metadata queries supply actual st_blocks to
BusyBox and unchanged CPython through their existing adapters. Directory link
counts now include subdirectories, enabling upstream directory deduplication.
Native Release integration includes sparse files, independent Windows path
queries and binary/unit boundaries. Both 512 MiB WHPX model APIs pass with
independent saved FAT cluster-chain checks in artifacts/du-whpx-responses and
artifacts/du-whpx-chat_completions. Native command/permission/Python/descriptor
regressions, host tests, host/UEFI Clippy, formatting and packaging pass.
The remaining required applets are df, tar and ln. General mount accounting,
installation manifests and retained sessions remain incomplete.

Upstream unzip now builds into the shared image with stored/DEFLATE members,
central-directory parsing, filters, overwrite policies and symlinks. Dynamic
archive descriptors preserve workspace fd 3. Extraction uses an anchored
destination directory, including metadata and symlink operations; outside
ancestor links cannot redirect writes. Staged fixes check stored CRC, output
lengths, local/central signatures, names and maximum-comment end records.
Regular files restore modes/times and directory restoration is deferred.
Native integration and both 512 MiB WHPX model API flows pass, with independent
saved bytes/modes/times/containment checks in artifacts/unzip-whpx-responses
and artifacts/unzip-whpx-chat_completions. Metadata, Python, command and
descriptor regressions, all host tests, host/UEFI Clippy and formatting pass.
ZIP64, encryption
and compression methods other than stored/DEFLATE remain unsupported.
The remaining required applets are df, tar and ln. Mount, manifest and
retained-session gates remain incomplete.

Upstream gzip/gunzip/zcat now build into the shared image with CRC32 and
direct stream transformers. Native Release integration and both 512 MiB WHPX
model APIs pass independent gzip/zlib stored/fixed/dynamic inputs, concatenated
members, optional headers and CRC, binary boundaries through 1 MiB, source
retention after errors and saved output decoding by host Python. Evidence:
artifacts/compression-whpx-responses and compression-whpx-chat_completions.
Staged fixes reject read failures, header CRC/reserved flags and corrupt member
lengths, and stream extra fields through 65535 bytes. Standalone applets now
enter the existing Asyncify jump driver. Native awk/sed/grep/streams/basic/text/
xargs/copy-move/mutation/PATH/CPython regressions, all host tests, host/UEFI
Clippy, format and syntax checks pass. Compression levels and long options are
enabled; upstream maps levels 1-3 to 4. Seamless non-gzip formats are disabled.
The remaining required applets are df, tar and ln. Mount, manifest
and retained-session gates remain incomplete.

Upstream xargs now uses cooperative spawn/wait with exact argument vectors,
inherited descriptors and concurrent children. Quotes, empty words, NUL input,
replacement, argument files, batching, trace and child-status propagation pass
native integration and both 512 MiB WHPX model APIs in
artifacts/xargs-whpx-responses and artifacts/xargs-whpx-chat_completions.
Staged fixes enforce -x, reject incomplete quoting and empty replacements,
report read/output errors, and drain parallel children without losing fatal
status. Upstream echo is also available as an external command. Interactive
tty options fail because /dev/tty is absent; the runtime's 32-process quota
still applies. All host tests, host/UEFI Clippy, format and Python/copy-move/
descriptor regressions pass. The wider environment requirements remain open.

Upstream cp/mv now replace the handwritten subsets and link shared copy-file,
inode-hash and coreutils stat helpers. Long options, recursive/archive copies,
symlink policies, prompts, update and multioperand moves work through ash.
Staged corrections prevent same-file truncation through destination symlinks,
fix --remove-destination's flag, report traversal/preservation/output failures
and suppress false move-success messages. Native and both 512 MiB WHPX model
APIs pass 50 semantic/boundary cases plus independent saved FAT bytes, modes,
symlinks and times in artifacts/copy-move-whpx-responses and
artifacts/copy-move-whpx-chat_completions. Basic/mutation/PATH regressions,
host/UEFI Clippy and formatting pass. Hard links, special node creation and
reflinks remain unavailable; cross-mount moves remain unverified. The broader
command, mount, manifest and retained-session requirements remain incomplete.

Installed commands and shared BusyBox aliases now appear in executable `/bin`
entries, including upstream which and the sh alias. Ash uses upstream PATH
lookup with builtin/function precedence and owner execute checks. Runtime
subprocess/env lookup supports PATH, explicit WASM paths and bounded absolute
shebangs. Child cwd participates in lookup; installed Python keeps its startup
name for unchanged library discovery under -I. Native Release and actual
512 MiB WHPX/Responses pass PATH ordering, empty components, shell precedence,
shebang options/errors and independent saved FAT script bytes/modes in
artifacts/program-paths-whpx-20261024. Workspace /bin entries remain visible;
command collisions reject installation before aliases are published. All WASM
tests, host/UEFI Clippy and native basic/descriptor/math/heredoc regressions pass.
The existing 17 CPython integration cases pass. General mount/link traversal,
persistent manifests, remaining applets and retained sessions remain incomplete.

Upstream BusyBox stat now builds into the shared image with file metadata,
custom/terse formats, link-following control and the fixed `agent` uid/gid
names. Filesystem-stat mode is disabled pending namespace capacity reporting.
Native ash/CPython integration and independent saved-workspace checks pass.
Actual 512 MiB Windows WHPX/OVMF passes both Responses and Chat Completions
tool flows with independent saved FAT metadata and byte checks in
`artifacts/stat-whpx-responses-final` and
`artifacts/stat-whpx-chat_completions-final`. Native Linux and Windows Release
checks include 64-bit epoch boundaries and buffered stdout failures.

Upstream BusyBox sort/uniq/cut/tr now build into the shared image. Length-aware
records preserve NUL and -z data. Sort fixes field-relative offsets, stable
record positions, large decimal precision, negative/zero human suffixes and
read failures. Uniq bounds skipped fields by actual input. Cut fixes positive
ranges, empty-line suppression, binary ordinary fields and read errors; regex
NUL or zero-length matches fail explicitly. Tr uses upstream byte classes and
equivalence sets. Native integration passes 142 semantic/property/boundary
cases and independent saved bytes. Wider command/mount/PATH/session delivery
remains incomplete.

Upstream BusyBox date now replaces its handwritten subset. It provides UTC
epoch/calendar/strptime parsing, RFC/ISO output, nanoseconds and file reference
times. Clock setting rejects ENOTSUP; non-UTC TZ values reject explicitly
because WASI libc otherwise silently uses UTC. Trailing -D input, clock errors,
oversized output and stdout errors fail. Upstream compare-string-array helpers
replace local duplicates. Full timezone support remains pending.
All 32 native date cases, current-time/calendar oracles and saved reference
bytes/times pass. Actual 512 MiB WHPX/Responses and TCG/Chat Completions pass
the combined current file/code-tool sequence and independent FAT checks in
artifacts/date-applet-whpx and date-applet-tcg. Release packaging, mutation,
basic/stream/awk/sed/grep and ash math/random regressions pass.

Upstream BusyBox od, hexdump and libbb dump support now build into the shared ash image.
Native Release run_guest and actual 512 MiB WHPX/Responses checks pass binary
octal/character/canonical output, custom formats, boundary lengths, skip/length,
duplicate suppression, read/output errors and independent saved FAT bytes.
Hexdump format-file read and close errors return failure. The runtime registry
and ash dispatch expose hexdump from the shared image.

Upstream BusyBox base64 encoding/decoding now builds into the shared ash image.
Native integration covers RFC vectors, independent Python byte oracles, wrapping
widths, binary data and 524289-byte streams through ash. Input and output errors
are reported; broken-pipe status follows the final pipeline command. The actual
512 MiB WHPX/Responses run passes with independent FAT checks for encoded and
decoded files in artifacts/base64-whpx.

Upstream BusyBox sha256sum and its checksum-file parser now build into the
shared ash image. The SHA algorithm and check semantics remain upstream; the
WASI support header supplies explicit little-endian hash context types and
unaligned access operations. Native Release integration checks independent
hashlib vectors and binary files, padding/block boundaries, 524289-byte and
1 MiB inputs, valid/invalid checksum files, missing/directory inputs and closed
stdout. Actual 512 MiB WHPX/Responses passes with independent saved FAT checks
in artifacts/sha256-whpx.

Upstream BusyBox realpath is registered in the shared ash image. It resolves
relative/absolute paths and directory symlinks; the WASI helper also preserves
upstream canonical output for a missing final component and dangling links.
Native and actual 512 MiB WHPX/Responses checks pass path normalization,
multiple inputs, missing-parent and trailing-slash errors, with independent
saved FAT checks in artifacts/realpath-whpx.

BusyBox mkdir/rmdir/rm/touch now use upstream applets and shared make-directory,
recursive-remove, confirmation and time helpers. Staged corrections report
directory traversal errors and new-file timestamp errors, preserve nonzero
status for verbose output failures, reject mkdir / or . without -p, and omit
false removal success messages. Native integration passes 63 command cases
and independently checks saved names, bytes and timestamps. The mount,
PATH, remaining applet inventory and retained-session gates remain incomplete.
Actual 512 MiB WHPX/Responses pass the full current
file/code-tool sequence and independent FAT checks in
artifacts/mutation-applets-whpx. All 85 WASM tests,
Clippy, formatting, Release packaging and shared-image guest regressions pass.

Upstream BusyBox basename/dirname/pwd/env/printenv/seq/yes/true/false now
build into the shared image. Local copies of path/single-argv/getcwd helpers
were replaced by upstream helpers. Pwd enables -L/-P; env uses cooperative
execution with explicit environments and preserves child/launch statuses.
An empty environment no longer dereferences clearenv's null pointer. Flush
results include existing stdio errors. A staged yes fix checks fputs failures,
so multiword producers exit after head closes. Without SIGPIPE delivery, yes
reports its EPIPE diagnostic; the final pipeline stage determines status.
Native integration passes 65 cases plus independent saved bytes. Actual
512 MiB WHPX/Responses and TCG/Chat Completions pass all current file/code-tool
checks in artifacts/basic-applets-whpx-fixed and basic-applets-tcg-fixed.
All 85 WASM tests, Clippy, format, Release packaging and shared guest
regressions pass. Remaining command/mount/PATH/session gates are tracked in
docs/environment-work.md.

Firmware monotonic time and upstream BusyBox sleep now pass actual 512 MiB
WHPX/Responses checks in
artifacts/monotonic-whpx-final-all. Both use PI CPU
timer 0 and report nominal 1 ns resolution through /caps. Standard Python
sleep, RTC comparison, ash timed reads, tail follow/EPIPE, sleeping-tree
cancellation and independent saved FAT bytes pass. Relative realtime waits
use monotonic time; absolute waits retain their requested clock. Counter wrap,
fractional conversion and separate-clock deadline tests pass among all 85
WASM tests. Upstream sleep supports fractional/summed s/m/h/d operands and
reports wait failures. The actual Timestamp protocol branch and physical
firmware remain unverified. Subprocess timeout/signal adaptation is pending.

BusyBox cat/head/tail/wc/tee now use upstream 1.37.0 applets and shared libbb
stream/numeric helpers. Staged fixes preserve NUL bytes and unterminated lines
in numbered cat and negative-line head, retain numbering across file joins,
read complete long tail lines, handle UINT_MAX counts, and report I/O failures.
Output queues now suspend on backpressure and return normal short writes
instead of rejecting data above 512 KiB. All iovecs are validated first;
private control messages retain their atomic size bound. Windows-native
stream integration passes 112 cases plus independent saved tee bytes.
Shared-image awk/sed/grep, permission, Python and descriptor checks pass.
Actual 512 MiB WHPX/Responses verify standard Python
subprocess stream capture, pipelines, errors and independent saved FAT bytes
in artifacts/streams-whpx-final. All 82 WASM tests,
Clippy, formatting and Windows Release EFI/launcher packaging pass.
The broader command, mount, PATH and session requirements remain incomplete.

BusyBox awk now uses the upstream 1.37.0 translation unit with SDK regex/math
and cooperative ash-backed system/popen/pclose. Windows native integration
checks 20 semantic cases and an independent 195000-byte pipe oracle; existing
ash math/test regression passes. This does not establish firmware awk behavior.
Other utility applets still include handwritten subsets and need upstream
replacement. The wider command/mount/session delivery remains incomplete.
Sed also uses its upstream parser and editor, plus upstream getopt32, linked
lists and line readers. Windows native checks pass 23 semantic/error cases
and temporary-file cleanup. In-place content/backup behavior is verified;
mode preservation now uses the private guest metadata interface described below.
Grep now uses upstream regex/options/context/recursive traversal. Windows
native integration passes 33 cases, including limits, failures and long lines.
Staged corrections reject -m 0 before reading and report input/output failures.
Recursive grep exposed virtual mount names in ordinary child directory views;
those views now retain their underlying namespace anchor without root mounts.
Runtime lookup/spawn accepts /bin-prefixed registered names and applet aliases.
Focused host checks cover compiled-only modules, lazy discovered aliases, and
rejection of absent/incorrect paths. Filesystem PATH and executable bin entries
still require implementation.

BusyBox now uses upstream chmod/parse_mode, effective stat/chmod/fchmod wrappers,
and inherited runtime umask/cwd. The private Python filesystem adapter exposes
the same namespace modes through standard os methods, including fd-relative
operations and scandir entry metadata. Directory-fd scanning retains an anchor
after the caller closes its fd or renames the directory. Namespace ownership is
fixed at uid/gid 1. Windows native integration verifies permissions, open-handle
lifetime, symbolic/recursive chmod, sed backup modes, umask/cwd inheritance,
persisted metadata and process restart. The C wrapper fixture distinguishes
AT_FDCWD from explicit root fd 3. Runtime creation-mode scopes preserve modes
through dangling links and are restored after errors. Final 512 MiB QEMU/OVMF
checks pass WHPX/Responses with independent saved FAT
mode/byte checks in artifacts/permissions-whpx-final.
All 15 CPython integration cases pass,
including bytecode cache creation/reuse and -B. Current directory state is still
path based; rename/unlink tracking remains pending. Python subprocess now passes
cwd and umask through its private process adapter; the native Release runner
passes all 17 CPython integration tests, including relative/absolute cwd,
missing cwd errors, umask inheritance and per-child overrides. This change has
passed the full Windows WHPX filesystem-tool sequence with independent saved
FAT checks. Firmware ash cwd behavior also passed. Native tests verify import
caches, explicit py_compile/compileall and -B behavior. WHPX verifies import
cache creation/reuse, py_compile, compileall, -B and independent saved FAT
bytecode headers. Scandir entries are private proxies
with the standard methods, rather than instances of the built-in DirEntry type.
The full mount/session/command delivery remains incomplete.

The common no_std efi-agent-filesystem crate now owns provider interfaces and
a persistent inode namespace. Two checksummed metadata generations preserve
directory entries, symlinks, modes and times above FAT backing files. Anchored
directory views survive rename; detached open files retain identity until the
last close. Native-backed integration covers recovery, permissions, links and
structured mutations. WASI mutation, readlink, stat lookup flags and timestamp
imports delegate to providers; unsupported providers return explicit errors.
Firmware and the native guest runner now install the namespace. Model file
tools and module loading use that same tree. Launcher staging and export use
logical snapshots; raw FAT paths are backing storage, not the guest tree.
The launcher suite verifies FAT stage/export/save/reload with empty directories,
links, modes, timestamps and deletions. The Release native runner passes all
15 CPython integration cases. Read-only closes defer access-time persistence;
explicit file/workspace flush persists it. Writable-close errors are retained
and reported at the execution boundary. Virtual mounts, applets,
sessions and the remaining delivery in docs/environment-work.md are incomplete.

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
and exposes print and a Promise-based $ command template, quoting each
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
Timeouts, signals, cwd, process groups/credentials and distinct custom argv[0]
are unsupported.
The runtime still executes registered names rather than filesystem PATH.
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
WHPX was not tested in this run.
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
missing issuers, wrong-IP and expired certificates. Certificate failures are
curl error 60; unavailable firmware entropy rejects TLS before HTTP with 35.
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
BusyBox 1.37.0 sources are staged under artifacts/guest-sources. The actual ash
translation unit now compiles to a wasm32 object through check_busybox.ps1 and
prepare_busybox.py. build_busybox.ps1 now links and Asyncify-transforms ash.wasm.
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
Filesystem PATH, ulimit/umask and full filesystem support remain incomplete. The real
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
Native rename reaches the provider; firmware rename/delete, timestamps and
filesystem PATH remain pending. File fd_write validates every iovec before
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
statistics, cancellation, and the read/write/edit/shell/python tool loop. There is no relay,
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
recorded complete regression is artifacts/wasm-applications-descriptors-512.
The 2026-10-12 commit review uses existing evidence; it does not rerun tests.
Guest source acquisition and automatic guest packaging remain pending.

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
A normal prompt runs at most twelve rounds of executed read/write/edit/shell/python tools.
The model can run shell commands through `shell(command)` and Python source
through `python(code)`. The runtime loads workspace ash.wasm once on first
code use and python.wasm once on first Python use, reusing registered modules.
Each call uses a fresh process with the shared workspace and empty stdin.
Source is passed unchanged as one argument (60 KiB limit, no NUL). Results
contain readable exit status and stdout/stderr, capped at 64 KiB per stream
with explicit truncation. The existing tool disclosure holds the result.
Tool completion or cancellation cancels and reaps its remaining process tree;
independent background roots continue. Original root IDs survive adoption.
The prompt specifies an Alpine Linux-style BusyBox ash environment, POSIX
syntax and actual WASM limits; it does not claim a Linux kernel or apk support.
All 117 core/wasm tests pass. Actual 256 MiB QEMU/OVMF checks pass both
provider APIs' code-tool sequences and focused Esc tree cleanup with an
independent background root. Evidence: artifacts/model-tools-chat,
artifacts/model-tools-responses-final and artifacts/model-tools-cancellation.
The Responses sequence passed before a CRLF background fixture failed;
the corrected LF fixture passes the focused rerun. Release EFI and Clippy
pass. The full network application combination was not rerun for these tools.
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
