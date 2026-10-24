# Verification record

## 2026-10-24: Upstream df and measured filesystem statistics

BusyBox 1.37.0 df now builds into the shared image with its upstream parser,
mount selection, unit formatting, percentages and inode mode. Its only staged
command change propagates buffered stdout failures. The upstream mount-point
helper consumes the runtime inventory through a private mount-table snapshot.
Windows uses actual volume capacity/allocation-unit queries; UEFI uses
EFI_FILE_SYSTEM_INFO. Total free bytes and caller-available bytes are distinct.
Workspace file counts describe its shared namespace entry-slot quota; aliases
and detached open inodes retain slots. The /bin overlay shares that filesystem.
Zero-capacity dev/proc mounts appear with -a or explicit operands.

Standard C statvfs/fstatvfs and private Python os.statvfs/os.fstatvfs adapters
consume the same fixed-width statistics reply. CPython remains prebuilt.
Private operation 26 checks path/fd filestat rights and returns twelve u64
fields in a close-on-exec snapshot. Operation 27 snapshots escaped mount fields.
Native ABI tests verify capacities beyond 32 bits, reserved-space distinctions,
both rights paths, malformed flags/NUL names, unavailable descriptors, provider
invariants and exact descriptor-quota exhaustion. Mount snapshots verify their
inventory and field escaping. Evidence: `artifacts/df-abi-tests.log`.

The shared /tmp quota is 32 MiB and 4096 live file objects. Ordinary writes
return short counts at exhaustion; resize rejects growth before changing data.
Truncation and last close release charges. Unlinked files and replaced open
targets retain their charges. Native tests distinguish full/one-byte-available,
overwrite/growth, empty writes beyond EOF, shared directory views and retained
inode quota at its exact limit. All nine mutation integration cases pass in
`artifacts/df-mutation-tests.log`. All 26 filesystem integration cases pass in
`artifacts/df-filesystem-tests.log`, including workspace slot retention and
host capacity comparisons.

Native Release df integration checks file/directory paths, symlinks, missing
and denied inputs, unit and POSIX formatting, human output, mount/type filters,
inode output, closed stdout, repeated descriptor cleanup, and open unlink on
workspace/tmp files. Independent host volume queries check reported totals and
free/available values after execution; saved output bytes and names are checked
separately. Evidence: `artifacts/df-native.log`.

The final shared image passes actual 512 MiB WHPX/OVMF through Responses and
Chat Completions. Independent saved FAT checks count data clusters and free
FAT entries, compare capacity snapshots, and verify retained bytes, links and
deleted names. Evidence: `artifacts/df-whpx-responses` and
`artifacts/df-whpx-chat_completions`, with completed host logs beside them.

All host workspace tests, host/UEFI Clippy, formatting and Release EFI/launcher
packaging pass in `artifacts/df-rust-tests.log`, `artifacts/df-clippy-host.log`,
`artifacts/df-clippy-uefi.log`, `artifacts/df-format.log` and
`artifacts/df-package.log`. Focused suites include the subsequently added quota
and slot cases. Native regressions pass ln, du, stat, copy/move, ls, mutation,
compression, unzip, basic/text commands, xargs, all 17 CPython cases, PATH,
permissions/C metadata, ash descriptors/math, awk/sed/grep and streams in
`artifacts/df-regression-*.log`. Modified Python syntax passes in
`artifacts/df-python-syntax.log`.

Corrections and execution issues:

- The previous capacity interface omitted total free bytes and Windows assumed
  a 4096-byte allocation unit. Both values now come from volume queries.
- Initial source integration needed the upstream suffix-table declaration.
  WASI libc already defines statvfs/fstatvfs, so the adapter uses linker wraps
  instead of duplicate public symbols. Temporary-storage refactoring initially
  missed one cross-view rename map access; compilation identified it.
- An initial fixture used -B1P, which passes the invalid size 1P. The corrected
  invocation supplies -B1 and -P separately. A Python statvfs result must omit
  the internal fsid field from its ten positional fields; fsid is extra metadata.
- FAT reader introspection established the actual boot-sector and FAT entry
  interfaces. The saved-volume verifier counts actual free data clusters.
  Expected not-yet-created logs and absent process names produced inspection
  errors during progress checks; the live test handles were retained and polled.
- Code review removed an unnecessary percentage-initialization change. Final
  guest sources retain the upstream percentage algorithm.

Tar, stat -f adaptation, general mounts/link traversal, persistent manifests,
retained sessions and final full environment verification remain incomplete.

## 2026-10-24: Upstream ln and persistent hard-link identity

BusyBox 1.37.0 ln now builds into the shared image. Namespace directory entries
can share one file or symlink inode. Native Release integration verifies hard
and symbolic links, force and backup policies, verbose output, directory targets,
-n/-T, multiple sources, denied destinations and closed stdout. Source-entry
protection includes alternate parent paths. Empty backup suffixes and backups
that would overwrite a source fail before changes. Verbose success lines follow
successful creation and output failures return failure.

Python os.link supports independent source/destination directory fds and explicit
symlink-following control through private operation 25. The upstream interpreter
binary remains unchanged. Actual C link, cp -l/-a and du default/-l behavior use
the same inode identities. An independent reader checks saved inode groups,
single backing objects, bytes, modes, symlink targets and deleted names.
Evidence: `artifacts/hardlinks-native.log` and
`artifacts/hardlinks-busybox-build.log`.

Actual 512 MiB WHPX/OVMF passes Responses and Chat Completions, including the
preceding basic applet suite. Both runs independently verify saved FAT inode
groups, shared backing paths, binary data, modes, symlink targets and removed
names. Evidence: `artifacts/ln-whpx-responses` and
`artifacts/ln-whpx-chat_completions`, with completed host logs beside them.

Namespace integration verifies shared writes, modes, allocation and link counts;
primary/alias deletion; anchored directory rename; replacement while the former
target remains linked/open; same-inode rename as a no-op; symlink hard links;
last-close reclamation; and journal recovery. A structured 180-step mutation
model independently tracks groups and contents across restarts. Import groups
physical host hard links by device/inode identity and commits logical names
before removing redundant raw aliases. A failed initial flush leaves aliases
intact for recovery; an interrupted link commit retains the prior generation.
All 25 filesystem integration cases pass in
`artifacts/hardlinks-filesystem-tests.log`.

Launcher integration passes 11 snapshot/save cases, including an actual FAT
pack/extract/restage round trip with file and symlink hard links. Save creates
physical host hard links, rebuilds aliases after canonical replacements, and can
split/rejoin groups. Host group changes conflict even when bytes and timestamps
match. Snapshot buffers share data and byte limits count each group once;
invalid groups fail before staging effects. Evidence:
`artifacts/hardlinks-launcher-tests.log`.

WASI and private endpoint tests verify both directory rights, independent fd
anchors, follow flags, malformed lengths, ranges, NUL names, unavailable providers
and provider errors before effects. All host workspace tests pass in
`artifacts/hardlinks-rust-tests.log`; the two subsequently extended suites pass
in their focused logs. Host/UEFI Clippy, formatting and Release EFI/launcher
packaging pass in `artifacts/hardlinks-clippy-host.log`,
`artifacts/hardlinks-clippy-uefi.log`, `artifacts/hardlinks-format.log` and
`artifacts/hardlinks-package.log`.

Stat, ls, copy/move, du, mutation, PATH, compression, unzip, all 17 CPython cases,
permissions/C metadata, awk/sed/grep, streams, basic/text commands, xargs, ash
descriptors and math pass native regressions in
`artifacts/hardlinks-regression-*.log`. Modified Python source syntax passes in
`artifacts/hardlinks-python-syntax.log`.

Corrections and execution issues:

- The directory rights mask omitted PATH_LINK_SOURCE/TARGET. Both rights are
  now exposed and restricted-directory tests verify independent enforcement.
- The prebuilt Python os.link rejected directory-fd arguments. The private
  startup adapter supplies those standard arguments without an interpreter build.
- Initial ln compilation needed the upstream is_directory declaration and
  translation unit. Initial tests used a nonexistent FileError variant and
  expected EINVAL for NUL paths; the existing capability resolver returns
  ENOTCAPABLE. The unavailable-provider test now expects ENOTSUP after the new
  rights pass. Clippy identified an oversized helper argument list and a map
  entry pattern; both were corrected.
- The first WHPX invocation selected the generic code-tool branch and checked
  unrelated default output names. The harness now uses its selected applet-mode
  count for that branch. That attempt was not counted as a passing applet gate.
- The ash math regression invocation initially omitted its child fixture. It
  passed with the required fixture. A log inspection used an invalid multi-file
  tail option form; the explicit line-count form succeeded.

Df, tar, general mounts, persistent manifests, retained sessions and final full
environment verification remain incomplete. Cross-mount hard links and du -x
require the mount gates.

## 2026-10-24: Upstream du and measured file allocation

The shared image builds BusyBox 1.37.0 du. Native Release integration verifies
apparent and allocated sizes, totals, depths, human/explicit units, command-line
and recursive symlink policies, directory deduplication, missing/dangling inputs,
denied traversal and closed stdout. Boundary files cover 0/1/511/512/513,
4095/4096/4097, 65535/65536/65537 and 524287/524288/524289 bytes. An 8 MiB
Windows sparse hole distinguishes measured allocation from rounded logical
length. An independent Windows path query checks the provider's handle query;
saved contents and reported blocks are verified separately. Evidence:
`artifacts/du-native.log` and `artifacts/du-build.log`.

Actual 512 MiB WHPX/OVMF passes Responses and Chat Completions, including the
preceding basic command suite. The host independently follows each saved FAT
cluster chain and compares its allocated bytes with guest stat and du results.
Evidence: `artifacts/du-whpx-responses` and
`artifacts/du-whpx-chat_completions`. Firmware uses EFI_FILE_INFO PhysicalSize.
The unchanged CPython binary receives block counts through its private startup
adapter. Logical namespace directories and symlinks use shared journals, with
zero independently allocated data blocks. Unknown provider allocation remains
unsupported, rather than being inferred from file length.

Namespace integration verifies host allocation through growth, truncation,
rename, snapshot, path/fd access and open unlink. Directory link counts include
immediate subdirectories and remain correct through rename/removal. The private
allocation ABI test distinguishes zero, unsupported and values above 32 bits;
it checks missing files, malformed flags/NUL names, descriptor type, filestat
rights and exact descriptor-quota exhaustion. Reply snapshots close on exec;
repeated Python stat calls verify client cleanup.

Stat, ls, copy/move, mutation, PATH, compression, unzip, all 17 CPython cases,
permissions/C metadata, awk/sed/grep, streams, basic/text commands, xargs and
ash descriptors pass native regressions in `artifacts/du-regression-*.log`.
All host workspace tests, host/UEFI Clippy and formatting pass in
`artifacts/du-rust-tests.log`, `artifacts/du-clippy-host.log`,
`artifacts/du-clippy-uefi.log` and `artifacts/du-format.log`. Final Release
runner, EFI and launcher packaging pass in `artifacts/du-runner-build.log`
and `artifacts/du-package.log`. Modified Python and guest-fixture syntax pass.

Corrections and execution issues:

- WASI libc synthesized allocated blocks from logical file size. Metadata now
  retains an optional provider measurement and exposes it through a private
  u64 snapshot; standard C and Python stat methods use that measurement.
- Directory links were always one, preventing upstream directory identity
  deduplication. Linked directories now report two plus their subdirectories;
  detached open directories still report zero.
- Du now enables its existing upstream ceiling code for explicit units.
  Its directory read/close failures retain failure status. A missing cleanup
  helper declaration was added with the upstream signature.
- An initial scandir test followed a deliberately dangling link. The boundary
  metadata check now requests lstat semantics. The FAT library inspection used
  an incorrect class name initially; DirectoryEntry exposes the actual runs.
- The ABI test initially had a WAT delimiter error and requested the unavailable
  fd_fdstat_set_rights import. It now opens an actual restricted directory to
  test rights. Descriptor exhaustion returns positive WASI MFILE.

Df, tar, ln, general mounts, manifest/session delivery and final networking
regression remain incomplete. Du hard-link and cross-mount behavior require
those provider gates before full environment completion.

## 2026-10-24: Upstream unzip with anchored extraction

The shared image uses BusyBox 1.37.0 unzip, ZIP32 central-directory parsing and
the upstream DEFLATE decoder. Stored and deflated symlinks, stdout/test/list
modes, filters, path flattening, suffix lookup, overwrite prompts and policies
pass native Release integration. Independent host Python ZIP inputs and saved
workspace checks verify binary contents, executable modes, directory modes,
symlink targets and 2040 leap-day timestamps. Stored boundaries cover
65535/65536/65537 and 524287/524288/524289 bytes. End-record comments cover
0/65513/65514/65535 bytes. Empty archives and finite stdin pass.

Containment checks distinguish safe internal links from relative and absolute
ancestor links outside the extraction directory. Leaf-link replacement and
deferred outside-link pivots fail while independent saved bytes remain intact.
Upstream lexical prefix removal is retained. Each extraction uses a retained
directory descriptor for path opens, directory creation, stat, modes, times
and symlink creation. Deferred directory attributes are restored deepest first.
ZIP64, encryption and methods other than stored/DEFLATE remain unsupported.

Corrupt stored CRC, stored/deflated sizes, local/central signatures, embedded
NUL names, truncated inputs, unsupported methods, missing files and closed
stdout return failure. Evidence: `artifacts/unzip-native.log` and
`artifacts/unzip-build.log`. Native permissions/C metadata, copy/move,
mutations, PATH, compression, 17 CPython cases, ten ash descriptor cases,
awk/sed/grep, streams, basic/text and xargs regressions pass in
`artifacts/unzip-regression-*.log`. All host workspace tests, host/UEFI Clippy,
Rust format and modified Python syntax pass. Release EFI/launcher packaging
passes in `artifacts/unzip-package.log`.

Actual Windows WHPX/OVMF at 512 MiB passes Responses and Chat Completions,
including the preceding basic command suite and independent saved FAT checks.
Evidence: `artifacts/unzip-whpx-responses` and
`artifacts/unzip-whpx-chat_completions`. Saved bytes are checked with a separate
host namespace reader and the host standard ZIP decoder.

Integration corrections and execution issues:

- The upstream fixed archive fd 3 would replace the workspace preopen. Archive
  input now uses its actual opened descriptor or a duplicate of finite stdin.
- Stored entries now check CRC and exact sizes; deflated length errors fail
  instead of printing a warning with successful exit. Write/close errors fail.
- Central-directory and local signatures, real end-record bytes, maximum ZIP
  comments and offset arithmetic are checked before trusting their fields.
- The build initially lacked chomp's declaration and then had an incorrect
  return type. The declaration now matches the upstream void function. A
  duplicate copy helper was removed in favor of the existing upstream helper.
  Extraction state is defined in metadata.c so the separate C fixture links.
- An initial uv syntax probe omitted the Python executable. The corrected
  invocation passes. The descriptor regression initially used the general
  Python arguments and then the wrong fixture directory; the corrected call
  uses its dedicated child fixture in artifacts/wasm-fixtures and passes.

General mounts, remaining commands, installation manifests and retained tool
sessions remain incomplete. Complete networking regression remains pending.

## 2026-10-24: Upstream gzip, gunzip and zcat

The shared image now builds BusyBox 1.37.0 gzip, bbunzip, DEFLATE decompression,
CRC32 and direct transformer helpers. Compression levels and long options are
enabled. Upstream maps levels 1-3 to level 4. Zcat reads gzip directly; the
fork-based seamless multi-format transformer is excluded. The unchanged
CPython interpreter and its standard library remain untouched.

Native Release integration passes independent Python gzip/zlib inputs with
stored, fixed and dynamic Huffman blocks, concatenated members, binary data,
optional names/comments, header CRC and extra-field sizes
0/16383/16384/16385/65535. Wrong header CRC, reserved flags, unknown methods,
invalid blocks, truncation, CRC and size failures return failure. A damaged
member followed by a valid one still fails. File checks cover -c/-k/-f/-t,
multiple inputs, .tgz renaming, unknown suffixes, mode/timestamp propagation,
deletion on success, and retained sources/deleted partial destinations on
integrity errors. Stream-input and closed-output failures return failure.
Round trips cross 32767/32768/32769, 65535/65536/65537,
524287/524288/524289 and 1048576 bytes. A host Python gzip decoder independently
verifies saved outputs, payloads, modes, times and names. Evidence:
`artifacts/compression-native.log` and `artifacts/compression-build.log`.

Actual 512 MiB Windows WHPX/OVMF passes the same command sequence under both
Responses and Chat Completions, including independent host decoding of saved
FAT outputs and saved metadata/name checks. Evidence:
`artifacts/compression-whpx-responses` and
`artifacts/compression-whpx-chat_completions`. Windows Release EFI and launcher
packaging pass in `artifacts/compression-package.log`. Python source and guest
fixture syntax checks and Git whitespace checks pass.

High-risk tests distinguish an incomplete optional field from a valid field
larger than the internal buffer, an early corrupt member from later success,
and a real source deletion from an error path. Expected compressed inputs come
from the host standard library; host output checks do not call guest code.

Integration corrections:

- Gzip previously treated read errors as EOF. Read failures now fail explicitly.
- Decompression validates reserved flags and header CRC, streams large extra
  fields, retains trailing read errors and stops after an invalid member size.
- Independent applets previously bypassed the Asyncify setjmp/longjmp driver;
  gunzip's first setjmp trapped. All applet entries now use the existing driver.
  Its error helper retains a return epilogue for Asyncify unwinding.
- The initial build lacked gzip's global-state pointer, shared declarations
  and unaligned-memory configuration. These now match wasm32 and upstream.
  One staging marker was corrected to include the actual switch branch.
- A shell quoting error broke an initial Python syntax probe. A literal
  PowerShell script and Python source string resolved it. Packaging initially
  lacked EFI_AGENT_MODEL; explicit local test settings resolved it.
- Existing upstream string-offset warnings remain nonfatal; QEMU reports
  nonfatal interrupt-vector and host application-manifest warnings.

Awk, sed, grep, streams, basic commands, text commands, xargs, copy/move,
mutation, PATH and 17 CPython cases pass native regression. All host workspace
tests, host/UEFI Clippy and Rust formatting pass. Complete network application
regression remains a separate delivery gate. General mounts, remaining applets,
installation manifests and retained tool sessions remain incomplete.

## 2026-10-24: Upstream xargs with cooperative children

The shared BusyBox image now uses upstream xargs with exact argument vectors,
cooperative spawn/wait, inherited descriptors and independent parallel child
tracking. The default command uses an externally installed upstream echo;
ash's builtin precedence is unchanged. Native Release integration checks
quotes/escapes, empty words, NUL records, end markers, replacements, trace,
argument files, size/count batching, -x boundaries, invalid options, input and
output errors, executable lookup and statuses 123/124/126/127. Stream checks
cross 65535/65536/65537 and 524289 bytes. One hundred serial batches verify
reaping beyond the live-process quota. Two workers each wait for the other's
start file, so serial execution cannot satisfy the -P2/-P0 concurrency check.
A fatal worker and a delayed failing worker verify draining and retained status
124. Saved completion files and executable modes are checked independently.

Windows Release EFI/launcher packaging and actual 512 MiB WHPX/OVMF pass
Responses and Chat Completions tool flows in `artifacts/xargs-whpx-responses`
and `artifacts/xargs-whpx-chat_completions`. Both independently verify saved
FAT bytes and modes. All host workspace tests, host and UEFI Clippy, formatting,
17 CPython cases, 50 copy/move cases and ten ash descriptor cases pass.
The complete networking application combination was not rerun in this step.
Interactive -p/-o fail explicitly without /dev/tty. Parallelism remains subject
to the runtime's 32-process quota, including ancestors and other roots.

Corrections and verification issues:

- Upstream -x was parsed but ignored. Partial-word and exactly full batches now
  reject additional input when the requested argument count has not been met;
  EOF/trailing whitespace and count-limited subsequent batches still succeed.
- Empty quoted words are preserved; unfinished quotes/escapes and empty -I
  replacement strings fail. Read and trace/output errors propagate.
- Spawn errno is saved before diagnostics. Fatal parallel status is retained
  while all outstanding children are drained.
- Linking libc dup introduced a duplicate dup2. The port's dup now uses the
  existing descriptor-duplication adapter. A duplicate ISSPACE macro was removed.
- Initial fixture expectations assumed different usage/read-error wording;
  assertions now check stable context together with exit status and effects.
- Existing upstream string-offset compiler warnings remain nonfatal.

## 2026-10-24: Upstream copy and move applets

Cp and mv now link the upstream 1.37.0 parsers, copy_file, inode_hash and
coreutils stat helpers. Native Release integration passes 50 semantic/error
cases, including binary copies at 65535/65536/65537 and
524287/524288/524289 bytes. Independent guest assertions check no-clobber,
interactive/forced replacement, source preservation after errors, directory
recursion containment, symlink/dangling policies, archive modes/times, --parents,
multiple sources, -t/-T, update, verbose output errors and retained old bytes
after replacement. The host reader independently checks saved bytes, modes,
links, timestamps and deleted names.

Actual 512 MiB Windows WHPX/OVMF with qemu64,+rdrand passes both Responses and
Chat Completions code-tool flows with the same 50 cases and basic/find applet
regressions. Independent FAT checks pass in
`artifacts/copy-move-whpx-responses` and
`artifacts/copy-move-whpx-chat_completions`. Existing native basic, mutation
and PATH integration passes. Host and UEFI Clippy, formatting and Python
syntax checks pass. The shared ash image was rebuilt with Windows WASI SDK 34
and Binaryen 133. No Rust production source changed in this step.

High-risk checks distinguish same-file aliases from separate contents, a
nonwritable target from a forced replacement, a recursive child destination
from a separate tree, and success diagnostics from real copy/move results.
The underlying namespace byte/mode oracle is independent of the applets.
Destination-alias copies fail before truncating the source. Runtime cancellation
and the complete networking/TLS combination were not rerun for this change.

Build and integration issues corrected:

- Configuration scanning initially omitted the new translation units, leaving
  disabled feature macros undefined. It now scans their upstream feature uses.
- A source-patch marker matched multiple move branches; it now identifies the
  complete cross-filesystem copy/remove condition.
- --remove-destination used the same flag as --parents in the imported helper
  enum. The flag now matches the upstream getopt allocation. A separate
  destination-symlink alias check prevents source truncation before a POSIX copy.
- Existing upstream string-offset compiler warnings remain visible and do not
  prevent the Windows WASM build.
- The command ledger omitted ls, rm and find from its required inventory.
  Those existing requirements are now listed explicitly.

Hard-link creation/preservation, special node creation and reflinks still need
provider support. They are not reported as available; automatic hard-link and
reflink features are disabled. Cross-mount fallback awaits complete mounts.
The wider environment delivery and retained sessions remain incomplete.

## 2026-10-24: Executable PATH, installed commands and shebangs

Upstream ash now searches executable filesystem entries with its normal
builtin/function precedence and hashing. The shared image includes upstream
which, and installed commands expose read-only `/bin` entries with shared image
bytes, metadata and file-backed symlink targets. Unrelated workspace `/bin`
files remain visible. Installation checks name collisions before publishing
aliases; a deterministic namespace test verifies rejection and retry without
losing the workspace entry.

Runtime subprocess/env resolution checks child PATH and cwd, owner execute bits,
explicit WASM filenames and bounded absolute-interpreter shebangs. Tests use two
different executable outputs to distinguish PATH order and skip a denied first
candidate. They cover empty PATH components, temporary assignments, builtin and
function precedence, exec, which -a, Python subprocess environments/cwd,
`#!/bin/python -I`, malformed images, interpreter loops and permission errors.
An additional Rust test checks real WASM execution, denied modes after module
compilation and symlink-parent `..` semantics.

Windows Release native integration passes the PATH fixture and independent
saved script bytes/modes. All 17 existing CPython integration cases pass with
the unchanged prebuilt interpreter. Native basic applets, all 10 descriptor,
20 math and 26 here-document cases pass. All native workspace tests and host/UEFI
Clippy pass with warnings denied. Windows Release EFI/launcher packaging passes.

Actual QEMU/OVMF with Windows WHPX, qemu64,+rdrand, 512 MiB and Responses passes
the focused shell/Python code-tool flow and upstream basic/find regressions.
The independent FAT reader verifies script contents, the persisted denied
execute mode and the final result in `artifacts/program-paths-whpx-20261024`.
This run uses the HTTP fixture. The complete networking/TLS application
combination was not rerun. General mount-boundary/link traversal, persistent
installation manifests, remaining applets and retained sessions remain pending.
Compiled-only host registrations expose executable metadata but have no readable
source image. Installed modules retain the existing compile-on-first-use cache.

Issues corrected or bypassed during this task:

- Packaging required API settings; process-local fixture values supplied them.
  The interpreter binary and standard library remain unchanged. Retaining the
  installed `python` startup name fixed `-I` library discovery.
- Early test expectations assumed command-v returned 1 for a missing command
  and an unexported PATH affected a child which process. They now check ash's
  status 127 and pass command-local PATH explicitly.
- The first which build called a nonexistent flush helper. It now uses the
  existing upstream flush-and-exit helper. Existing upstream string-offset
  compiler warnings remain visible; they did not block the WASM build.
- An incorrect uv dependency request for the standard-library py_compile module
  was replaced by an ordinary uv Python syntax check. PowerShell text sent to
  Bash was rerun through pwsh.exe with a Windows-local directory.
- Endpoint/provider fixtures initially rejected the new installation-time /bin
  inspection. They now allow that inspection while retaining execution-time
  no-effect assertions. A missing descriptor fixture at the guest-output path
  was resolved by using its existing wasm-fixtures path.
- An earlier broader WHPX attempt stopped after its clock check and was
  interrupted; its remaining QEMU process was terminated. No passing evidence
  or root cause is claimed for that attempt. The focused run above passed.
- Git commands inspecting the dirty submodule stalled. Explicit file staging
  and ignored submodule inspection preserve vendor and user workspace changes.

## 2026-10-22: Mounted namespace and logical workspace persistence

Firmware and the native runner now mount the persistent namespace. Model file
tools and module loading share it. Launcher staging and save use logical
snapshots, including deletions, empty directories, links and metadata, with
conflicts checked before effects. Protected Windows-safe link placeholders
retain virtual metadata without requiring host symlink privileges.

Windows-native verification passes 18 filesystem and 19 launcher tests. The
launcher tests include actual FAT pack/extract/export/save/reload/restage and
same-size, same-mtime host replacement detection. The filesystem cases include
deferred read-close access times, explicit read-only flush persistence and
writable-close failure recovery. Host all-target Clippy and UEFI target Clippy
pass with warnings denied.

All 14 CPython integration cases pass with the Release run_guest runner and
the unchanged prebuilt interpreter, including standard imports, descriptor
mapping, concurrency, duplex/capture boundaries, nested directories, 8 MiB
writes, finite stdin and bytecode compilation. Result checks independently
decode the CRC-checked namespace generations and backing bytes instead of
assuming logical filenames are physical host paths. Native module discovery
also uses the logical namespace. The real native timestamp source is enabled.

The existing focused Responses QEMU/OVMF TCG 512 MiB run in
artifacts/namespace-tools-tcg-512 verifies nested model writes, shared
shell/Python files, finite stdin, mutation/open-file identity and independent
logical FAT bytes. That run predates the latest flush changes. Current binary
WHPX verification passes the same complete Responses sequence at 512 MiB with
qemu64,+rdrand in artifacts/namespace-tools-whpx-512, including cancellation,
the next request and independent saved FAT content. The current binary also
passes Chat Completions under TCG with max at 512 MiB in
artifacts/namespace-tools-current-tcg-512. The native libc/ash/
QuickJS filesystem regression also passes file cursors, append/resize,
directory cookies and positioning, Unicode globbing, random bytes, fresh-task
file sharing and language capture against logical namespace bytes. Virtual
mounts, BusyBox applet delivery, command resolution and persistent sessions
remain incomplete.

Issues corrected during this integration:

- The Debug runner exceeded its 60-second execution deadline for 11 of 14
  Python cases. The same suite passes in 34.8 seconds total with the Release
  runner and real namespace timestamps. The execution deadline is unchanged.
- Tests read newly created logical files at raw host paths and failed with
  FileNotFoundError. Independent namespace readers now check the actual bytes.
- Read-only closes previously rewrote the whole journal for access times.
  They now defer persistence; explicit flush retains its metadata semantics.
- A new test assumed OpenOptions implements Default. It now supplies the
  explicit supported options; the complete suite passes.
- An earlier native integration used `?` with FileError as a standard Error;
  contextual conversion fixes the compile failure. An initial command used
  the wrong launcher package name; subsequent commands use efi-agent.
- One earlier build accidentally invoked Linux Cargo in the Windows tree.
  All results reported here use Windows Cargo from the Windows project path.

## 2026-10-22: Persistent filesystem foundation and WASI mutations

The follow-up common filesystem suite has 14 passing cases. Its additional
read-only export test verifies logical renamed paths, empty directories and
link targets while preserving both journal slots and detached open backing
files. Export reads the latest committed generation without recovery writes.
Host filesystem Clippy and UEFI target Clippy pass. Firmware raw unlink/rmdir
now check entry type, protect root, reject nonempty directories and consume
EFI Delete results; capacity reads EFI_FILE_SYSTEM_INFO. These raw operations
are compile-checked only and still require actual firmware verification.

Windows-native `cargo test -p efi-agent-filesystem -p efi-agent-wasm` passes.
The common filesystem crate has 13 native-backed integration cases. They cover
open-file unlink/replacement lifetime, anchored directory rename, mode checks,
timestamp persistence, relative/absolute/dangling/cyclic links, recovery from
partial journal writes and failed flush, protected metadata collisions, partial
initialization, and 160 structured mutations against an independent map with
periodic restart. Clock failure is checked before resize/read effects.

The WASI filesystem suite has five passing integration cases, including a
namespace-backed actual WAT guest. It verifies directory identity after rename,
creation through links, nofollow errors, stat/lstat types, truncated readlink,
path and descriptor timestamps, unlink with an open descriptor and directory
removal with an open view. Optional-operation tests now expect provider errors
for newly granted mutation rights. All existing WASM unit/integration tests pass.

The common provider types are extracted into a no_std crate. Native raw
providers now expose actual timestamps, supported modes and capacity, and raw
unlink/rmdir for backing reclamation. The persistent namespace remains outside
normal firmware/native-runner mounting. Its logical snapshot must be used for
launcher export before mounting it in normal operation. No new firmware
mutation or BusyBox applet claim follows from these native checks.

Issues corrected during this work:

- An extraction script omitted directory creation and continued after a write
  error. The exact original was recovered from Git before extraction resumed.
- Rust rejected an inner module comment below extern crate; it now precedes it.
- Windows FlushFileBuffers denied read-only handles. Read-only file flush now
  succeeds without requesting a write-only OS operation.
- Clippy found an unused unit expression and a collapsible condition; both
  were corrected.
- Existing optional-operation tests expected denied rights. They now verify
  unsupported provider errors after mutation rights were implemented.
- The new WAT test requested unimplemented rights by mistake. The exact
  supported union corrected it; the guest then passed the complete sequence.
- A source read used the old filesystem test filename; the current test file
  was located with rg. A documentation patch used an incorrect heading;
  no changes were applied, and an exact-context patch replaced it.

## 2026-10-06: Single-vCPU VM CPU configuration

The launcher explicitly selects one vCPU with `-smp 1`. Windows WHPX uses
`qemu64,+rdrand` instead of `max`; Linux KVM retains `host`. RDRAND remains
required for TLS. Windows Release launcher build and actual WHPX DNS/HTTPS
smoke with `qemu64,+rdrand` passed streaming, file tools, cancellation, and
the next request.

## 2026-10-06: Static QEMU VM launch helpers

Added `scripts/run-vm.sh` and `scripts/run-vm.ps1`. They do not build or
package files. They set the QEMU user-network address `10.0.2.15/24`, gateway
`10.0.2.2`, and DNS `10.0.2.3:53`, then launch the existing Release launcher
with `artifacts/esp` and `workspace`.

## 2026-10-06: Network error stage context

Provider failures now identify the DNS server and API IPv4 endpoint, and label
DNS connect/send/read, API connection, request send, response header, error
response, and response stream stages. This preserves the underlying UEFI TCP4
error while distinguishing DNS TCP timeouts from API TCP or TLS/request
failures. `cargo fmt --all -- --check`, 45 core tests, and the x86_64 UEFI
target check pass.

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

Native packaging now boots EDK II UEFI Shell 2.2 from the shallow
`vendor/uefi-shell` submodule at 26H1. The x64 binary is a local
`EFI_AGENT_SHELL` build input and is not stored in Git. The submodule carries
the upstream source and BSD-2-Clause-Patent license.
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

Static IPv4 configuration was added for physical firmware that exposes IP4 and
TCP4 but has no IPv4 Config2 protocol. `CONFIG.JSON` accepts `ipv4.address`,
`ipv4.subnet_mask`, and `ipv4.gateway`; the launcher populates them from
`EFI_AGENT_IPV4_ADDRESS`, `EFI_AGENT_IPV4_NETMASK`, and
`EFI_AGENT_IPV4_GATEWAY`. TCP4 children use the configured station address and
add a default route. Omitting all three variables keeps the existing DHCP and
firmware-address behavior. Host core tests and UEFI target checks passed after
the change; physical static-address validation remains required.

Packaging scripts now discover the existing x64 Shell output under the pinned
EDK2 submodule, matching the selected DEBUG or RELEASE profile. An explicit
EFI_AGENT_SHELL still overrides discovery. They do not compile EDK2 themselves.
Windows Release build and packaging passed without EFI_AGENT_SHELL using test
provider settings. The generated artifact configuration contains those test
settings and needs a new package operation with real provider settings before
physical use. Bash syntax validation passed; Linux packaging was not run.

## iPXE SNP diagnostic build

Added `vendor/ipxe` as a Git submodule at
6262f1081fe185564e8ec8365a1d23597ec6e6f5. No commit was made. Exported
that revision into an isolated WSL `/tmp` workspace and built with GCC and
GNU Make using `make -j4 bin-x86_64-efi/snp.efi`. No Secure Boot target or
signing step was used. Copied the output to `artifacts/ipxe/snp.efi`.

PE inspection confirms x86-64, EFI application subsystem 10, and an empty
Security Directory. The linked ELF contains `snp_driver` and no
`snponly_driver`. SHA256:
`b45bc2a17b91e4c2de8f6c3ca5364f7aa3493f7a0d942ed963a4c73eba011e65`.
Make reported clock skew; the link and EFI conversion completed successfully.
Physical boot and network traffic have not yet been tested. This application
uses its own network stack over SNP and does not install TCP4 for Agent.

## CPU random generator

Replaced all Agent EFI RNG access with RDRAND and rand_chacha ChaCha20Rng.
Each image session collects one 32-byte seed on first nonempty request, checking
CPUID and the RDRAND success flag with ten attempts per 64-bit word. Failed
partial seeds are cleared and do not publish state. Successful initialization
is never repeated during that generator lifetime. The shared TLS/getrandom
entry is guarded against reentry. /caps probes the same generator without
displaying random bytes. Firmware certificate time remains required.

Core tests check bounded retries, partial-seed failure, recovery, the successful
tenth attempt, a ChaCha20 known vector, and more than 1 MiB of partitioned
output without another hardware seed. rand_core can discard unused bytes in
partial-word requests; the partition test uses word-aligned requests.
Windows UEFI Release build and UEFI Clippy passed. Host all-target Clippy is
blocked by the existing shell_assets tuple type_complexity warning in boot.rs.

Actual Windows QEMU/TCG with CPU max and no virtio RNG passed direct verified
HTTPS, DNS, streaming, tools, cancellation, and subsequent requests. Results
are in artifacts/rdrand-smoke. CPU max,rdrand=off rejected TLS with an explicit
Unsupported error and sent no provider credentials; results are in
artifacts/rdrand-negative-smoke. No claim about physical CPU entropy quality
is made by these tests. The VM launcher exposes CPU features and no longer
adds a virtio RNG device. Native firmware still needs physical retesting.

## WASM process dispatcher (2026-10-11)

The UEFI app embeds wasmi 2 through the no_std `efi-agent-wasm` crate. Nine
Windows host integration tests execute validated Wasm modules. They check
independent instance globals, yield/resume without repeated side effects,
bounded execution of an infinite loop, cancellation, child wait suspension,
exit status, inherited output, UTF-8 and empty arguments, trap isolation,
resource limits, and PID allocation. Boundary checks accept an iovec at byte
65,535 with length one and reject length two with no partial output. A memory
growth check accepts 1,024 pages and rejects the next page.

Windows UEFI Release build, UEFI Release Clippy, and WASM all-target Clippy
passed. Debug UEFI code generation failed in the existing sha2 0.10.9 dependency
with `rustc-LLVM ERROR: Do not know how to split the result of this operator!`.
The WASM crate's host Debug build and tests passed. No crypto configuration
was changed to bypass that compiler failure.
The 45 existing core tests also passed. The UEFI dependency feature tree shows
wasmi without its `std` or `wat` features; WAT parsing is a host fixture tool.

Actual Windows QEMU/TCG and OVMF passed `scripts/smoke_wasm.py` with the new
Release EFI image. The VM has no NIC and receives no provider configuration.
One infinite-loop instance remained runnable while a second instance spawned
a child, waited across explicit yields, received `WASM_CHILD_731` through its
inherited output, and exited with code 37. The smoke also cancelled and reaped
the loop, isolated a Wasm unreachable trap, cancelled a foreground loop with
Esc, ran new commands after both failures, and exited the firmware cleanly.
Results are in `artifacts/wasm-smoke` (serial bytes, final text and QEMU log).

Reproduce on Windows in a Windows-local checkout:

```powershell
cargo.exe test -p efi-agent-wasm
cargo.exe build -p efi-agent
cargo.exe build -p efi-agent-uefi --target x86_64-unknown-uefi --release
cargo.exe run -p efi-agent-wasm --example compile_fixtures -- scripts/fixtures/wasm artifacts/wasm-fixtures
./scripts/build_wasm_c_fixture.ps1 -WasiSdk $env:WASI_SDK_PATH
uv.exe run scripts/smoke_wasm.py --launcher target/debug/efi-agent.exe --efi target/x86_64-unknown-uefi/release/efi-agent-uefi.efi --code artifacts/firmware/OVMF_CODE_4M.fd --vars artifacts/firmware/OVMF_VARS_4M.fd --fixtures artifacts/wasm-fixtures --output artifacts/wasm-smoke --qemu qemu-system-x86_64.exe
```

The smoke uses unique process IDs and result markers for each operation;
previous screen text cannot satisfy a later result check. It uses observation
deadlines rather than fixed boot sleeps. No flakiness was observed in the
completed run, but repeated-run stability has not been established. This test
does not cover a physical machine, guest filesystem or network operations,
BusyBox, QuickJS, CPython, curl, or WolfSSH. Their implementations and
full-application verification remain required; see docs/wasm-world.md.

## WASM descriptor and pipe verification

Fifteen Windows WASM integration tests passed after adding descriptor tables,
bounded pipes, redirected spawn, fd close/renumber/stat, and blocking reads
and writes. The pipeline test observes both blocked read and blocked write
states. It checks 196,608 nonuniform bytes through producer, copier and
consumer instances. A separate host oracle compares the actual captured bytes
with independently generated data at read sizes 257, 997 and 8,191. Tests also
cover the last duplicated writer, last-reader cancellation with EPIPE,
orphan EOF after cancellation, zero-length I/O, wrong descriptor directions,
the 128-descriptor limit, atomic pipe allocation with one free slot, and
invalid redirect descriptors.

The updated Windows QEMU/TCG smoke passed with the Release UEFI app in
`artifacts/wasm-pipe-smoke`. It runs that three-process pipeline in actual
firmware, checks every received byte in the consumer, validates the success
marker and exit code, and proves that both children have been reaped. An
earlier test assertion inspected old UI text and failed after the pipeline
itself completed. The check now asks to reap each already-reaped child and
requires a unique UnknownProcess result. This removes that stale-text check;
repeated-run stability remains unverified. WASM all-target Clippy passed.

The Windows MSYS2 Clang failed to load its DLLs, so it is excluded from the
build procedure. The fixture script requires an explicit independent WASI
SDK path and does not choose Clang from PATH. The official WASI SDK 34 Windows
archive is being downloaded; the SDK script path still needs validation.

## C process capture verification

The actual C capture module passed both the host wasmi runner and Windows
QEMU/OVMF (`artifacts/wasm-capture-smoke`). It uses the same C process header
and library that the application ports will use. It checks argv packing on
both sides of a capacity boundary, creates redirected child input/output
pipes, transfers a payload with an embedded NUL, collects three-byte reads
until EOF, compares the actual bytes, waits for the child exit, and checks
reaping. The UEFI smoke requires the marker `C_SPAWN_CAPTURE_OK`, root exit
code 0, and a unique UnknownProcess result for the reaped child.

For this verification, the independent Windows Clang 20.1.8 frontend compiled
the freestanding sources to wasm32 LLVM bitcode with `-emit-llvm -c`.
Rust's bundled rust-lld then generated the command module with
`-flavor wasm --no-entry --export=_start --export-memory`. This uses no MSYS2
compiler or DLL. Native Clang in PATH lacks a Wasm object-code backend, so
direct `-c` code generation is not the procedure used for this run.
The SDK-specific build procedure above remains unverified until the SDK
download finishes. The WASM runner example can execute the generated module:

```powershell
cargo.exe run -p efi-agent-wasm --example run_guest -- artifacts/wasm-fixtures capture
```

Fifteen WASM tests, WASM all-target Clippy, and UEFI Release Clippy passed.
Actual QuickJS, Python, BusyBox, curl and WolfSSH application execution is
still pending. QuickJS 2026-06-04 sources have been downloaded under
artifacts/guest-sources for the next port work; downloading sources does not
prove that the engine is embedded or executable.

## WASM readiness polling

Seventeen Windows WASM tests passed after adding WASI poll_oneoff. A real
Wasm caller suspends in Polling state while a child holds an empty pipe open.
It resumes when data arrives, checks 64-bit userdata, event type, available
bytes and HUP, then reads the same bytes to prove the poll did not consume
them. A second call returns an invalid-descriptor error event and a valid
EOF event in subscription order. Immediate relative timeout tests return
only ready subscriptions. Invalid output memory and empty subscription lists
return errors before writing the event count. Timed and absolute waits
return NOTSUP until clock support is implemented.

UEFI Release build, WASM all-target Clippy, and the expanded Windows
QEMU/OVMF smoke passed. Results are in artifacts/wasm-poll-smoke. Actual
firmware checks the POLL_READY_OK marker, root exit 0, and child reaping,
alongside the existing process, backpressure and C binary capture checks.
The test fixture initially used an i64 as an if condition; Wasm validation
rejected it. The fixture now compares that i64 to zero and both host and
firmware tests passed. No runtime validation was bypassed.

The WASI SDK and sysroot transfer handles still run without completed output.
Their local archive sizes are not evidence of successful download or of a
usable toolchain. Revalidate the running handles before any retry. The named
application ports and the QuickJS/Python command interfaces remain incomplete.

## WASM clock and timed wait verification

Twenty Windows WASM integration tests passed after adding clock_time_get,
clock_res_get, a host clock provider, and relative/absolute poll deadlines.
Deterministic tests compare a relative deadline at 150 ns against 149 ns,
150 ns and a realtime rollback to 90 ns. A following absolute deadline of
200 ns completes at 200, not at now plus 200. They also check clock failure
while suspended, unchanged result memory on failure, deadline overflow,
invalid and unavailable clock IDs, and the last valid eight-byte output
range versus a one-byte overrun. The provider cannot be replaced while any
process exists.

UEFI Release build, WASM all-target Clippy and UEFI Release Clippy passed.
Actual Windows QEMU/OVMF passed the expanded smoke in
artifacts/wasm-clock-smoke. The Wasm clock fixture reads firmware realtime
and positive reported resolution, waits for a relative one-second timeout,
checks that realtime advanced by at least one second, and requires the
FIRMWARE_CLOCK_WAIT_OK marker and root exit 0. The prior process, pipe,
poll and C capture cases also passed.

UEFI realtime comes from GetTime, including its nanosecond field and the same
UTC calendar conversion used for TLS. GetTime capabilities supply the clock
resolution. UEFI monotonic and process/thread CPU clocks return NOTSUP;
this verification does not claim those clock types or physical RTC behavior.
The host runner uses SystemTime and Instant and a bounded idle sleep for
fully sleeping worlds. Application ports remain incomplete.

## Concurrent command capture and real QuickJS

WASI SDK 34, its sysroot and compiler runtime finished downloading and
extracting. The SDK's Windows-native clang.exe and wasm-ld.exe successfully
built scripts/build_wasm_c_fixture.ps1. No MSYS2 compiler was invoked.
scripts/build_quickjs.ps1 built the real QuickJS 2026-06-04 engine with WASI
libc and the restricted command wrapper. The copied source patches disable
threaded Atomics, include malloc.h and remove dtoa's unused setjmp.h include.
quickjs-libc.c is not linked. The upstream license accompanies the module.

Twenty-three Windows WASM integration tests, WASM all-target Clippy, UEFI
Release build and UEFI Release Clippy passed. New checks cover live versus
exited nonblocking wait, untouched status memory while live, invalid status
memory, reaping once, stream seek errors and the last valid result range.
A large callee with 20,003 global increments verifies that fuel exhaustion
preserves its continuation while another process completes. Real QuickJS
initially exposed a lazy-translation fuel error; eager module compilation
resolved it without increasing the runtime quantum or replaying guest code.

The shared C capture fixture runs two real children concurrently. Each child
writes 70,003 binary stderr bytes before 65,537 stdout bytes. The parent
compares every byte, exit code 23 and both lengths, then repeats with stdout
capacity reduced by one byte. Overflow is reported while both streams still
drain and the child is reaped. A missing program checks descriptor cleanup.
Host wasmi and actual UEFI QEMU/OVMF passed. Firmware artifacts are under
artifacts/wasm-shell-capture-smoke and artifacts/wasm-quickjs-smoke.

The real QuickJS command ABI smoke passed through scripts/smoke_quickjs.py.
It verifies Promise.all with two parallel commands, exact generated shell
bytes for single quotes, empty words, Unicode and shell metacharacters,
stderr-before-stdout capture, nonzero-exit rejection, NUL rejection and
absence of std/os/process/require globals. Its child is an explicit ABI probe
registered temporarily as ash. The probe is not a shell interpreter, so this
does not prove BusyBox ash or actual shell syntax execution.

Actual Windows QEMU/OVMF passed scripts/smoke_wasm.py with --quickjs
artifacts/wasm-guests/quickjs.wasm. Firmware loaded and ran the real engine,
resolved top-level await, checked restricted globals and firmware Date.now,
printed QUICKJS_ENGINE_OK and reported Process 20: Exited(0). The expanded
C capture case printed MULTI_CHILD_SHELL_CAPTURE_OK and Process 16: Exited(0);
PIDs 17, 18 and 19 were confirmed reaped through UnknownProcess results.
The earlier dispatcher, pipe, cancellation, timed poll and clock cases passed.
No flaky result was observed in these runs. Wall-clock QEMU smoke deadlines
still depend on host load. Physical firmware remains unverified.

BusyBox ash NOFORK, CPython, curl, WolfSSH, guest filesystem/socket support,
and the Python command interface remain incomplete. The QuickJS command API
still requires a real registered ash port for actual shell command execution.

## Guest nonlocal jumps for ash

BusyBox 1.37.0 sources were downloaded from busybox.net and extracted under
artifacts/guest-sources. Inspecting the actual ash source confirmed that
FEATURE_SH_NOFORK does not remove its nested setjmp/longjmp error recovery,
or its fork paths for pipelines, external commands and subshells. No BusyBox
ash application has been built or claimed executable yet.

Binaryen 133's Windows archive completed download and extraction. The new
guests/sjlj runtime saves Wasm frames with Asyncify, restores the guest C stack
pointer, and handles capture/jump cycles below the instrumented application.
It does not require host stack manipulation or Wasm exception handling.
scripts/build_wasm_sjlj.ps1 uses the verified Windows WASI SDK and Binaryen,
with no MSYS2 compiler invocation. All transforms and runtime calls stay
inside each Wasm module.

The actual C fixture passed at O0, O2 and O3 in host wasmi. It checks stack
allocated environments, nested jumps, zero-to-one and negative return values,
re-capture at a repeated location, five jumps to the same saved environment,
volatile stack objects changed after capture, and entry side effects executed
once. Deep recursive jumps call sched_yield first. Two child instances execute
concurrently with the root and are waited for and reaped.

scripts/smoke_sjlj.py computes a separate arithmetic oracle for 37 structured
jumps per seed, with varying depths and signed values. Each optimization
level passed root/child seeds 11/17/29 and standalone seeds 0, 1, 2, 999,
65535, 123456789 and 4294967295. The captured digest and marker counts matched
the host oracle. This explores jump paths, not random invalid-module input.

Actual Windows QEMU/OVMF passed the expanded smoke with --sjlj
artifacts/sjlj-o2/sjlj.wasm. Artifacts are in artifacts/wasm-sjlj-smoke.
The firmware root PID 21 printed SJLJ_MULTI_INSTANCE_OK and exited 0.
Child PIDs 22 and 23 were confirmed reaped with UnknownProcess errors.
The real QuickJS engine and all earlier process, pipe, capture and clock
checks also passed. No flaky result was observed. The Asyncify jump runtime
still needs maximum-depth/resource checks and integration with actual ash
error recovery. This result does not prove BusyBox, CPython or shell execution.

## Process startup environments

The world now stores an independent startup environment per instance.
World::spawn_with_environment supplies explicit root entries; firmware roots
remain empty. Ordinary spawn copies its parent's startup entries, while the
new spawn_env_fds import replaces or clears them. WASI environ_sizes_get and
environ_get expose actual entries. The guest C helper packs both argv and
environment into caller scratch. No host environment variables are copied.
Guest libc or shell mutations require explicit environment passing.

Twenty-eight distinct WASM integration tests passed, with WASM all-target
Clippy, UEFI Release build and UEFI Release Clippy. Five environment tests
check host validation, exactly 65,536 joint argument/environment bytes versus
one byte beyond, unchanged PID allocation after rejection, structured lists
with empty/Unicode/newline/equals-containing values, invalid output memory,
raw spawn invalid UTF-8, duplicate names, missing final NUL, bad descriptors,
explicit-size overflow and inherited-size overflow. Serialized output is
compared with an independently constructed host byte list.

The real WASI-libc C fixture verifies getenv in a child and grandchild,
explicit clearing from a nonempty child, parent isolation, invalid names and
duplicate entries, and a 46-byte scratch capacity against a 45-byte capacity.
It waits for all children and validates their exit statuses. Host wasmi and
actual Windows QEMU/OVMF passed. Firmware artifacts are under
artifacts/wasm-environment-smoke. The root PID 20 printed
PROCESS_ENVIRONMENT_OK and exited 0. Children 21 through 24 were confirmed
reaped through UnknownProcess errors. QuickJS PID 25 and jump-runtime PID 26
also exited 0; jump children 27 and 28 were reaped. Earlier process, capture,
pipe, timed polling and clock checks passed. QuickJS's separate process ABI
smoke passed with quoted templates, parallel capture and failure rejection.

No flaky result was observed. These checks verify environment transport,
not ash assignment/export parsing. Actual BusyBox ash and the remaining
application ports are still incomplete.

## Upstream ash object and terminal program bridge

scripts/prepare_busybox.py and scripts/check_busybox.ps1 now prepare the real
BusyBox 1.37.0 ash source and compile it to artifacts/wasm-guests/ash-build/ash.o
using Windows WASI SDK 34. The generated source and upstream license are
retained beside the object. The initial configuration enables aliases,
getopts, command, internal glob and FEATURE_SH_NOFORK. Full applet/builtin
and arithmetic configurations remain pending.

Direct fork is replaced in the generated translation unit with the declared
efi_ash_spawn_node boundary. This boundary still needs its serializer and
fresh-instance entry implementation. No fork implementation or successful
no-op was supplied. Utility, signal, CPU-clock and filesystem declarations
do not prove those interfaces are implemented. The object check alone does
not verify ash execution. C noreturn promises on ash error helpers
are replaced with noinline boundaries so Asyncify can unwind through them.
Explicit unwind return values resolved all return-path compiler diagnostics.

The terminal program bridge in guests/busybox/exec.c is implemented and
tested separately. It packs the selected argv and explicit environment,
starts a registered fresh instance, waits/reaps it and exits with its status.
POSIX process errno values are mapped to the different WASI-libc errno
numbers. Missing programs and duplicate environment entries return ENOENT
and EINVAL before successful child execution. Program selection currently
uses argv[0]; filesystem PATH search still needs implementation.

The real C fixture and scripts/smoke_ash_exec.py passed in host wasmi. The
child checked its nonzero parent PID and its empty/equals-containing
environment values, printed ASH_EXEC_FRESH_INSTANCE_OK and exited 37.
The root propagated 37. The host smoke checks both that exact status and
the standalone output line; a generic failure or trap does not satisfy it.

Actual Windows QEMU/OVMF passed the expanded smoke with --ash-exec
artifacts/wasm-fixtures/ash_exec.wasm. Artifacts are under
artifacts/wasm-ash-exec-smoke. Root PID 29 exited 37 with the expected marker;
child PID 30 was confirmed reaped with UnknownProcess. QuickJS, process
environments, nonlocal jumps and earlier dispatcher/capture/clock cases
also passed. No flaky result was observed. Full ash interpretation,
pipelines, command substitution, subshell state isolation and syntax-error
recovery are not verified yet. CPython, curl and WolfSSH remain incomplete.

## Windows Clang dependency failure and ash link check

MSYS2 mingw64 clang.exe produced Windows entry-point error dialogs, including
__emutls_v._ZSt11__once_call in C:\msys64\mingw64\bin\libclang-cpp.dll.
Static PE inspection confirmed that libclang-cpp.dll imports
__emutls_v._ZSt11__once_call from libstdc++-6.dll. Neither the installed MSYS2
runtime nor QEMU's runtime exports that symbol. The installed package records
show Clang 21.1.8-4 and GCC runtime 16.2.0-3. This confirms an incompatible
Clang/runtime pair; the DLL selected by the failing process was not traced.
QEMU's directory also precedes MSYS2 on PATH and contains same-name GCC DLLs.
Do not invoke that compiler for this project. Guest build scripts select
bin/clang.exe under the explicit
WASI SDK directory instead of resolving clang from PATH.

The Windows WASI SDK compiler started successfully during the follow-up check.
build_wasm_c_fixture.ps1 compiled all C fixtures with the explicit SDK path.
No global PATH changes or MSYS2 package changes were made. The standalone
LLVM installation did not contain llvm-readobj.exe; WSL objdump inspected the
PE files without executing the affected compiler.

Follow-up inspection on 2026-10-11 confirmed that the MSYS2 package versions
remain Clang/clang-libs 21.1.8-4 and gcc-libs 16.2.0-3. The explicit Windows
WASI SDK 34 compiler starts successfully. The current CPython rebuild also
completed compilation and linking without invoking MSYS2 Clang. Its eight
host integration tests still fail during interpreter initialization because
the restricted builtin table removes `posix`, which external importer setup
requires. This separate runtime issue was resolved in the CPython integration
checks recorded below.

Windows WASI SDK 34 clang --version passed. build_busybox.ps1 compiled, linked
and Asyncify-transformed the real ash source into artifacts/wasm-guests/ash.wasm.
The first host run_guest attempt rejected instantiation on the missing
wasi_snapshot_preview1.fd_fdstat_set_flags import. This runtime failure was
separate from the MSYS2 DLL loading failure and is now resolved below.

## Real ash and QuickJS shell execution

The world now provides stream fd_filestat_get and fd_fdstat_set_flags. It
accepts zero flags, rejects unsupported defined flags with NOTSUP and rejects
unknown flag bits with INVAL. Streams advertise FD_FILESTAT_GET rights. File
metadata uses the WASI 64-byte layout; character streams have CHARACTER_DEVICE
type, one link and zero size/timestamps. No filesystem preopens exist yet.
Prestat returns BADF; directory operations return NOTDIR for valid streams and
BADF for absent descriptors. These are interim domain errors, not filesystem
support. The metadata integration test checks exact memory-end boundaries,
invalid descriptors, flag cases and the independent WASI field offsets.

Windows cargo test -p efi-agent-wasm passed 29 integration tests. Clippy with
--all-targets -- -D warnings passed. The UEFI Release build passed.
scripts/smoke_ash.py passed real BusyBox ash echo, quoted expansion, functions,
function return status, for loops, conditions, explicit exit 37 and syntax
error exit 2. With --quickjs, it also passed concurrent actual shell execution,
single quotes, empty arguments, Unicode, literal shell-special substitutions,
capture and rejection of exit 17. No flaky result was observed.

Actual Windows QEMU/OVMF passed scripts/smoke_wasm.py with --ash, --quickjs,
--sjlj and --ash-exec. Artifacts are under artifacts/wasm-real-ash-smoke. Ash
root PID 31 printed ASH_REAL_ENGINE_OK and exited 0. QuickJS PID 32 launched
two actual ash instances and printed QUICKJS_REAL_ASH_OK. The existing process,
pipe, clock, environment, QuickJS, nonlocal jump and exec-bridge checks passed
in the same run. Full shell node serialization, pipelines, command substitution,
external command evaluation, filesystem support and the other application
ports remain incomplete.

## Real ash external program dispatch

The live efi_process.program_exists import queries exact registered names and
yields without allocating a PID. Its integration test checks an absent name,
registration after the caller starts, a memory-end valid name, a one-byte
overrun, empty names, size limits, embedded NUL and invalid UTF-8. Windows
cargo test -p efi-agent-wasm passed 30 integration tests. Clippy with
--all-targets -- -D warnings and the UEFI Release build passed.

prepare_busybox.py now routes ordinary external commands through efi_ash_run
instead of forkshell. Builtins and functions retain lookup precedence; command
bypasses functions. The parent waits/reaps and continues, while exec exits
after its fresh child finishes. Registered command lookup is independent of
filesystem PATH; filesystem PATH support remains pending.

scripts/smoke_ash_external.py passed actual ash dispatch of ash_child.wasm:
empty/quoted/Unicode argv, stdout/stderr, exported and temporary variables,
export override, repeated assignments using the last value, parent isolation,
nonzero status followed by continued execution, exec exit 23, set -e exit 17,
missing command status 127, function precedence, command bypass, and status 126
when expanded environment bytes exceed 64 KiB. Direct i32 child exits -1 and
300 become shell statuses 255 and 44. QuickJS also passed actual ash external
command capture and rejection. The separate ash exec bridge smoke still passed.

The repeated-assignment check exposed duplicate environment names and an
unsigned ash exitstatus conversion that changed spawn failure -1 into 255.
Environment lists now compare variable names and discard earlier assignments.
Spawn errors remain signed until converted to shell statuses. The world still
rejects duplicate raw environment entries; its validation was not relaxed.

Actual Windows QEMU/OVMF passed the expanded smoke with --ash-child, --ash,
--quickjs, --sjlj and --ash-exec using the final rebuilt ash and C fixtures.
Artifacts are under artifacts/wasm-ash-external-smoke. Ash PID 35 reported child
status 17 and continued. QuickJS PID 37 rejected the same external command with
exitCode 17. Children 36, 38 and 39 were confirmed reaped. Existing firmware
checks passed in the same run. No flaky result was observed. Shell-node
serialization, pipelines, command substitution, filesystem PATH and the other
application ports remain incomplete.

## Real ash shell tasks, pipelines and command substitution

guests/busybox/tasks.inc now encodes typed AST fields and shell state, then
starts a fresh ash instance through the process ABI. It transfers no pointers
or linear-memory image. The state carries variables with export/readonly/unset
flags, functions, aliases, options, positionals and $0, getopts cursor, $$,
previous status and $!. The binary limit is 28,000 bytes, hex-encoded into
argv; traversal depth is limited to 256. Task errors use ash error recovery.
The child starts in a new Store with selected standard descriptors.

Pipeline creation starts all stages before waiting and closes the parent's
unused pipe endpoints. Job waiting polls known children cooperatively and
stores packed exit statuses. Command substitution drains its pipe before
waiting; upstream ash removes trailing newlines and expands the result. The
child clears errexit for substitution, matching this upstream configuration.
Tested subshells and pipelines preserve EV_TESTED. Background tasks support
$! and wait without asynchronous signal delivery.

Windows scripts/smoke_ash_tasks.py passed 20 named shell cases covering state
isolation, inherited functions and parameters, control trees, builtin/external
pipes, pipeline last status, background wait, nested pipelines, substitution,
nested substitution, substitution status and trailing newlines, substitution
pipelines, alias/getopts/$$ inheritance, exported unset variables and tested
errexit. A producer sends
196,609 deterministic bytes through three actual ash/external stages with
4093-byte writes and 257-byte reads; the consumer checks every byte and EOF.
QuickJS captured the successful actual bounded pipeline.

Five seeded structured shell states include Unicode, empty/quoted content,
newlines and literal shell operators. Their pipeline output matched the
independent Python string oracle. Readonly and nounset failures stayed inside
the child, preserving parent state. Subshell redirection expansion executed
its external command substitution once, then returned the expected file error;
the child used the parent's expanded result without replay. The transfer check accepted a 20,000-byte
variable and rejected a 30,000-byte variable with exit 2; a following parent
command prevents the terminal subshell optimization from bypassing the test.
The depth check accepted NCMD plus 255 argument nodes (depth 256) and rejected
one extra argument node with exit 2. Odd/invalid hex, truncated words and a bad
state version returned domain errors instead of interpreter traps.

The depth check exposed Asyncify's interpreter value-stack overhead. Raising
call depth alone did not fix it. The world now bounds call depth at 4096 and
value-stack storage at 8 MiB, growing as needed. The exact guest depth boundary
then passed. An independent WAT test completed 1200 nested calls and verified
that unlimited recursion traps locally without stopping another instance.
Windows cargo test -p efi-agent-wasm passed 31 integration tests; Clippy with
--all-targets -- -D warnings and the UEFI Release build passed. Existing ash
external-command and QuickJS capture/rejection host checks also passed.

Actual Windows QEMU/OVMF passed the final Release firmware and guest modules
with --ash-tasks, --ash-child, --ash, --quickjs, --sjlj and --ash-exec. Artifacts
are under artifacts/wasm-ash-tasks-smoke. Ash root PID 40 verified the 196,609-byte
pipeline; PID 47 returned child:parent from command substitution. QuickJS PID 49
captured a command substitution containing the same bounded pipeline and printed
UEFI_ASH_TASKS_OK. Children 41-46, 48 and 50-57 were confirmed reaped. Existing
firmware checks passed in the same run.

Full task failure cleanup, inherited nonstandard descriptors, background stdin
isolation, large heredocs, remaining builtins, filesystem/PATH and other
application ports remain incomplete. No flaky result was observed in the
completed host checks.

## Actual directory traversal and ash glob

fd_readdir now uses real host/EFI directory entries. Each traversal retains a
sorted list bounded to 4096 entries and 512 KiB of encoded WASI directory
records. Cookie zero refreshes the list; subsequent cookies retain its contents
even if another guest creates a file. Duplicate descriptors share the list.
Directory opens have independent lists. Directory NONBLOCK is accepted and
reported in fdstat. The regular file flag behavior remains unchanged.

The actual C directory fixture passed on the Windows host and in QEMU/OVMF.
It checks buffers of 0, 23, 24 and 25 bytes around the 24-byte header and first
one-byte name. A changing buffer size forces retries at partial headers/names
while traversing 82 initial entries. It checks unused buffer tails, invalid
cookies, invalid output memory, BADF/NOTDIR/NOTCAPABLE, duplicate lifetime,
creation during traversal and refresh on cookie zero. Actual libc opendir,
readdir, telldir, seekdir, rewinddir, closedir and empty-directory EOF pass.
Two new Rust integration tests independently check the entry and byte limits,
including a rejected large entry followed by an exactly fitting entry.

The fixture saves initial, refreshed and libc name/type records as hex-encoded
UTF-8 names. Independent Python checks compare complete sets with the original
host inputs and reject duplicates. Seeded inputs include 67 variable-length
names, spaces, punctuation, hidden files, precomposed/decomposed Unicode,
directories and long names. The refreshed listings include added.txt; the old
listing excludes it. Actual ash glob results are independently derived from
the staged names and verify sort order and hidden-name exclusion. QuickJS
captures the actual ash glob and compares every byte. No fake guest filesystem
or mocked libc is used.

33 Rust integration tests, all-target wasm Clippy, host file/directory/ash/
QuickJS checks and the UEFI Release build passed. Windows QEMU/OVMF passed
scripts/smoke_wasm.py with --filesystem --directories, real ash and QuickJS.
Artifacts are under artifacts/wasm-directory-smoke. The test reads the actual
FAT image after shutdown and compares all directory records and glob-list.txt.
Directory ash PID 36 returned UEFI_ASH_DIRECTORY_OK, and QuickJS PID 37 captured
the glob through ash PID 38. These checks also retain the prior actual file
cursor, append, rights, path validation and saved-byte checks.
The complete optional firmware regression also passed under
artifacts/wasm-directory-full-smoke, including nonlocal jumps, external dispatch,
exec, pipelines, command substitution, read and printf before the directory cases.

Intermediate issues resolved: the firmware module initially lacked the no_std
ToString trait import. libc opendir requests O_NONBLOCK; the first version
rejected it with NOTSUP. Directory enumeration now accepts that flag and
reports it accurately. libc dirent types use DT_DIR/DT_REG values, which differ
from WASI types; the record verifier explicitly converts them.

One firmware smoke failed before the directory case when its unframed pyte
display contained misplaced characters around an actual successful QuickJS
exit. The smoke now consumes complete frames using guest view metadata and
sets dimensions from each committed frame, as the host launcher does. The
subsequent actual run passed. Offline replay of 239 recorded frames also
recognized the successful QuickJS exit. A temporary replay command first had
Bash/PowerShell quoting errors, causing Bash to interpret Python lines and
start ImageMagick import commands. Those processes were terminated; the replay
then passed with a literal Bash heredoc containing the complete PowerShell
script. The temporary Wasm disassembly probe was removed. Python bytecode cache
directories are now ignored by Git. No MSYS2 compiler or network download was
needed. The supplied project state omits existing WASM progress; local Current
state retains the verified implementation. The display-parser failure is a
flaky-test issue observed in this turn; its framing fix passed actual rerun.

Directory mutation, rename/delete, file resize imports, timestamps, symlinks,
filesystem PATH, CPython, curl and WolfSSH remain incomplete.

## Actual WASI filesystem and ash redirections

The no_std world now accepts a filesystem provider before spawning. UEFI opens
real EFI_FILE_PROTOCOL handles on the configured boot-volume workspace, or
\work without model configuration. Each process receives a directory preopen
at fd 3, named dot. The host runner uses native handles in its program directory.
Open supports read/write, create/exclusive/truncate/append and directory handles.
Metadata, seek/tell, sync and file readiness are implemented. Copies of a
descriptor share a cursor; independent opens do not. Path resolution checks
each intermediate directory, rather than removing dot-dot components blindly.

Windows scripts/smoke_wasm_filesystem.py passed the real WASI libc fixture,
actual ash write/append/read redirections, a fresh ash command-substitution
task using the same filesystem, and QuickJS capture from an actual file.
The fixture checks binary bytes, readonly creation, exclusive-create errors,
shared and separate cursors, append after seek, metadata, Unicode openat,
explicit and implicit directory opens, missing and non-directory parents,
traversal outside a directory, rights failures, descriptor allocation limits,
and output-pointer validation before truncation. A structured 97-operation
cursor workload uses a 257-byte file. Guest reads and an independent
Python byte-array oracle agree. Poll reports zero bytes at EOF and 256 bytes
after seeking to offset 1.

The Release UEFI build passed. Actual Windows QEMU/OVMF passed the complete
optional guest smoke with --filesystem, --ash-printf, --ash-read, --ash-tasks,
ash external dispatch, the exec fixture, nonlocal jumps and real QuickJS.
Artifacts are under artifacts/wasm-filesystem-smoke. The filesystem fixture
finished at PID 71; ash PID 72 and its task PID 73 returned success. QuickJS
PID 74 captured a file through ash PID 75. After the final readonly-create
and directory-error changes, a second firmware run also passed under
artifacts/wasm-filesystem-final-smoke. Both runs read the FAT image after clean
shutdown and verified exact binary file bytes plus first/second shell output.
The final run uses the latest fixture, including file-readiness boundary checks.
All 31 Rust integration tests and wasm all-target Clippy passed. The host read
smoke also passed after the redirection changes.

Intermediate issues resolved: the initial fixture used an incorrect dup2 name
and then expected zero instead of the target descriptor. Both were corrected
against the actual process ABI. WASI libc passes inheriting rights when opening
regular files and requests rights for multiple file types. The implementation
now verifies them against the parent and retains the applicable rights on the
opened descriptor. Ash used unsupported libc F_DUPFD operations when saving
redirections; those five upstream call sites now use the world's dupfd import.
An early permissions check still returned stream BADF for a restricted file;
it now reports NOTCAPABLE before touching output memory. The native provider
also handles read-only creation without granting guest write rights.

The first firmware display check matched command input before output arrived.
The next check assumed streamed output would occupy a single transcript panel.
The checks now require actual output lines and allow separate panels. Neither
failure indicated a guest file-operation failure. No flaky result was observed
after these fixes. The final checks also inspect the persisted FAT files.
Local dependency probes first used nonmatching paths; the installed UEFI crate
was found in Cargo's configured mirror directory. A direct upstream source
request failed with a connection reset. The Windows proxy at port 2080 worked,
but the requested source path returned HTTP 404. Local Wasm disassembly supplied
the libc call evidence, and its temporary probe file was removed. MSYS2 Clang
was not invoked. The supplied project-state text still omits verified WASM
progress; the local Current state and this record retain that progress.
A WSL Git status probe on the Windows tree was slow and was cancelled.
Windows-native Git completed the final status and whitespace checks.

Directory enumeration/mutation, rename/delete, file resize imports, timestamps,
symlinks, filesystem PATH, complete working-directory state, socket operations
and the remaining named application ports are not verified or complete.

## Actual ash printf builtin

Windows WASI SDK 34 compiled and Binaryen 133 transformed the actual upstream
printf builtin and bb_strtonum helpers. scripts/smoke_ash_printf.py passed
14 named cases, a seeded independent signed/hexadecimal integer oracle and
real QuickJS NUL capture. Cases cover repeated formats, missing arguments,
dynamic width/precision, signed and unsigned 64-bit limits, radix parsing,
floats, escapes, stop escape, malformed numbers, overflow, format/usage errors,
real pipelines and read over NUL-containing output. The read smoke passed
again after replacing the local numeric helper with upstream bb_strtonum.

Actual Windows QEMU/OVMF passed scripts/smoke_wasm.py with --ash-printf,
--ash-read and all earlier optional guest checks. Artifacts are under
artifacts/wasm-ash-printf-smoke. Root PID 68 printed the signed minimum and
negative hexadecimal conversion. QuickJS PID 69 captured a three-byte a/NUL/b
result and printed UEFI_QUICKJS_PRINTF_BINARY_OK. Its ash child PID 70 was
confirmed reaped. No flaky result was observed; the integer oracle is seeded
and byte output comparisons do not depend on terminal rendering.

Intermediate issues resolved: the feature scanner initially omitted printf.c,
leaving ENABLE_HUSH_PRINTF undefined. It now scans that translation unit.
A smoke run before the successful build tested the retained old ash artifact
and returned command-not-found. The new artifact passed. The initial missing
integer argument test expected success; upstream reports invalid empty number
and returns 1, so the test now checks that behavior. MSYS2 Clang remains
excluded because of the previously recorded DLL entry-point errors; its
installation was not repaired. The supplied project-state text omits the
existing WASM work; the local Current state retains verified progress.

## Actual ash read builtin

prepare_busybox.py extracts upstream BusyBox shell_builtin_read from
shell/shell_common.c with its license header. The build links it with real ash.
Platform changes remove termios, explicitly reject silent terminal input,
check the monotonic clock before positive timeouts, and reject invalid
descriptors in the zero-timeout readiness path. Ash now handles -d even with
Bash compatibility disabled; it was previously parsed but silently ignored.
The existing WASI read and poll_oneoff imports provide cooperative suspension.

Windows scripts/smoke_ash_read.py passed 17 named input checks: whitespace and
nonwhitespace IFS, last-field remainder, empty/missing fields, raw versus escaped
backslashes, continuation, empty IFS, partial and empty EOF, -n read boundary,
-d delimiter, multiline loops, timed input and invalid descriptors. Ten seeded
raw input cases matched an independent Python field oracle. A writer kept its
pipe open for two seconds: read -t 1 returned failure without changing its
variable, and a following blocking read collected the later input. Invalid
count/name and unsupported echo control returned errors. Actual QuickJS
captured a real ash read pipeline. No flaky result was observed in this run;
the delayed timeout case requires enough host scheduling time within its
120-second overall limit.

Actual Windows QEMU/OVMF passed scripts/smoke_wasm.py with --ash-read and the
existing --ash-tasks/--ash-child/--ash/--quickjs/--sjlj/--ash-exec checks. Artifacts
are under artifacts/wasm-ash-read-smoke. Root PID 58 returned UEFI_READ_INPUT.
PID 61 exited 2 with the expected unavailable monotonic timeout clock error.
QuickJS PID 64 printed UEFI_QUICKJS_READ_OK after actual read/IFS capture.
Children 59-60, 62-63 and 65-67 were confirmed reaped. All earlier firmware
checks passed in the same run. This turn changed guest build preparation and
smoke scripts; no runtime Rust or firmware binary change was required.

Intermediate platform limitations handled explicitly: WASI has no termios
interface for this guest; firmware has no monotonic clock provider yet. Both
return domain errors instead of fabricated success. No Windows/WSL tool,
dependency, shell-quoting or network failure occurred in this read-port work.
The supplied Current state text omits ongoing WASM changes; the local state
and this document retain the verified implementation and outstanding scope.

## Actual guest file resizing

The WASI fd_filestat_set_size import requires resize rights and a writable
descriptor. Native and EFI providers preserve the shared file cursor, zero-fill
growth and reject sizes above 48 MiB with FBIG before mutation. EFI uses SetInfo
for shrink and explicit zero writes for growth. Append handles resize without
moving their cursor; the next write selects the new EOF. A physical write
failure can leave partial growth. The zero-fill loop is synchronous.

Actual WASI libc checks cover shrink with the cursor past EOF, duplicate handle
growth, zero extension across chunk boundaries, negative and oversized lengths,
restricted rights, invalid descriptors, directory/stream errors, zero shrink,
regrowth and append after resize. Seventy-one structured resize/overwrite
operations compare every intermediate byte in the guest. The host and firmware
integration drivers compare final file contents with an independent bytearray
oracle, including bytes read from the FAT image after shutdown. These file
checks extend the smoke drivers into integration tests; markers alone are not
accepted as proof of persistence. The cases are deterministic, use isolated
workspaces and do not depend on timing to establish the resize result.

Windows host file/directory, actual ash task/redirection and QuickJS capture
checks passed. All 33 Rust integration tests passed, and the UEFI Release build
passed. Actual QEMU/OVMF file and directory checks passed with artifacts under
artifacts/wasm-resize-smoke. No CPython, curl or WolfSSH support is established
by these checks.

Clippy passed with warnings denied. The expanded QEMU/OVMF integration run
also passed with nonlocal jumps, ash exec/external dispatch, pipeline/task
isolation, read/printf builtins and QuickJS capture enabled. Its artifacts are
under artifacts/wasm-resize-full-smoke. No flaky result was observed in these
host and firmware runs.

Intermediate issues resolved: the C compiler rejected a signed/unsigned read
length comparison; the fixture now uses an explicit ssize_t conversion.
Windows append-only native handles denied resizing; the native provider now
opens a writable handle and seeks EOF for each append write. EFI FAT returned
an error for a read beyond the truncated EOF; the firmware provider now checks
the cursor against current metadata and returns zero without changing it.
The combined cursor/read assertion was split to identify either failure.
The supplied Current state still omits the WASM work; the local document keeps
its verified state. No MSYS2 compiler was invoked during these resize checks.

## Actual guest cryptographic random bytes

The world now supplies WASI random_get through an explicit host provider.
Processes share that provider without restarting its state. Memory validation
precedes generation. Valid empty requests succeed without a source. Nonempty
requests return NOTSUP when no source is available or IO on provider failure.
The 512 KiB per-call cap bounds synchronous allocation and generation; larger
requests return INVAL. Staging prevents publication of partially generated
bytes after a provider error.

UEFI uses the existing RDRAND-seeded ChaCha20 application generator, also used
by TLS. It does not reseed per call or process. Missing RDRAND and seed failure
return NOTSUP; there is no weak fallback. The native runner uses the OS source
through getrandom. QuickJS receives no additional JavaScript host interface.

All 36 Rust WASM integration tests passed. Three new tests use actual wasmi
modules to check valid/invalid memory-end boundaries, pointer overflow, empty
ranges, the exact request cap and cap plus one, absent/failed providers and
three independent instances. An error-injecting provider fills its temporary
buffer before failing; guest sentinels confirm that none of those bytes are
published. Expected output from a shared deterministic sequence is computed
independently of the dispatcher order. These tests check ABI behavior, not
random quality. The core generator's three tests passed, including the
ChaCha20 zero-key known vector, bounded seed failure and no reseeding.

Clippy with warnings denied and the UEFI Release build passed. Actual WASI
libc getentropy checks passed on the Windows native runner. The fixture checks
the libc 256-byte limit, invalid raw pointers, guard bytes and repeated calls,
then saves three 256-byte records. The host integration driver verifies actual
saved data. Actual QEMU/OVMF passed with --random --filesystem --directories,
real ash and QuickJS. The firmware integration driver verifies all three
distinct records directly in FAT after shutdown. Artifacts are under
artifacts/wasm-random-smoke. Record inequality detects accidental repeated
output; it is not a statistical or cryptographic quality test.

A separate actual QEMU run with --random --no-rdrand disabled the CPU feature.
Raw random_get and libc getentropy returned the expected unsupported errors;
all target and guard bytes retained their sentinels. Artifacts are under
artifacts/wasm-random-unavailable-smoke. Existing process, pipe, readiness,
environment, file, directory, ash and QuickJS checks passed in their respective
runs. No flaky result was observed.

Intermediate issues resolved: one fixture build patch omitted the existing
process-library argument in its context and did not apply; it was corrected
against the actual file. A WSL git diff probe stalled on the Windows worktree;
that probe was stopped and the final check used git.exe in a Windows-local
working directory. A documentation patch used mismatched context and was
corrected before applying. Searches for nonexistent network/entropy source
paths were replaced by the actual random.rs modules. No compiler, dependency
download or network failure occurred. The supplied project-state text omits
the WASM work; the local Current state retains verified progress. CPython,
curl, WolfSSH and guest sockets remain unfinished.

## Actual CPython 3.14 and restricted Shell interface

Windows build: `scripts/build_cpython.ps1` with Windows WASI SDK 34 and the
stable CPython 3.14.0 build interpreter. It produces the real static CPython
engine, frozen UTF-8 codec modules, `efi`, native `_efi_shell`, and the upstream
license. The resulting `python.wasm` is about 6.4 MiB. All compiler paths are
explicit SDK paths. Native MSYS2 is used only for configure and make.

The restricted configuration uses CPython's pthread stubs. Dynamic loading,
readlink and realpath capability probes are disabled. The import core installs
only built-in and frozen importers. The FileIO source permits integer fd 0–2
without a custom opener. Host modules and builtins listed in docs/wasm-world.md
are excluded. These interface restrictions do not establish a security sandbox.

Ten native integration cases in scripts/test_cpython.py were verified against
the actual interpreter and real ash. They check the final release version,
arithmetic, math, Unicode, exact binary output, separate stderr, asymmetric
argument quoting, checked and unchecked exit statuses, repeated fresh Shell
state, pipelines, NUL and UTF-8 command bounds, uncaught Python errors, and
disabled file/module interfaces. The restriction check adds `.` to sys.path
and confirms that the existing external.py still cannot be imported. Alternate
_io open functions and custom openers are also rejected. Output at exactly
512 KiB is captured intact; one byte above the limit fails on either stream,
drains/reaps the child, and permits the next command. Concurrent Python children
have independent efi module state and create independently checked saved files.
Nine cases passed in the combined run; the corrected multi-instance case and
the strengthened restriction case passed as focused reruns.

Actual firmware command, run from the Windows-local project directory:

```powershell
.\artifacts\toolchains\uv-current\uv.exe run scripts/smoke_wasm.py --launcher target/debug/efi-agent.exe --efi target/x86_64-unknown-uefi/release/efi-agent-uefi.efi --code artifacts/firmware/OVMF_CODE_4M.fd --vars artifacts/firmware/OVMF_VARS_4M.fd --fixtures artifacts/wasm-fixtures --output artifacts/wasm-cpython-smoke --python artifacts/wasm-guests/python.wasm --ash artifacts/wasm-guests/ash.wasm --quickjs artifacts/wasm-guests/quickjs.wasm --filesystem --directories --random --memory 512 --qemu qemu-system-x86_64.exe
```

The command used the project-local uv-current/uv.exe. The actual QEMU/OVMF
test passes CPython language execution, host-interface restrictions, binary
Shell capture, errors, repeated commands and saved FAT bytes. Two Python root
instances each run ash; alpha creates its file and waits for beta's file, while
beta waits for alpha before writing its own. Both roots complete. After QEMU
stops, direct FAT reads verify python-shell.txt == PYTHON_SHELL_FILE,
python-alpha.txt == alpha and python-beta.txt == beta. The same run passes
QuickJS, real ash, process/pipe/timed-wait/environment checks, directory and
filesystem byte oracles, and actual libc entropy checks. Wire and disk evidence
are retained in artifacts/wasm-cpython-smoke.

The first combined concurrent firmware run with 256 MiB failed while initializing
the second Python root, with an explicit CPython out-of-memory error. The test
now accepts --memory; the same workload passes with 512 MiB. Root Stores retain
execution memory until explicit reap. Memory reclamation at exit remains work;
the larger test allocation does not resolve that lifecycle limitation.

Intermediate issues resolved: CPython's external importer required the removed
posix module, so external importer initialization is omitted in the copied
restricted import core. Moving that source required its generated clinic header,
which is now copied too. Nested firmware Python source now uses a raw outer
string to preserve binary escapes. Focused unittest selection needed a fully
qualified __main__ name. The multi-instance fixture omitted the printf argument
and saved empty files; its command now passes $1 and the saved-byte check passes.
One documentation patch had incorrect context and was reapplied against the
actual section. MSYS2 Clang remains excluded; its incompatible runtime has not
been changed. Upstream HACL endian macro redefinitions and unused external
import-helper warnings remain in the CPython build log. The supplied Current
state omits the WASM work; the local state has been updated. curl, WolfSSH,
guest sockets and complete application-world verification remain unfinished.

## Execution memory is released before process reap

The process record now separates execution resources from wait/reap metadata.
Normal Wasm return, proc_exit, traps and host cancellation drop the Store,
linear memory, continuation and pending I/O immediately. Dropping the descriptor
table closes pipe endpoints and filesystem handles before waking waiters.
Parent PID, terminal status and shared stdout/stderr remain until wait/reap.
Live children retain inherited output and are adopted by PID 0 when a parent
exits. A terminal record still consumes a process slot; reaping frees that slot.

The independent allocation check in crates/wasm/tests/exit_memory.rs uses a
counting System allocator in a separate single-test binary. Each actual Wasm
instance has 64 MiB linear memory, writes separate stdout/stderr and suspends.
For return, explicit exit, trap and cancellation, the check observes a drop of
more than 60 MiB of live allocation bytes before reap, verifies the exact saved
outputs and status, checks idempotent cancellation, and finally reaps all four
records. It does not infer memory release from process status. The threshold
allows small interpreter/cache allocations while excluding retained 64 MiB
linear memory. Modules and any reusable engine caches remain world resources.

An additional actual process test reaps an exited parent before its child
continues, checks getppid changes from the original parent to PID 0, and verifies
the child's inherited output. Existing pipe tests exercise cancellation while
blocked, last-reader EPIPE, last-writer EOF and orphan progress. Existing wait
tests verify child exit codes, reaping exactly once and status-copy behavior.
All 38 Rust integration tests and all ten actual CPython/ash integration cases
pass in their combined suites. Clippy for all wasm targets passes with
warnings denied; the native runner and UEFI Release build pass.

The combined QEMU/OVMF application check now passes with 256 MiB, using the
same arguments as the CPython command above except --memory 256 and --output
artifacts/wasm-exit-memory-retry. It runs actual CPython roots with mutual file
waits and real ash children, QuickJS, process/pipe/environment/timer checks,
directory and file byte oracles, and entropy checks. Direct FAT reads verify
all three Python-created file contents. The previous need for 512 MiB in this
workload is resolved by dropping terminal execution resources. This does not
establish that every workload of up to 32 live instances fits in 256 MiB.

Intermediate issues: the first allocation fixture attempted to grow memory in
one dispatcher step. Fuel charging suspended before growth completed, so its
measurement was premature. The fixture now declares the full memory at module
instantiation and measures terminal release after an explicit yield. One
combined firmware run stopped before Python when a serial view frame contained
ANSI bytes instead of its JSON payload. The retry passed without changing or
relaxing the parser. The original failure is retained under
artifacts/wasm-exit-memory-smoke; the transport corruption remains unroot-caused.
QEMU also emitted a Windows GLib application-manifest warning; it did not prevent
the successful run. A source search used a nonexistent network.rs/vm filename;
the actual TCP source is crates/uefi/src/tcp.rs. The supplied Current state still
omits the WASM work; the local state has been maintained. No global compiler or
package changes were made. curl, WolfSSH and guest sockets remain unfinished.

## Initial cooperative TCP ABI and actual native network checks

The no_std runtime now accepts a nonblocking network provider. The initial
efi_net.tcp_connect import validates four address bytes, port 1..65535 and a
four-byte result range before opening a connection. It reserves a descriptor
while connecting and suspends the guest in Connecting. Only successful
completion publishes the descriptor. Failure closes the reservation and leaves
the result unchanged. Provider methods must return immediately; pending I/O
uses the existing retained continuation and descriptor polling paths.

Socket descriptors share connections across duplicates. Existing WASI
fd_read/fd_write/poll_oneoff/fd_close and descriptor duplication/renumbering
apply. Socket fdstat reports SOCKET_STREAM. Dropping the last descriptor closes
the native connection. Native run_guest uses mio 1 with nonblocking TCP and
zero-time event polling. mio is a dev dependency only; firmware remains no_std
and receives no native socket implementation.

Four ABI cases verify output/address memory boundaries, unsigned port endpoints
and invalid ports, descriptor exhaustion before provider invocation, pending
connect suspension without duplicate attempts, asynchronous refusal with retry,
descriptor reuse and pending-connect cancellation cleanup. A separate actual
native TCP test blocks one WASM root in a socket read, completes another root
with exit code 37, cancels the blocked root and observes EOF at the server.

The actual libc network fixture and scripts/test_network.py pass on Windows:

```powershell
.\artifacts\toolchains\uv-current\uv.exe run --no-project scripts/test_network.py --runner target/debug/examples/run_guest.exe --fixture artifacts/wasm-fixtures/network.wasm
```

The local server independently checks all 196,613 request bytes, then waits
for a guest child to save network-peer.txt == PEER_PROGRESS before replying.
The child must run while its parent waits for the socket. The server sends
131,089 response bytes in 317-byte fragments and half-closes; the guest reads
in 997-byte buffers, validates every byte, and saves network-response.bin.
The test independently checks saved bytes and observes the guest closing its
last duplicate socket descriptor. A bound, non-listening local port verifies
real connection refusal and unchanged result output without a port-reuse race.
Both cases pass. The existing 38 Rust tests plus four network ABI cases and
one native network case pass (43 total). Clippy with warnings denied, native
runner build and UEFI Release build pass. Source formatting and diff checks
are required below.

Intermediate issues: the SDK uses subscription.u.u.fd_read, not the initially
assumed fd_readwrite union member. The fixture was corrected and compiled with
warnings treated as errors. An initial search expected subscription definitions
in wasi/api.h; the current SDK defines them in wasi/wasip1.h. Network/tcp source
inspection used one nonexistent network.rs path; the actual file is tcp.rs.
No MSYS2 compiler, global package update or network download was needed. The
supplied Current state still omits WASM progress; local state is maintained.

This evidence proves native TCP integration only. UEFI does not yet install
a network provider and tcp_connect returns NOTSUP there. Stable asynchronous
TCP4 tokens, DNS, guest socket controls, actual curl and WolfSSH application
ports, and firmware network verification remain unfinished. This initial ABI
does not replace any of those full-goal requirements.

## Actual UEFI cooperative TCP4 provider

The firmware world now installs tcp/cooperative.rs as its network provider.
It uses separate TCP4 child protocols and GET_PROTOCOL service-binding access
for shared NIC use. Optional CONFIG.JSON static IPv4 is passed into the world;
without it the provider starts DHCP policy and advances default mapping during
polls. Mapping and connection use a 30-second deadline without internal waits.
Connection, receive and transmit tokens, their completion contexts, packets and
byte buffers have stable owned heap storage across WASM suspension. Network
polls run at APPLICATION and callbacks only queue notifications.

Read readiness queues a receive and retains at most 64 KiB until reads consume
it. Writes copy/accept at most 64 KiB and report that prefix once. Following
writes wait for completion, and writable polling retires completed transmits.
Dropping the last descriptor queues abortive close then synchronously resets
TCP4, dispatches completions and asserts all queued tokens are retired before
freeing storage, closing the protocol and destroying the child. Abortive close
can discard accepted writes still in flight; graceful flush and shutdown are
not implemented. DNS and guest POSIX socket integration remain required.

Actual Windows-native QEMU/OVMF check:

```powershell
.\artifacts\toolchains\uv-current\uv.exe run scripts/smoke_wasm.py --launcher target/debug/efi-agent.exe --efi target/x86_64-unknown-uefi/release/efi-agent-uefi.efi --code artifacts/firmware/OVMF_CODE_4M.fd --vars artifacts/firmware/OVMF_VARS_4M.fd --fixtures artifacts/wasm-fixtures --output artifacts/wasm-network-cancel --network --qemu qemu-system-x86_64.exe
```

This check passes with 256 MiB, e1000 and QEMU NAT static IPv4. A separate local
server verifies all 196,613 request bytes. It sends no response until the guest
child emits NETWORK_PEER_PROGRESS after saving its file. The firmware UI and
child must make progress while the root waits in TCP read polling. The server
then sends 131,089 independently derived bytes in 317-byte fragments and sends
EOF. After shutdown, direct FAT reads verify every response byte and the exact
PEER_PROGRESS file. A second connection blocks in fd_read; Esc cancels it with
status 130 and the server observes EOF or reset. The provider's pending-token
retirement assertions pass. network-server.json records accepted connection,
request/response byte counts, child progress and observed cancellation close;
serial.bin and disk.img preserve firmware and saved-file evidence.

The same run passes the process, pipe, timed-wait and environment integration
checks. Native TCP integration tests pass with the updated fixture. Firmware
Clippy with warnings denied, Release build, formatting, Python source parsing
and diff checks pass. This verifies static IPv4 on this OVMF/e1000 setup only;
DHCP, physical NICs, multiple simultaneous firmware sockets, connect cancellation
and all transport failure paths need further verification. curl and WolfSSH
are still not ported or verified, so the complete requested world is unfinished.

Intermediate issues resolved: the first server handshake timed out because
/wasm run collected output without redrawing it during network polling. The
loop now redraws new messages immediately, and the same handshake passes.
Completed transmit readiness was initially left pending; readiness now collects
completion. Writes now report copied-prefix acceptance once, avoiding completion
counts being attributed to a different writer on a duplicate descriptor.
The test initially removed the first `none` argument while replacing -net none;
that could remove the display value, so it now removes the adjacent option pair.
Rust requires parentheses around an unsafe block used as the left comparison
operand in the close-queue closure; this syntax error was corrected. Clippy's
collapsible-if finding was fixed with a let-chain. No dependency or network
download error occurred. MSYS2 Clang remains excluded and global tool packages
are unchanged. The supplied Current state omits WASM progress; local state is
updated with the actual verification scope and pending work.

## Normal TCP close drains accepted sends cooperatively

The network provider now exposes poll_close and the guest import
efi_net.tcp_close(fd). It waits for outstanding accepted sends, then closes
the shared connection normally. Pending close suspends the process in Closing;
other processes remain runnable. Completion or transport error consumes the
selected descriptor. Unknown and non-socket descriptors return BADF/NOTSOCK
without changing them. Duplicate descriptors continue to exist but data
operations return NOTCONN. Normal close is explicit: fd_close and cancellation
still use abortive cleanup, and half-shutdown is not implemented.

Firmware waits for its copied transmit prefix to complete before queuing a
boxed TCP4 close token with abort_on_close=false. A 30-second deadline bounds
drain/close. The same reset and retirement assertions protect pending normal
close tokens on cancellation or errors. A completed normal close does not
queue a second abortive close during resource destruction.

Seven network ABI tests pass, including normal-close suspension with another
root completing, descriptor consumption exactly once, cancellation while
closing and exact transport-error propagation. All prior Rust tests pass
(46 cases including the added focused error case). Three actual Windows TCP
integration tests pass. The new test sends 196,613 independently checked bytes
and closes immediately after the final accepted write; the server reads to EOF
and verifies all bytes. It also checks that the closed descriptor is BADF and
a remaining duplicate returns NOTCONN without changing the read-count output.

The actual QEMU command from the preceding section passes with output
artifacts/wasm-network-graceful. Its third connection runs that same real libc
normal-close fixture. The independent server reads in 419-byte chunks until
EOF, checks the complete asymmetric payload, and only then closes its end.
network-server.json records graceful_bytes=196613 and graceful_eof=true. The
run also verifies prior transfer, child progress, blocked-read cancellation,
saved FAT byte oracles and process/pipe/environment checks. Release build,
firmware and wasm Clippy with warnings denied pass. No new environment,
dependency or download failure occurred. Formatting, diff and source checks
are run for the final sources. The supplied Current state omits WASM work;
the local state is maintained. DNS, guest POSIX networking, curl and WolfSSH
remain unfinished; successful normal close does not establish those ports.

## Cooperative WASM DNS over TCP

The guest efi_net.resolve_ipv4 import now resolves one ASCII hostname to an
IPv4 A address through the installed TCP provider. Firmware passes its configured
DNS address and port into World; the native runner reads EFI_WASM_DNS. Queries
retain their own connection and suspend in Resolving. Input/output ranges and
hostname syntax are validated before side effects. A cryptographic two-byte
transaction ID and a 10-second realtime deadline are required. Terminal paths
drop the query before publishing its four-byte address; errors preserve output.

Windows cargo test -p efi-agent-wasm passed all integration and library tests.
Four new ABI cases cover a deadline at 10,000,000,731 ns and the preceding
nanosecond, immediate connection release before reap, cancellation, a failing
clock during suspension, invalid memory/name input, failed entropy collection
and deadline overflow. The deterministic providers count random calls, network
attempts and connection drops. Guest assertions check the exact errno and an
unchanged sentinel at the last valid four-byte memory range. The final library
rerun passes three parser cases, including CNAME records before/after the A
record, duplicate equivalent aliases, conflicting targets, A+CNAME conflicts,
alias cycles and malformed A/CNAME RDATA lengths. Every truncated prefix of a
valid compressed response is rejected.

The rebuilt native run_guest passes scripts/test_dns.py with an independent
local TCP server for six variants: A, CNAME, NXDOMAIN, wrong transaction ID,
wrong question and an invalid compression pointer. The server checks the
normalized mixed.example IN A query and sends the response frame one byte at
a time. Actual guest libc calls check 203.0.113.7 or the exact error with an
unchanged output buffer.

Actual QEMU/OVMF verification passed with this Windows-local invocation:

```powershell
& .\artifacts\toolchains\uv-current\uv.exe run scripts/smoke_wasm.py --launcher target/debug/efi-agent.exe --efi target/x86_64-unknown-uefi/release/efi-agent-uefi.efi --code artifacts/firmware/OVMF_CODE_4M.fd --vars artifacts/firmware/OVMF_VARS_4M.fd --fixtures artifacts/wasm-fixtures --output artifacts/wasm-dns-smoke --network --dns --qemu qemu-system-x86_64.exe
```

The firmware test runs the same six independent DNS responses using TCP4 and
QEMU NAT, with DNS address 10.0.2.2 and a dynamically selected local server port.
It also passes TCP transfer, child progress handshake, EOF, blocked-read
cancellation, graceful close, saved FAT response bytes, process/pipe scheduling,
timed wait and process environment checks. Artifacts include serial.bin,
screen.txt, qemu.log and network-server.json under artifacts/wasm-dns-smoke.
Release firmware build, wasm all-targets Clippy and firmware Clippy with warnings
denied pass. Python AST checks, cargo fmt and git diff --check pass.

During implementation, retaining Rc<RefCell<Query>> in a wasmi HostError failed
its Send+Sync requirement. Query ownership moved to the process Context; the
suspended action carries only an output pointer. An initial TCP framing check
rejected the two-byte prefix stage; it now checks the declared message length
after the prefix is complete. A patch initially assigned DNS state in the TCP
connect path and was corrected before verification. During toolchain diagnosis,
a search included a nonexistent .cargo directory and a package query included
an uninstalled llvm package; subsequent probes used existing files/packages.
MSYS2 Clang/runtime incompatibility remains unresolved globally, and builds use
the independent Windows WASI SDK without invoking MSYS2 Clang. The supplied
Current state omits existing WASM work; the local Current state retains it.

This establishes bounded cooperative IPv4 DNS only. IPv6, IDNA, caching,
nonblocking POSIX socket controls, half-shutdown, actual curl/WolfSSH ports and
multi-application firmware verification remain unfinished. Deadlines currently
use wall time, so a clock adjustment can change the observed wait duration.

## Shared nonblocking socket flags

Socket descriptors now share a NONBLOCK flag through duplicates. WASI
fd_fdstat_get reports that flag and FD_FDSTAT_SET_FLAGS rights. Setting zero
restores blocking mode. Unsupported known flags return NOTSUP and unknown
bits return INVAL without mutation. Pending reads or writes return AGAIN
instead of suspending when NONBLOCK is set. Neither buffers nor result counts
are changed by AGAIN. Empty operations complete with count zero; EOF also
returns zero instead of AGAIN. poll_oneoff retains its readiness semantics.
Connection establishment still uses the existing blocking cooperative import.

All Rust tests pass, including the new deterministic network case. Its WAT
guest checks flags through both duplicates, rejected changes, advertised
rights, read/write backpressure, output sentinels and empty operations. It
restores blocking mode and reaches Reading(3), then cancellation releases the
last shared connection. Four actual Windows TCP integration cases pass through
the rebuilt run_guest and compiled libc fixture. The new server withholds data
until receiving byte 0xa7, independently proving the initial read has no data.
The guest checks AGAIN, restores blocking mode, receives exactly [19,73,211],
polls EOF, switches to NONBLOCK and checks successful count zero.

Actual QEMU/OVMF verification passes with:

```powershell
& .\artifacts\toolchains\uv-current\uv.exe run scripts/smoke_wasm.py --launcher target/debug/efi-agent.exe --efi target/x86_64-unknown-uefi/release/efi-agent-uefi.efi --code artifacts/firmware/OVMF_CODE_4M.fd --vars artifacts/firmware/OVMF_VARS_4M.fd --fixtures artifacts/wasm-fixtures --output artifacts/wasm-network-nonblocking --network --dns --qemu qemu-system-x86_64.exe
```

network-server.json records nonblocking_eof=true as well as the complete
196,613-byte graceful payload, 131,089-byte response, child progress and
cancelled-peer closure. All six firmware DNS cases and process/pipe/environment
checks pass. The firmware release build, wasm all-targets Clippy and firmware
Clippy with warnings denied pass. Source formatting, Python syntax and diff
checks pass.

The first new WAT rights assertion used i32.eqz with an i64 mask, so module
validation rejected the test. It was corrected to i64.eqz without changing
the assertion. An SDK header probe used wasm32-wasi rather than the installed
wasm32-wasip1 include directory; scoped discovery found the correct directory.
QEMU emitted a GLib-GIO warning for a missing ChatGPT AppxManifest.xml, but
completed all checks. A documentation patch expected single quotes where the
source used double quotes; the patch was corrected and reapplied. MSYS2 Clang
remains excluded; no global package or PATH changes were made. The supplied
Current state omits existing WASM work; the local state retains it. POSIX socket
creation, asynchronous connect/SO_ERROR, half-shutdown, curl/WolfSSH ports and
full application verification remain open.

## Separate socket creation and asynchronous connect

efi_net now provides tcp_open, tcp_connect_fd and tcp_error. The no_std Socket
wrapper owns New, Connecting, Connected, Failed and Closed state, shared by
duplicate descriptors. A failed attempt releases its provider connection
immediately while retaining the descriptor and a shared pending error. Reading
that error clears only the report. A failed socket can start another attempt;
a pending repeat returns ALREADY and a connected repeat returns ISCONN.
NONBLOCK connect returns INPROGRESS until completion. Blocking mode retains
the continuation without creating another descriptor. Write polling advances
pending connects and returns a completion/error event. tcp_error validates
output memory before progressing or clearing the report. The convenience
tcp_connect path still removes its reserved descriptor on failure.

All Rust regression tests pass. The final focused network rerun passes eleven
cases, including three added state-machine cases. These independently verify
pending polling, shared errors, failed-error clearing, invalid output preserving
the pending error, retry, NOTCONN after error clearing, range/port/descriptor
validation, blocking failure with a retained descriptor, peer scheduling and
cancelled pending poll cleanup with duplicate descriptors. Controlled provider
attempt/drop counts verify exact transport lifetime and no duplicate attempt.

Six real Windows TCP integration cases pass with the rebuilt native runner and
WASI SDK-compiled libc fixture. New cases create NONBLOCK sockets separately,
connect, poll write completion, retrieve and clear the error, verify ISCONN,
exchange handshake bytes and reach EOF. A bound non-listening port independently
checks asynchronous CONNREFUSED, retained descriptor closure and cleared error.
Both immediate and pending connection outcomes are accepted by the integration
fixture; deterministic tests require the pending path explicitly.

The actual firmware command passes:

```powershell
& .\artifacts\toolchains\uv-current\uv.exe run scripts/smoke_wasm.py --launcher target/debug/efi-agent.exe --efi target/x86_64-unknown-uefi/release/efi-agent-uefi.efi --code artifacts/firmware/OVMF_CODE_4M.fd --vars artifacts/firmware/OVMF_VARS_4M.fd --fixtures artifacts/wasm-fixtures --output artifacts/wasm-network-async --network --dns --qemu qemu-system-x86_64.exe
```

The fifth server connection verifies the separate open/connect/poll/error path
and exact [19,73,211] data following the 0xa7 guest handshake. Independent
network-server.json records async_eof=true. Prior 196,613-byte request and
normal-close payload, 131,089-byte response, cancelled-peer closure, nonblocking
read, child progress, saved FAT response checks, six DNS variants and
process/pipe/environment checks all pass. Firmware Release build, both Clippy
checks with warnings denied, formatting, Python AST and diff checks pass.

No new dependency, shell, build or network error occurred. MSYS2 Clang remains
excluded; global packages and PATH were not changed. The supplied Current state
omits existing WASM work; local state retains it. These imports establish the
connection ABI only. Guest POSIX socket/connect/getsockopt wrappers,
half-shutdown, curl/WolfSSH builds and full application checks remain open.

## Guest POSIX C networking

guests/posix/network.c and its two headers now provide actual WASI Preview 1
socket, connect, getsockopt, send, recv, getaddrinfo, freeaddrinfo and gai_strerror
functions. The layer uses the verified efi_net imports and SDK byte-order,
errno, fcntl, poll and close support. It handles IPv4 TCP only and reports
unsupported features explicitly. No host OS socket handles enter guest memory.
The standard fixture builder now builds posix_network.wasm with this layer.

Four Windows integration cases in scripts/test_posix_network.py pass against
actual run_guest. Numeric resolver assertions independently check address
bytes [203,0,113,7], port bytes [192,1], port limits 0/65535 and rejection of
65536, signed/malformed ports and service names. AI_NUMERICHOST avoids DNS.
Loopback/passive defaults, result cleanup, unsupported family/type/flags,
canonical-host restrictions and SOCK_CLOEXEC rejection are checked. Real TCP
checks exercise socket/connect, SDK poll/fcntl, duplicate flags, send/recv,
null buffers, unsupported peek, exact bytes and EOF. A non-listening native
port must produce ECONNREFUSED. SO_ERROR short buffers preserve the pending
error, full reads publish it and subsequent reads return zero. Six independent
DNS responses verify getaddrinfo A/CNAME success, NXDOMAIN as EAI_NONAME and
malformed ID/question/compression as EAI_FAIL.

The Windows WASI SDK compiler passes -O2 -Wall -Wextra -Werror. The final actual
QEMU/OVMF command passes:

```powershell
& .\artifacts\toolchains\uv-current\uv.exe run scripts/smoke_wasm.py --launcher target/debug/efi-agent.exe --efi target/x86_64-unknown-uefi/release/efi-agent-uefi.efi --code artifacts/firmware/OVMF_CODE_4M.fd --vars artifacts/firmware/OVMF_VARS_4M.fd --fixtures artifacts/wasm-fixtures --output artifacts/wasm-posix-network-final --network --dns --posix-network --qemu qemu-system-x86_64.exe
```

The firmware runs the real POSIX fixture over TCP4, verifies byte order and
resolver boundaries, performs connect/poll/fcntl/send/recv, and checks a timed
out connection's SO_ERROR preservation/clearing. Its sixth server connection
checks the 0xa7 handshake and exact [19,73,211] bytes; network-server.json records
posix_eof=true. Six additional POSIX getaddrinfo calls use the independent DNS
server, in addition to all six raw DNS ABI calls. Previous process/pipe/env,
network payload, cancellation, normal close, async connect and saved FAT byte
checks pass. Final source syntax and diff checks pass. No Rust source changed
in this step; the previously verified firmware and runner were used.

The initial firmware test assumed the bound non-listening host port would
return ECONNREFUSED. It instead returned ETIMEDOUT (73), confirmed by a guest
diagnostic and independent network.pcap: repeated SYN packets have no reverse
RST or ICMP reply. Failed runs are retained under artifacts/wasm-posix-network,
artifacts/wasm-posix-network-diagnostic and artifacts/wasm-posix-network-packets.
The test now explicitly checks timeout for this NAT path; its expected error
can be selected with --posix-connect-error timeout|refused for other QEMU
environments. Native ECONNREFUSED assertions remain unchanged. The final packet
capture is retained with the passing firmware artifacts. A documentation patch
joined two lines incorrectly and was corrected before the final diff check.
MSYS2 Clang remains excluded. The supplied Current state omits existing WASM
work; local state retains it.

This is a client IPv4 TCP layer. SOCK_CLOEXEC, setsockopt, address queries,
listen/accept, peek/waitall, half-shutdown and hostname canonical target output
are not implemented. Actual curl/WolfSSH builds and named-application combined
firmware checks remain required.

Follow-up for the reported entry-point dialogs on 2026-10-11: Windows-native
PE inspection again found the imported `__emutls_v._ZSt11__once_call` symbol
in MSYS2 `libclang-cpp.dll` and no matching export in its installed
`libstdc++-6.dll`. No affected Clang process was running during inspection.
The explicit Windows WASI SDK 34 compiler passed `--version`, and
`build_wasm_c_fixture.ps1` completed successfully with that SDK. The global
MSYS2 installation remains unchanged and unresolved. A Windows ripgrep
invocation initially used a shell-style wildcard as a file operand; the
inspection was repeated with `-g` filters.

## Actual curl HTTP and socket close-on-exec

On 2026-10-11, scripts/build_curl.ps1 built upstream curl 8.22.0 with the
explicit Windows WASI SDK 34 compiler. Native MSYS supplies configure/make
only. The intermediate build disables TLS, threaded resolution, socketpair,
IPv6, FTP and TFTP. Other advertised protocols are not verified.

Socket creation now accepts EFI_NET_CLOEXEC (0x10000) alongside NONBLOCK (4).
The former is descriptor-local; the latter remains shared connection state.
Default spawn excludes marked standard descriptors. Explicit spawn_fds, dup2
with different descriptors and dupfd clear the copy's mark. Same-fd dup2 and
fd_renumber preserve it. The integration test deliberately allocates sockets
at fd 0 after closing stdin, covers independent duplicate metadata and tests
both marked and unmarked inheritance. All wasm Rust tests, wasm/UEFI Clippy,
actual native POSIX tests and the UEFI Release build pass.

The actual curl CLI passes scripts/test_curl.py against an independent HTTP
server: redirected 131,089-byte download, chunked binary stdout, and a
196,613-byte upload with independently computed SHA256. No API credentials
or third-party provider are used. The firmware run passes:

```powershell
& .\artifacts\toolchains\uv-current\uv.exe run scripts/smoke_wasm.py --launcher target/debug/efi-agent.exe --efi target/x86_64-unknown-uefi/release/efi-agent-uefi.efi --code artifacts/firmware/OVMF_CODE_4M.fd --vars artifacts/firmware/OVMF_VARS_4M.fd --fixtures artifacts/wasm-fixtures --output artifacts/wasm-curl-http --network --curl artifacts/wasm-guests/curl.wasm --qemu qemu-system-x86_64.exe
```

The test reads both saved binary downloads and the upload digest from actual
FAT after QEMU exits and compares full bytes. Existing dispatcher, pipes,
process environments, actual TCP transfer, cancellation, graceful close,
nonblocking flags and asynchronous connection checks also pass.

Build issues resolved during this port: unconditional signal/setjmp includes
were guarded by configured feature macros without adding emulation; missing
POSIX declarations and unavailable bind/options/address operations now fail
explicitly; a static archive replaced raw-object LIBS; make targets exclude
unrelated upstream test binaries. Checked unsupported WASI directory mutation
and sock_accept imports permit CLI instantiation without fabricated success.
Curl's initial error 27 was wakeup-pipe initialization failure, removed by
--disable-socketpair. Dummy dependency files required a clean after configure;
the script now does that and forces relink for external POSIX archive changes.
An MSYS clean invocation initially lacked /usr/bin on PATH and was repeated
with the correct environment. A patch and two source searches used outdated
text/paths and were corrected. The subsequent curl error 7 was SOCK_CLOEXEC
rejection, resolved by real descriptor semantics. Curl error 23 on binary
stdout was its character-device safety check; --output - permits the intended
binary stdout and the exact-byte check remains unchanged. That failing test
also produced a server connection-reset traceback. A firmware build command
used an incorrect package name; efi-agent-uefi is the actual package and the
corrected Release build passes. QEMU emits a GLib warning for a missing ChatGPT
AppxManifest.xml, but completed the firmware checks. No MSYS Clang, global
package update, global PATH change, or network workaround was needed.

TLS, actual curl cancellation and combined named-application checks remain
required. WolfSSH remains unfinished. This step does not complete the goal.

The expanded native curl suite passes four tests, including six independent
DNS responses. Actual upstream curl uses getaddrinfo/efi_net resolution for
MiXeD.Example.; A and CNAME resolve to the local HTTP server and all 131,089
bytes are compared. NXDOMAIN, wrong transaction IDs, wrong questions and
cyclic compression terminate the real CLI with curl error 6 and no body.

The matching actual QEMU/OVMF combined run passes:

```powershell
& .\artifacts\toolchains\uv-current\uv.exe run scripts/smoke_wasm.py --launcher target/debug/efi-agent.exe --efi target/x86_64-unknown-uefi/release/efi-agent-uefi.efi --code artifacts/firmware/OVMF_CODE_4M.fd --vars artifacts/firmware/OVMF_VARS_4M.fd --fixtures artifacts/wasm-fixtures --output artifacts/wasm-curl-dns-final --network --dns --posix-network --curl artifacts/wasm-guests/curl.wasm --qemu qemu-system-x86_64.exe
```

This includes six raw DNS cases, six POSIX resolver cases, six real curl DNS
cases, the prior actual network scenarios and all three curl HTTP scenarios.
Both positive curl DNS downloads are independently read from saved FAT and
compared with the full body. Negative CLI statuses are checked before reap.
The first combined run exhausted the process table because completed root
metadata was retained. The driver now reaps each checked DNS/curl process;
it preserves the runtime limit and the exit-status assertions. The rerun passes.
Asynchronous connect tests additionally create NONBLOCK|CLOEXEC sockets and
verify duplicated sockets retain shared NONBLOCK without exposing CLOEXEC as
WASI fd flags. Targeted network/close-on-exec tests and final syntax/diff checks
pass. The supplied Current state was older than the authoritative worktree;
the existing WASM implementation and verification records were preserved.

## Actual curl cancellation and named application chains

On 2026-10-11, the native actual curl suite additionally verified --max-time 1
against a held HTTP response: the real CLI returns error 28 with no body and
the independent server observes connection closure. Actual QEMU/OVMF verifies
curl waiting on an incomplete HTTP response, another WASM root completing,
and cancellation/reap status 130 with server-observed TCP4 connection closure.
The held connection must remain open until the explicit cancellation.

The combination then passed actual QuickJS Promise-based zx-like commands to
real ash to actual curl and restricted CPython efi.shell to ash to curl, while
another real curl was still waiting. The HTTP server independently checks the
196,613-byte upload. Both language callers check its exact SHA256 response;
QuickJS concurrently captures another real ash command. Subsequent HTTP/file
byte checks, CPython restrictions, Python root isolation, persistent shell files,
process/pipe/env checks and network scenarios pass with 256 MiB RAM.

Passing artifacts: artifacts/wasm-curl-applications-ready. The command used
scripts/smoke_wasm.py with --network, --curl artifacts/wasm-guests/curl.wasm,
--quickjs artifacts/wasm-guests/quickjs.wasm, --ash artifacts/wasm-guests/ash.wasm,
and --python artifacts/wasm-guests/python.wasm, plus the standard Windows
launcher, Release EFI, firmware, fixture and QEMU paths recorded above.

The first combined test incorrectly matched a child's status before the
language root had completed; the driver now waits for the full-line success
marker and the Ready footer before selecting/reaping the root. A 30-second
held-server deadline also expired during CPython startup, so that test-only
endpoint now allows 180 seconds for application setup. Explicit open-before-
cancel and closed-after-cancel assertions remain. Failed artifacts are kept
under wasm-curl-applications, wasm-curl-applications-final and
wasm-curl-applications-run. A failed early exit also produced an expected peer
reset traceback during server teardown. The corrected combination passes.

TLS preparation downloaded wolfSSL 5.9.4's official stable tag archive after
GitHub API rate limiting persisted with the Windows proxy. Git tag discovery
succeeded through that proxy; the guessed release asset URL returned 404, and
the official codeload tag archive succeeded. scripts/build_wolfssl.ps1 builds
a static WASI library with SDK clang, native CMake/Ninja and single-thread user
settings. Random bytes use WASI random_get with an explicit crypto failure
on unavailable entropy. No predictable random fallback or native socket I/O is
installed. The library's COPYING and user settings are retained with the build.
Initial build issues: Windows backslashes in WASI_SDK_PREFIX broke generated
CMake escaping, corrected to slash paths and a fresh configure; TLS 1.3 needed
an explicit FFDHE size; IP certificate checks needed inet declarations.
The archive build passes. Curl TLS verification is still in progress below;
a library archive alone is not HTTPS evidence.

## Actual curl TLS 1.2/1.3 through wolfSSL

On 2026-10-11, scripts/build_wolfssl.ps1 built wolfSSL 5.9.4 and
scripts/build_curl.ps1 linked actual curl 8.22.0 against it. Builds use Windows
WASI SDK 34, native CMake/Ninja, and MSYS configure/make. There is no MSYS Clang
invocation. The shared user settings select portable math, single-thread user
I/O, TLS 1.2/1.3, certificate/IP checks and WASI random_get. Old TLS, RC4, 3DES
and the NTLM feature are excluded. Guest PEM trust is explicit via --cacert;
no default CA bundle is packaged. Source/license records remain with artifacts.

The Release native runner passes all six scripts/test_curl.py tests, including
six DNS variants and six independent TLS cases. The TLS server allows only
1.2 or only 1.3 for the two positive cases, records the actual negotiated
version, and returns the independent 131,089-byte oracle. The real CLI's entire
stdout must match. Wrong-key CA signatures, absent CA issuers, wrong IP SAN and
expired server certificates return error 60, no body and no HTTP request.

Build and verification commands:

```powershell
& ./scripts/build_wolfssl.ps1 -WasiSdk artifacts/toolchains/wasi-sdk-34.0-x86_64-windows
& ./scripts/build_curl.ps1 -WasiSdk artifacts/toolchains/wasi-sdk-34.0-x86_64-windows -Reconfigure
& cargo.exe build -p efi-agent-wasm --example run_guest --release
& ./artifacts/toolchains/uv-current/uv.exe run scripts/test_curl.py --runner target/release/examples/run_guest.exe --guest artifacts/wasm-guests/curl.wasm
& ./artifacts/toolchains/uv-current/uv.exe run scripts/smoke_wasm.py --launcher target/debug/efi-agent.exe --efi target/x86_64-unknown-uefi/release/efi-agent-uefi.efi --code artifacts/firmware/OVMF_CODE_4M.fd --vars artifacts/firmware/OVMF_VARS_4M.fd --fixtures artifacts/wasm-fixtures --output artifacts/wasm-curl-tls-final --network --curl artifacts/wasm-guests/curl.wasm --curl-tls --qemu qemu-system-x86_64.exe
& ./artifacts/toolchains/uv-current/uv.exe run scripts/smoke_wasm.py --launcher target/debug/efi-agent.exe --efi target/x86_64-unknown-uefi/release/efi-agent-uefi.efi --code artifacts/firmware/OVMF_CODE_4M.fd --vars artifacts/firmware/OVMF_VARS_4M.fd --fixtures artifacts/wasm-fixtures --output artifacts/wasm-curl-tls-no-rdrand --network --curl artifacts/wasm-guests/curl.wasm --curl-tls --random --no-rdrand --qemu qemu-system-x86_64.exe
```

Both actual QEMU/OVMF commands pass. The normal run verifies negotiated TLS
versions and exact saved FAT bytes for each positive case, plus the four
negative certificate cases before HTTP. The no-RDRAND run rejects all six TLS
attempts with error 35 and observes zero HTTP requests; WASI random failure
also preserves the guest output. Existing plaintext HTTP, binary upload,
held-request peer progress/cancellation, TCP transfer, EOF, graceful close,
nonblocking/async socket and process/pipe/environment checks still pass.

Library integration issues resolved: generated wolfSSL options.h reset the
handwritten feature macros, causing client/library declaration mismatches and
hardening warnings. WOLFSSL_NO_OPTIONS_H and a shared user-settings include
now make the configurations agree. The full upstream BIO interface is needed
by curl. The inet header is limited to library compilation so Autoconf's
connect probe does not encounter a conflicting declaration. No warning is
suppressed to hide those configuration errors.

The TLS-linked CLI introduced a real WASI path_readlink import. The runtime
now validates directory rights, path and output memory before metadata access.
Providers reject symlink access/traversal. Existing non-links return INVAL;
missing/provider errors remain exact and no outputs are written. Independent
WAT/provider tests cover file/directory, zero-size buffer, missing path,
invalid descriptors, traversal, denied capabilities and memory edges. The first
capability test referenced unimplemented fd_fdstat_set_rights; it now opens a
real reduced-rights directory with path_open. All wasm tests and both Clippy
checks pass; the UEFI Release build passes. An attempted runner relink hit
Windows LNK1104 while earlier tests still used the executable; that test session
finished and the rebuild then passed. A version preflight now fails a suite
before starting DNS peers if the actual module cannot instantiate.

Upstream curl initially classified ASN_SIG_CONFIRM_E as generic error 35 and
ASN_NO_SIGNER_E as CA-file error 77. The staged backend patch maps both peer
certificate failures to error 60 while retaining certificate validation and
other file/transport errors. The tests retain their exact expected error 60;
they were not weakened to accept any failure. The independent TLS rejection
server treats connection resets between requests or before HTTP as expected;
payload errors remain recorded. Earlier reset tracebacks are documented above.
Debug wasmi spent substantial CPU time in cryptography; the Release suite
passes in about nine seconds without increasing its timeout. WSL-mounted log
reads sometimes showed older output while Windows was still rewriting logs;
Windows-native log/process inspection was used to resolve current status.
A documentation patch used a stale line boundary, failed without edits and
was reapplied against the actual text.

WolfSSH 1.6.0 official stable-tag source is staged under artifacts/guest-sources.
Its actual WASM client and integration remain required, as do remaining shell
and guest transport operations. The full goal is still active; these TLS
checks do not prove WolfSSH.

The final 256 MiB actual firmware combination also passes with the current
TLS-enabled curl module. It includes raw/POSIX/curl DNS responses, both TLS
versions and all certificate failures, QuickJS/ash/curl and Python/ash/curl
chains while a held curl stays connected, cancellation, exact HTTP/FAT bytes,
restricted Python and QuickJS checks, concurrent Python roots, saved shell
files, actual guest entropy, and the previous process/pipe/network scenarios:

```powershell
& ./artifacts/toolchains/uv-current/uv.exe run scripts/smoke_wasm.py --launcher target/debug/efi-agent.exe --efi target/x86_64-unknown-uefi/release/efi-agent-uefi.efi --code artifacts/firmware/OVMF_CODE_4M.fd --vars artifacts/firmware/OVMF_VARS_4M.fd --fixtures artifacts/wasm-fixtures --output artifacts/wasm-curl-combined-final --network --dns --posix-network --curl artifacts/wasm-guests/curl.wasm --curl-tls --quickjs artifacts/wasm-guests/quickjs.wasm --ash artifacts/wasm-guests/ash.wasm --python artifacts/wasm-guests/python.wasm --random --qemu qemu-system-x86_64.exe
```

Final Rust formatting, PowerShell/Python syntax and Git diff checks pass.
No commit was requested or made. No physical hardware or third-party service
verification is claimed by these local QEMU/independent-server checks.

## MSYS2 Clang ABI repair and actual WolfSSH exec client (2026-10-11)

The reported Windows entry-point error was reproduced at the binary-import
level without opening another error dialog. MSYS2 Clang/clang-libs 21.1.8-4
imported `__emutls_v._ZSt11__once_call` and
`__emutls_v._ZSt15__once_callable` from libstdc++-6.dll. Installed GCC libraries
16.2.0-3 exported the newer access functions instead. Pacman integrity checks
reported no altered Clang, Clang-library or GCC-library files; this was a
mixed compiler ABI, not a corrupt DLL or a missing file.

The available signed Clang 22.1.8-2 package was downloaded and its staged
libclang-cpp.dll imports checked against the actual GCC exports. Pacman then
upgraded exactly mingw-w64-x86_64-clang, clang-libs, llvm-libs and compiler-rt
to 22.1.8-2. GCC libraries remain 16.2.0-3. No global PATH change or GCC
downgrade was performed. clang, clang++, clang-cpp, clang-format, clang-check,
clang-cl, clang-refactor, clang-repl and clang-scan-deps version checks pass.
A native C++20 two-thread call_once program builds with warnings as errors
and returns `once-ok`, verifying both compilation and runtime synchronization.
Its temporary files and staged DLL were removed. The old Current state
instruction prohibiting MSYS2 Clang startup is obsolete and was corrected.
Guest builders still select the independent Windows WASI SDK explicitly.

scripts/build_wolfssh.ps1 now builds official WolfSSH 1.6.0 library sources
and guests/wolfssh/main.c into artifacts/wasm-guests/wolfssh.wasm. It uses
the existing guest POSIX TCP ABI, upstream single-thread user-I/O hooks, and
the previously verified wolfSSL archive. No native server service or external
SSH endpoint is needed. The guest requires an explicit SHA256 host-key pin
and a bounded password file. It attempts password authentication once, opens
one exec channel, sends stdin EOF, drains both output streams, waits for close
after EOF, and propagates remote status. Missing/out-of-range status fails
with 255. The build retains upstream LICENSING and rejects linker warnings.

wolfSSL's CMake config detected HAVE___UINT128_T, while its consumers initially
lacked that feature declaration. WolfSSH therefore declared sp_cmp_d/sp_sub_d
with i32 digit arguments while the library used i64. The shared user settings
now declare the same WASI integer feature for every consumer. The rebuilt
WolfSSH and wolfSSL logs contain no warnings/errors. curl was reconfigured,
rebuilt, and passed its complete native HTTP/DNS/TLS suite with the new shared
settings, including exact certificate rejection codes.

Independent native AsyncSSH tests pass nine cases: interleaved 262181-byte
stdout and 196613-byte stderr; stderr-only window replenishment; remote status
23 with distinct streams; status 37 delivered after EOF; missing status;
wrong key rejection before authentication; incorrect password with no exec;
4096-byte accepted and 4097-byte/NUL-rejected password files; and malformed
host-key pins before networking. Native logs are artifacts/wolfssh-native.log
and artifacts/curl-ssh-settings-native.log.

```powershell
& ./scripts/build_wolfssh.ps1 -WasiSdk artifacts/toolchains/wasi-sdk-34.0-x86_64-windows
& ./artifacts/toolchains/uv-current/uv.exe run scripts/test_wolfssh.py --runner target/release/examples/run_guest.exe --guest artifacts/wasm-guests/wolfssh.wasm
```

Actual UEFI TCP4 verification passes in artifacts/wasm-wolfssh-combined.
The independent server keeps one authenticated SSH exec channel waiting while
other guest instances finish the binary output/error cases and trust/auth
negative paths. QuickJS Promise commands and restricted CPython call actual
ash-to-WolfSSH chains and check exact remote status/stdout/stderr. The held
connection must stay open through those peers. `/wasm kill` then closes it as
observed by the server, and `/wasm reap` returns 130. Saved FAT stdout/stderr
files are independently compared byte for byte. With RDRAND disabled,
artifacts/wasm-wolfssh-no-rdrand passes failure before authentication, with no
exec and no entropy fallback. Existing process/environment/pipe/clock/TCP4,
Python restrictions and concurrent Python roots also pass.

```powershell
& ./artifacts/toolchains/uv-current/uv.exe run scripts/smoke_wasm.py --launcher target/debug/efi-agent.exe --efi target/x86_64-unknown-uefi/release/efi-agent-uefi.efi --code artifacts/firmware/OVMF_CODE_4M.fd --vars artifacts/firmware/OVMF_VARS_4M.fd --fixtures artifacts/wasm-fixtures --output artifacts/wasm-wolfssh-combined --network --wolfssh artifacts/wasm-guests/wolfssh.wasm --quickjs artifacts/wasm-guests/quickjs.wasm --ash artifacts/wasm-guests/ash.wasm --python artifacts/wasm-guests/python.wasm --random --qemu qemu-system-x86_64.exe
```

Firmware tests exposed fixed 10 ms idle waits after each background WASM time
slice, causing SSH cryptography to miss the test handshake deadline. Runtime
poll now reports whether it ran a time slice. Both serial and native-console
loops continue with event dispatch after progress and retain the idle wait
without progress. Input is still checked each iteration. This preserves the
distinction between runnable CPU work and a world waiting for I/O/timers;
real timed-wait and cancellation scenarios pass with this change.

Other bypassed issues: the standalone LLVM installation has no llvm-readobj,
so the independent SDK llvm-objdump was used for PE imports/exports; the MSYS2
mirror file is mirrorlist.mingw rather than mirrorlist.mingw64; clang-tidy is
not installed in this MSYS2 prefix, so its optional startup probe was removed.
A WSL git status scan stalled and was interrupted; Windows Git with tracked
files selected supplied the status. A PowerShell linker argument containing
commas initially failed to parse; a proper argument array fixed it. The
SHA256 convenience declaration required hash.h. Exit-status functions require
WOLFSSH_SHELL. Upstream's licensing filename is LICENSING, not COPYING or
LICENSE. An early suite ran before successful linking and reported a missing
guest; the dependency was completed before rerunning. Client stream_read
reports EOF when the last channel is already retired; the client checks the
channel list before trying to drain a freed channel. A rejected password
initially retried until the dispatcher limit; the client now supplies it once.
Test command IFS expansion entered redirect filenames; explicit subshell
syntax separates stdout/stderr targets. A nested Bash/PowerShell Python syntax
probe lost its quotes; a literal heredoc fixed it. Tests preserve the original
byte/status/security expectations. Terminal draining continues while the
test waits for a peer event. QEMU's existing missing ChatGPT AppxManifest GLib
warning persists and did not prevent passing tests.

Release UEFI build, all WASM Rust tests, host/UEFI Clippy with warnings as
errors, Rust formatting, PowerShell/Python syntax and Git whitespace checks
pass. No commits were requested or made. The full WASM-world goal remains
active; remaining shell/filesystem/task/transport work is listed in
docs/wasm-world.md. No physical hardware or third-party SSH result is claimed.

The additional full curl/TLS/DNS/POSIX/WolfSSH/QuickJS/ash/CPython combination
in artifacts/wasm-applications-combined reached all SSH scenarios but failed
with 256 MiB RAM when the second concurrent Python root initialized. Python
reported `_PyErr_NoMemory` before PyExc_MemoryError was initialized; the expected
beta marker was absent. The failure is retained. The separate 256 MiB WolfSSH
combination above passes. This does not establish a minimum RAM requirement
for the complete application set.

Rerunning the same complete combination with 512 MiB RAM passes, with all
existing byte/status/security assertions retained. Results are in
artifacts/wasm-applications-combined-512 and its adjacent log. It includes raw,
POSIX and actual curl DNS, curl HTTP/TLS and certificate failures, both held
curl/SSH cancellations, both QuickJS/Python-to-ash application chains, binary
FAT output/error oracles, concurrent Python roots, guest entropy, and the
existing process/environment/pipe/timed-wait cases.

```powershell
& ./artifacts/toolchains/uv-current/uv.exe run scripts/smoke_wasm.py --launcher target/debug/efi-agent.exe --efi target/x86_64-unknown-uefi/release/efi-agent-uefi.efi --code artifacts/firmware/OVMF_CODE_4M.fd --vars artifacts/firmware/OVMF_VARS_4M.fd --fixtures artifacts/wasm-fixtures --output artifacts/wasm-applications-combined-512 --memory 512 --network --dns --posix-network --curl artifacts/wasm-guests/curl.wasm --curl-tls --wolfssh artifacts/wasm-guests/wolfssh.wasm --quickjs artifacts/wasm-guests/quickjs.wasm --ash artifacts/wasm-guests/ash.wasm --python artifacts/wasm-guests/python.wasm --random --qemu qemu-system-x86_64.exe
```

## Actual ash here-documents without fork (2026-10-11)

The previous turn made verified progress by repairing MSYS2 Clang and adding
actual WolfSSH/native/UEFI application checks. This step removes another
remaining fork dependency from BusyBox ash: upstream openhere() used a forked
pipe writer for inputs above PIPE_BUF. The port now expands the here-document
in the current shell using upstream expandhere(), then snapshots its bytes
into an anonymous read-only host stream. No writer process, fork, temporary
workspace file or filesystem permission is required.

The new efi_process.input_snapshot import copies at most 512 KiB and returns
a descriptor or negative POSIX errno. Reads consume shared state across
duplicates/inherited descriptors, then return EOF. Writes and seeks fail.
Memory bounds, the size cap and free descriptor checks precede installation;
failed requests do not consume slots. Independent Rust tests verify embedded
NUL/non-UTF8 bytes, source mutation after copy, asymmetric reads through two
duplicates, close/EOF, write/seek rejection, exact 512 KiB acceptance, one-byte
overflow, memory edges, descriptor exhaustion and reuse. All WASM Rust tests
pass; host/UEFI Clippy, Rust formatting and syntax/whitespace checks pass.

Native actual ash integration passes 14 named cases in
artifacts/ash-heredocs-native.log: byte-exact literal sizes 4095/4096/4097,
65535/65536/65537 and 196613; unquoted variable assignment/substitution and
escaped dollar; quoted literal substitution text; <<- leading-tab handling;
multiple here-documents with ordered exactly-once command substitutions;
pipeline inheritance; 300 repeated builtin redirections; and expanded
payload above 512 KiB rejected as a redirection error. Existing real shell
tasks, randomized state transfer, malformed-task and QuickJS pipeline capture
regressions pass in artifacts/ash-heredoc-tasks-regression.log.

```powershell
& ./scripts/build_busybox.ps1 -WasiSdk artifacts/toolchains/wasi-sdk-34.0-x86_64-windows -Binaryen artifacts/toolchains/binaryen-version_133
& ./artifacts/toolchains/uv-current/uv.exe run scripts/test_ash_heredocs.py --runner target/release/examples/run_guest.exe --ash artifacts/wasm-guests/ash.wasm --child artifacts/wasm-fixtures/ash_child.wasm
```

Actual 256 MiB QEMU/OVMF verification passes the same cases against independent
saved FAT stdout/stderr byte oracles in artifacts/wasm-ash-heredocs. It also
passes the existing process/environment/pipe/timed-wait, QuickJS, fresh ash,
external child status and reaping checks. Add --ash-heredocs with --ash-child
to the firmware driver:

```powershell
& ./artifacts/toolchains/uv-current/uv.exe run scripts/smoke_wasm.py --launcher target/debug/efi-agent.exe --efi target/x86_64-unknown-uefi/release/efi-agent-uefi.efi --code artifacts/firmware/OVMF_CODE_4M.fd --vars artifacts/firmware/OVMF_VARS_4M.fd --fixtures artifacts/wasm-fixtures --output artifacts/wasm-ash-heredocs --ash artifacts/wasm-guests/ash.wasm --ash-child artifacts/wasm-fixtures/ash_child.wasm --ash-heredocs --quickjs artifacts/wasm-guests/quickjs.wasm --qemu qemu-system-x86_64.exe
```

Bypassed issues: the first Rust fixture incorrectly assumed dup2 returns zero;
the import returns the target descriptor, and the fixture now checks that
contract. Early shell fixtures used arithmetic and test/[ even though these
features are not enabled; the repeated cleanup and expansion fixtures now
use existing for/case syntax and retain their original byte/resource checks.
This exposed arithmetic/test as remaining work, not verified support. An
oversize redirection returns ash status 1; the original test incorrectly
expected shell syntax status 2. The corrected fixture requires the exact
redirection diagnostic and status 1. Existing smoke_ash_tasks assumed no
filesystem provider, which is outdated: its target can now be created. The
test now uses an explicitly missing parent directory while retaining its
original status and exactly-once substitution assertion. No failure assertion
was removed. Windows log probes before build completion found no firmware
log; the confirmed live build session was retained and polled until terminal.
The existing QEMU missing ChatGPT AppxManifest warning remains non-fatal.

Large syntax trees/heredoc bodies serialized to fresh pipeline/subshell tasks
still hit the separate 28000-byte task limit. Arithmetic/test builtins,
filesystem PATH, descriptor inheritance and task failure cleanup remain open.
This step does not complete the full goal. No commits were requested or made.

The final complete 512 MiB firmware regression also passes with the rebuilt
ash module and the new here-document cases enabled. Results are in
artifacts/wasm-applications-heredocs-512 and the adjacent log. This retains
all curl HTTP/DNS/TLS/certificate, WolfSSH trust/auth/status, two cancellation
peers, QuickJS/Python-to-ash application chains, concurrent Python roots,
entropy and existing process/pipe/timed-wait assertions, and adds saved FAT
here-document output/error checks:

```powershell
& ./artifacts/toolchains/uv-current/uv.exe run scripts/smoke_wasm.py --launcher target/debug/efi-agent.exe --efi target/x86_64-unknown-uefi/release/efi-agent-uefi.efi --code artifacts/firmware/OVMF_CODE_4M.fd --vars artifacts/firmware/OVMF_VARS_4M.fd --fixtures artifacts/wasm-fixtures --output artifacts/wasm-applications-heredocs-512 --memory 512 --network --dns --posix-network --curl artifacts/wasm-guests/curl.wasm --curl-tls --wolfssh artifacts/wasm-guests/wolfssh.wasm --quickjs artifacts/wasm-guests/quickjs.wasm --ash artifacts/wasm-guests/ash.wasm --ash-child artifacts/wasm-fixtures/ash_child.wasm --ash-heredocs --python artifacts/wasm-guests/python.wasm --random --qemu qemu-system-x86_64.exe
```

## Actual ash 64-bit arithmetic and test builtins (2026-10-11)

The previous turn made verified progress with here-document input snapshots,
native/UEFI byte checks and complete application regression. This step enables
BusyBox 1.37.0's actual shell/math.c and coreutils/test.c in the ash build,
including 64-bit arithmetic, base notation and 64-bit test operands. The guest
does not expose a host identity/group database. Its supplementary groups are
empty, consistent with the existing local UID/GID model. Bash [[ remains
disabled. Real test/[ is a builtin and does not start a fresh external process.

Native tests exposed upstream logical arithmetic evaluating both sides of
&&/||, including assignments which should be skipped. The staged math patch
keeps the enclosing suppression state for each logical stack entry, disables
its right operand when its left operand decides the result, and restores the
saved state before combining values. Existing ternary suppression then
continues to work inside/outside logical branches. The build uses -fwrapv so
signed overflow is explicit two's-complement wrapping. Upstream's existing
minimum-integer divide/modulo guard is retained and tested.

scripts/test_ash_math.py passes 20 named native integration cases. Independent
oracles cover precedence/base notation, 32/64-bit boundaries, assignment and
increment, signed wrapping including minimum/-1, nested short-circuit with
skipped division/writes, a real while/test loop, test strings and numeric
limits, regular/directory/missing/empty file checks, subshell/command-
substitution/pipeline state isolation, and 300 successive bad-number test
errors followed by successful tests. Divide-by-zero, recursion, syntax and
readonly errors return shell status 2; test number overflow or missing ]
returns builtin status 2 while later shell commands recover. No guest trap
is accepted. Eighty structured random arithmetic expressions match independent
Python integer results. Sixty nested logical/ternary trees match an independent
recursive evaluator's result and variable side effects, without depending on
the production parser's operator-stack representation.

```powershell
& ./scripts/build_busybox.ps1 -WasiSdk artifacts/toolchains/wasi-sdk-34.0-x86_64-windows -Binaryen artifacts/toolchains/binaryen-version_133
& ./artifacts/toolchains/uv-current/uv.exe run scripts/test_ash_math.py --runner target/release/examples/run_guest.exe --ash artifacts/wasm-guests/ash.wasm --child artifacts/wasm-fixtures/ash_child.wasm
```

The build log has no diagnostics. Existing native task transfer/random state,
malformed-task, QuickJS pipeline, printf exact-byte/NUL/integer and all
here-document checks pass in artifacts/ash-math-tasks.log,
artifacts/ash-math-printf.log and artifacts/ash-math-heredocs.log. New tests
are in artifacts/ash-math-native.log. Python/PowerShell syntax, guarded
upstream source patch markers and Git whitespace checks pass.

Bypassed issues: test's missing index_in_strings and bb_basename helpers were
implemented with their actual string semantics; no unrelated library emulation
was added. Test's error helper carried NORETURN while Asyncify longjmp first
returns during unwind. The copied source now uses NOINLINE, matching the
existing ash adaptation; 300 error/recovery checks verify it. An upstream
string-offset warning was removed by using the equivalent literal format.
The first fixture writer used Windows write_text, translating LF into CRLF
inside actual shell scripts and adding a literal CR to the final argument;
fixtures now write exact UTF-8 bytes. The first native test ran after a failed
link and inspected the old ash module; build and test are now sequential with
native failures propagated. The logical-side-effect mismatch was fixed in
the engine, retaining the expected short-circuit result. Tests do not accept
the wrong side effects. The existing QEMU ChatGPT AppxManifest warning remains
non-fatal. Earlier Current state claims that arithmetic/test were disabled
are now outdated and were updated; historical verification failures remain.

Large task/heredoc serialization, PATH, additional builtins, descriptor
inheritance and task cleanup still require work. No commits were requested
or made; the full goal remains active.

Actual 512 MiB QEMU verification in artifacts/wasm-applications-math-512
passes all 20 math/test cases using independently checked FAT stdout/stderr
bytes, including the structured arithmetic and nested side-effect oracles.
It also retains all HTTP/DNS/TLS/certificate/WolfSSH/entropy/cancellation,
QuickJS/Python-to-ash application-chain, concurrent-Python, here-document,
external-child/reap and process/pipe/timed-wait checks. This verifies the
rebuilt ash module in the complete named-application environment:

```powershell
& ./artifacts/toolchains/uv-current/uv.exe run scripts/smoke_wasm.py --launcher target/debug/efi-agent.exe --efi target/x86_64-unknown-uefi/release/efi-agent-uefi.efi --code artifacts/firmware/OVMF_CODE_4M.fd --vars artifacts/firmware/OVMF_VARS_4M.fd --fixtures artifacts/wasm-fixtures --output artifacts/wasm-applications-math-512 --memory 512 --network --dns --posix-network --curl artifacts/wasm-guests/curl.wasm --curl-tls --wolfssh artifacts/wasm-guests/wolfssh.wasm --quickjs artifacts/wasm-guests/quickjs.wasm --ash artifacts/wasm-guests/ash.wasm --ash-child artifacts/wasm-fixtures/ash_child.wasm --ash-heredocs --ash-math --python artifacts/wasm-guests/python.wasm --random --qemu qemu-system-x86_64.exe
```

An additional language-interface fixture initially contained a literal space
inside the JavaScript diagnostic string `bad number`. /wasm run splits on
whitespace, so QuickJS received excess arguments and returned usage status 2.
The fixture now represents that space as a JavaScript hex escape, preserving
its exact diagnostic/status assertion. This was a fixture argument-encoding
failure, not an arithmetic/runtime failure.

The next language check exposed two further fixture errors. The actual test
diagnostic is `invalid: out of range\n`, not `bad number`. Both language
fixtures now compare the exact diagnostic and status 2. QuickJS now awaits
the outer Promise, so a rejected assertion fails the root process instead
of leaving an unobserved rejection. Both exact fixture programs pass the
native guest runner and print their QUICKJS_ASH_MATH_OK/PYTHON_ASH_MATH_OK
markers. Python's generated source is also compiled before native execution.

Interrupting a test left its QEMU process holding disk.img open (WinError 32).
The process was identified by the exact language-test output path and stopped;
the previous test process was allowed to finish before reusing its log file.
An initial CIM query lost its quoted filter through Bash quoting. The corrected
literal heredoc retained the Windows query quotes. No unrelated VM was stopped.
Clang and clang++ 22.1.8 startup checks were repeated successfully after this
cleanup. The earlier instruction treating MSYS2 Clang as unusable remains
obsolete; the explicit WASI SDK selection still applies to guest builds.

The corrected 512 MiB QEMU run completed with exit status 0 in
artifacts/wasm-ash-math-languages; artifacts/ash-math-languages.log reports
all 20 arithmetic/test cases and both dedicated language interfaces as PASS.
The real QuickJS Promise.all concurrently captures the sum-of-squares loop
(0..20 = 2870) and the rejected test result. Restricted CPython captures the
same loop and checks the test failure with check=False. Both compare exact
stdout/stderr and status. Saved FAT fixture outputs are checked independently.

## Explicit process descriptor mappings (2026-10-11)

efi_process.spawn_mapped_fds creates a fresh instance with explicit argv,
environment and descriptor pairs {child target, parent source}. The map
replaces standard stream inheritance, supports 0..127 entries and reserves
target fd 3 for the configured WASI workspace. It uses the unchanged parent
table, preserving swaps and duplicate-source sharing. Explicit copies clear
close-on-exec only in the child. Validation completes before spawning.

The new descriptor_mapping integration tests verify binary input across
two targets after the parent closes its source, stdout/stderr swapping,
absent unmapped stdin, empty maps, reserved/duplicate/out-of-range targets,
bad memory, count overflow, absent sources, and no PID consumed on failure.
The existing socket close-on-exec integration test now also exercises the
mapped API. All efi-agent-wasm targets pass, with output in
artifacts/descriptor-mapping-tests.log; the added close-on-exec case passed
again after the full run. The release UEFI build also passes.

scripts/fixtures/wasm/descriptor_mapping.c builds with the explicit Windows
WASI SDK. Actual QEMU runs it against the firmware's FAT workspace: a child
reads the copied binary snapshot through fd 4 and fd 127, closes its copies,
and the parent still owns a valid source at EOF. A second child checks all
128 descriptor numbers with a full 127-entry map plus the fd 3 preopen.
Reserved-fd rejection and exact child status are also checked. The run
completed with status 0; artifacts/descriptor-mapping-firmware.log contains
the explicit mapping PASS marker.

```powershell
& ./scripts/build_wasm_c_fixture.ps1 -WasiSdk artifacts/toolchains/wasi-sdk-34.0-x86_64-windows
& ./artifacts/toolchains/uv-current/uv.exe run scripts/smoke_wasm.py --launcher target/debug/efi-agent.exe --efi target/x86_64-unknown-uefi/release/efi-agent-uefi.efi --code artifacts/firmware/OVMF_CODE_4M.fd --vars artifacts/firmware/OVMF_VARS_4M.fd --fixtures artifacts/wasm-fixtures --output artifacts/wasm-descriptor-mapping --descriptor-mapping --qemu qemu-system-x86_64.exe
```

Bypassed issues: the first no_std compilation lacked alloc's vec macro import;
the import was added and compilation/tests repeated successfully. The initial
source inspection requested tasks.c; the actual task codec is tasks.inc and
was inspected instead. A nested-quote Python syntax probe lost its string
quotes at the Bash boundary; the literal-heredoc invocation passed. Python,
PowerShell syntax and Git whitespace checks pass. Documentation that restricted every spawn API to three
standard streams was updated for the new explicit mapping API. Ordinary spawn
and ash's existing invocation path still select standard streams only.
Ash's 28000-byte hex task limit remains; moving state to mapped binary snapshot
descriptors is the next integration step. No commit was made.

## Ash binary snapshot task transfer (2026-10-11)

Ash task serialization now sends its binary state through input_snapshot and
spawn_mapped_fds. Selected stdio retains its meaning; child fd 4 carries the
state and only its decimal length appears in argv. Hex encoding is removed.
The limit is now 512 KiB for the complete serialized state/tree, with the
existing depth limit of 256. The parent closes its source on both spawn success
and failure. The child requires exactly the declared bytes and EOF, then closes
fd 4 before decoding. Invalid length, flags, version or truncated node data
returns shell status 2, without accepting a guest trap as success.

Native integration passes 256 KiB variable state, oversized-state rejection,
inherited stdin cursor sharing, all previous state/tree/pipeline checks and
300 successive fresh tasks without descriptor exhaustion. The binary-input
fixture checks empty/truncated/extra bytes, invalid flags/version and truncated
state, with exact diagnostic text and child status. The expanded here-document
oracle passes 26 cases: existing 14 plus 30001/65537/196613-byte bodies through
subshells, pipelines, command substitutions and inherited functions. The exact
output bytes are derived independently from the task codec.

The focused 512 MiB QEMU run artifacts/wasm-binary-tasks-focused passes these
26 here-document cases, independently checks saved FAT stdout/stderr, and
executes the binary-input rejection fixture. Native logs are
artifacts/ash-binary-task-native.log and artifacts/ash-binary-task-heredocs.log.
The guest build has no diagnostics in artifacts/ash-binary-task-build.log.

Bypassed issues: malformed task startup used an unset arg0 and printed `(null)`;
the task entry now establishes argv[0] before decoding. A firmware test selected
the lowest successful PID, which could be an already-reaped child when the root
failed. Math/heredoc checks now find the root first and compare its actual status.
The resulting firmware evidence showed correct 30001-byte output but status 1.
Upstream printf accumulated errno after output as a conversion failure, although
successful stdio can change errno. The staged source now captures each numeric
conversion's errno before output, leaving output failures to ash's existing
ferror check. The focused QEMU retest passes without weakening output/status
assertions. Native printf regression also passes with the rebuilt source.

The native timed-read regression exhausted the guest runner's fixed 100000-step
bound before a two-second timer expired. The runner now uses a 60-second wall
deadline, independent of dispatch speed. A standalone FAT-inspection dependency
resolution hit network timeout and connection-reset errors; the command-local
Windows proxy was tried, then the existing uv smoke_wasm environment was reused
without downloads. An offline resolution attempt also lacked the requested
package version. No global environment or toolchain configuration changed.
An AGENTS.md patch did not match its expected lines and applied no edits; it was
reapplied against the actual text. Old hex/28000-byte Current state claims were
updated. Historical verification entries remain records of their tested state.

The final native read regression now passes the real timeout-then-read sequence,
IFS oracle and QuickJS pipeline. All 20 arithmetic/test cases also pass with
the final staged printf source. Logs are artifacts/ash-binary-task-smoke_ash_read.log,
artifacts/ash-binary-task-smoke_ash_printf.log and artifacts/ash-binary-task-math.log.
Python/PowerShell syntax and Git whitespace checks pass.

The final full 512 MiB QEMU combination completed with exit status 0 in
artifacts/wasm-applications-binary-tasks-512. Its adjacent log reports PASS for
HTTP/DNS/TLS/certificate failures, WolfSSH streams/status/cancellation,
QuickJS/Python application chains, concurrent Python, entropy, process/pipe
checks, all descriptor mappings, binary-state rejection, all 26 here-document
byte oracles, all 20 arithmetic/test cases and both dedicated language math
interfaces. The final rebuilt ash module is used throughout this combination.
All efi-agent-wasm targets also pass in artifacts/ash-binary-task-runtime-tests.log.

```powershell
& ./artifacts/toolchains/uv-current/uv.exe run scripts/smoke_wasm.py --launcher target/debug/efi-agent.exe --efi target/x86_64-unknown-uefi/release/efi-agent-uefi.efi --code artifacts/firmware/OVMF_CODE_4M.fd --vars artifacts/firmware/OVMF_VARS_4M.fd --fixtures artifacts/wasm-fixtures --output artifacts/wasm-applications-binary-tasks-512 --memory 512 --network --dns --posix-network --curl artifacts/wasm-guests/curl.wasm --curl-tls --wolfssh artifacts/wasm-guests/wolfssh.wasm --quickjs artifacts/wasm-guests/quickjs.wasm --ash artifacts/wasm-guests/ash.wasm --ash-child artifacts/wasm-fixtures/ash_child.wasm --ash-heredocs --ash-math --ash-task-input --descriptor-mapping --python artifacts/wasm-guests/python.wasm --random --qemu qemu-system-x86_64.exe
```

General shell descriptor inheritance, filesystem PATH lookup, remaining builtins
and the complete reproducible packaging workflow still need work. No commit
was made; the requested full environment goal remains active.

## Ash user descriptor inheritance and general close-on-exec (2026-10-11)

Close-on-exec metadata now belongs to the process descriptor table for all
descriptor kinds. get_fd_flags/set_fd_flags expose bit 0 with negative POSIX
errors; unknown bits fail without changing state. Socket creation initializes
the same metadata. Close (including graceful socket close and failed connecting
socket removal) discards flags; descriptor reuse starts clear. Duplication
clears the new copy, same-fd dup2 preserves it, and renumber transfers it while
discarding the replaced target's flags. Default spawn still selects standard
streams and omits marked descriptors; explicit maps duplicate selected sources.

Ash now maps open user descriptors into both fresh shell tasks and external
commands. Its pipe wrapper marks internal endpoints, and staged F_DUPFD_CLOEXEC
calls mark saved parser/redirection copies. Explicit task stdio selects needed
internal pipe ends without passing unrelated endpoints. The task snapshot gets
an available child descriptor >=4 and passes that number with its size, so
user fd 4 is preserved. Closed stdin is omitted rather than making task spawn
fail. The shared argv/environment packer also supports the explicit map helper.

All efi-agent-wasm targets pass in artifacts/descriptor-flags-tests.log, including
new default-spawn filtering/clear/reuse tests for console input/output, snapshots
and pipes, plus existing socket and mapping tests. The C firmware mapping fixture
checks explicit child copies clear flags while parent flags remain set, closed
number reuse, regular-file duplication/renumber/overwrite, output and pipe flags,
and fd 127/full-map/workspace checks. The release UEFI and C fixture builds pass.

scripts/test_ash_descriptors.py passes ten native scenarios with exact stdout
and empty stderr: shared fd 7 cursor across subshell and parent, user fd 4,
substitution, pipeline, external command, nested tasks, shared output file,
child close isolation, closed stdin, and 300 open/task/close cycles. Existing
task/state/large pipeline, external command, printf and read regressions pass
in artifacts/ash-descriptors-smoke_ash_*.log. The build has no diagnostics.

The full actual 512 MiB QEMU combination completed with status 0 in
artifacts/wasm-applications-descriptors-512. Its adjacent log reports all ten
descriptor cases with independent saved FAT bytes, descriptor flags/mappings,
26 here-documents, binary-state rejection, 20 arithmetic/test cases, both
language arithmetic interfaces, HTTP/DNS/TLS/certificate failures, WolfSSH,
concurrent Python, language application chains, entropy and process/pipe
cancellation as PASS. No third-party provider or physical hardware claim is made.

```powershell
& ./artifacts/toolchains/uv-current/uv.exe run scripts/smoke_wasm.py --launcher target/debug/efi-agent.exe --efi target/x86_64-unknown-uefi/release/efi-agent-uefi.efi --code artifacts/firmware/OVMF_CODE_4M.fd --vars artifacts/firmware/OVMF_VARS_4M.fd --fixtures artifacts/wasm-fixtures --output artifacts/wasm-applications-descriptors-512 --memory 512 --network --dns --posix-network --curl artifacts/wasm-guests/curl.wasm --curl-tls --wolfssh artifacts/wasm-guests/wolfssh.wasm --quickjs artifacts/wasm-guests/quickjs.wasm --ash artifacts/wasm-guests/ash.wasm --ash-child artifacts/wasm-fixtures/ash_child.wasm --ash-heredocs --ash-math --ash-task-input --ash-descriptors --descriptor-mapping --python artifacts/wasm-guests/python.wasm --random --qemu qemu-system-x86_64.exe
```

Bypassed issues: source inspection initially used wasm32-wasi rather than the
SDK's wasm32-wasip1 include directory, and an absent abi/*.rs glob; both searches
were corrected to actual files. A multi-file patch failed its header marker
and applied no changes; it was reapplied against the current text. The new fd 7
test exposed the existing dupfd wrapper assigning POSIX error numbers directly
to WASI errno. Closed targets were reported as Bad message instead of EBADF,
breaking ash's redirection branch. Explicit errno conversion fixes this, and
the exact-byte tests pass. A process poll mistakenly supplied the Windows PID
to write_stdin; the original tool session was polled next and reported success.
An absent-QEMU Get-Process probe returned nonzero after the VM exited normally;
the original test session's status 0 and PASS log are authoritative. Earlier
Current state/stdin-only and socket-only flag descriptions were updated. The
old unsupported SOCK_CLOEXEC statement was also corrected to current behavior.
Python syntax and Git whitespace checks pass. No commits were made; filesystem
PATH lookup, remaining builtins, task cleanup audit and reproducible packaging
remain work toward the full requested environment.

## WASM progress review and functional commits (2026-10-12)

This review uses the source tree, existing native logs and the final recorded
QEMU log at artifacts/wasm-applications-descriptors-512.log. No build, test or
firmware acceptance run was repeated. That log records PASS for the actual
BusyBox ash, QuickJS, CPython, curl and WolfSSH application combination,
network/TLS, concurrent instances, descriptor inheritance and language-to-shell
chains. The full combination requires 512 MiB in the recorded configuration;
the earlier full 256 MiB run failed at second concurrent Python startup.

The work is committed in dependency order by function: the no_std wasmi world
and native adapters; UEFI command/UI and firmware providers; shared C process,
capture and Asyncify jump helpers; BusyBox ash; QuickJS; CPython; POSIX network
wrappers; curl/wolfSSL; WolfSSH; and shared integration fixtures/documentation.
Existing test sources are retained. Static Git whitespace and staging checks
are used for commit hygiene only and do not establish new execution results.

The current documentation now lists the implemented WASI imports and process
helpers. Stale statements that curl/WolfSSH ports and all UEFI sockets were
pending were replaced with the remaining limits. Source acquisition, automatic
guest packaging, filesystem PATH, remaining filesystem/socket operations,
monotonic clocks, permissions and physical hardware verification remain open.

The pre-existing dirty vendor/uefi-shell tree is excluded from these commits.
Its nested EDK II checkout contains an X11IncludeHack change and dirty OpenSSL
and libspdm submodules. Those changes are retained for separate work.

Bypassed issues: the initial WSL Git status was slow on the Windows checkout
and held the index lock, blocking the first staging attempt. The review's WSL
status process was stopped; its lock cleared and staging then succeeded.
Subsequent Git operations used Windows Git through PowerShell 7 with a native
working directory. A PowerShell search command used incorrect quote escaping
and exited before it produced results; a literal Bash here-document supplied
the corrected PowerShell command. No production code was changed for these
environment issues. The old progress descriptions conflicted with later source
and verification records; historical records were retained and current state
descriptions were corrected.

## WASM memory allocation investigation (2026-10-12)

Source inspection confirms that each PID creates its own Store and instantiates
only the selected registered Module. The five applications are separate WASM
files and are not linked into one instance. Module compilation is shared across
instances through the world's Engine and registry.

A temporary Windows-native Release probe used a counting allocator, the current
guest binaries and a temporary copy of the runtime with a read-only linear-memory
size accessor. It installed the native realtime/monotonic clock and OS entropy
providers. Two Python roots execute `while True: pass`; the dispatcher advances
1000 slices for the first root and 2000 more after starting the second. Both
remain Ready with no reported first-root stderr. This is an allocation diagnostic,
not a repeated application acceptance run. Project runtime sources are unchanged.
The output is artifacts/memory-inspection-2026-10-12.log.

| Stage | Requested live host allocations | Phase peak |
|---|---:|---:|
| Five modules registered, no live instances | 32.209 MiB | 33.015 MiB |
| First Python instantiated | 51.561 MiB | 51.723 MiB |
| First Python initialized and running | 70.441 MiB | 89.313 MiB |
| Second Python instantiated | 89.772 MiB | 89.955 MiB |
| Both Python roots initialized and running | 108.652 MiB | 127.524 MiB |
| Both roots cancelled | 32.240 MiB | Prior running peak |
| World dropped | 0.001 MiB | Prior module allocation |

The counting allocator deliberately uses GlobalAlloc's default allocate/copy/free
reallocation. This matches the allocation order of uefi 0.41's allocator, which
implements alloc/dealloc but no realloc override. It does not reproduce firmware
pool overhead, fragmentation, image allocations, or OVMF allocations. Native
System reallocation can have a lower transient peak; it produces the same live
allocation totals here. The reported values count requested allocation bytes,
not Windows process working set or complete firmware physical memory.

Binary inspection finds Python initial memory at 18.875 MiB, including the build's
explicit 16 MiB C stack. Running memory.size corresponds to 19.375 MiB per root.
The large-allocation log shows each root's backing buffer growing from 18.875 to
37.750 MiB. wasmi_core 2.0.0 memory/buffer.rs uses Vec::try_reserve, whose geometric
growth doubles this capacity even though visible memory grows by only 0.5 MiB.
Each root therefore retains 18.375 MiB of unused capacity. During the first growth,
old and new buffers coexist for 56.625 MiB before freeing the old allocation.
Two running roots retain 36.750 MiB of unused linear-memory capacity in this probe.

The five initial linear memories are ash 2.0625 MiB, QuickJS 1.125 MiB, Python
18.875 MiB, curl 0.375 MiB and WolfSSH 1.0625 MiB. The 64 MiB per-instance limit
is not allocated up front. Python module registration adds about 19.483 MiB of
retained native allocations; registration does not instantiate Python memory.
Cancellation releases execution resources; no retained per-instance memory was
observed after dropping the world in this diagnostic. Small Engine stack caches
can remain until that drop.

These results identify avoidable linear-memory capacity and growth peaks, plus
the large fixed Python C stack. They do not account for all memory in the full
firmware combination or prove that correcting these allocations makes it fit
in 256 MiB. Exact-capacity growth must also consider repeated-copy cost; shrinking
the C stack requires a separate stack-usage and recursion check. No production
memory policy or guest build was changed by this investigation.

Bypassed diagnostic issues: the first temporary probe omitted host entropy and
Python exited during initialization, so its startup result was discarded.
The corrected probe installed the existing native clock and random providers.
A temporary print expression held an immutable World borrow while draining
output; moving output extraction before the print corrected the Rust borrow
error. Temporary probe source and the copied runtime were removed afterward.

## Repeated module registration retention (2026-10-12)

The earlier two-Python probe registers each named application once. It does not
represent the full firmware driver's module registration history. The full
curl/DNS/WolfSSH/Python driver registers curl twice and Python three times before
the final concurrent Python check. World::register always calls Module::new on
the shared Engine, then replaces the registry entry. The Engine's function-code
storage is append-only and retains previous compiled functions until Engine drop.
Replacing the Module entry therefore does not release all old compiled storage.

A second temporary Windows-native Release counting-allocator probe registers
ash/QuickJS/Python/curl/WolfSSH once, then reloads curl/Python/Python in that order.
It starts no instances. Recorded live requested allocations are:

| Stage | Live allocations |
|---|---:|
| Five applications registered once | 32.208 MiB |
| curl registered again | 36.357 MiB |
| Python registered again | 49.813 MiB |
| Python registered a third time | 62.269 MiB |
| World dropped | 0.001 MiB |

Results are artifacts/module-reload-inspection-2026-10-12.log. The extra retained
30.061 MiB is a confirmed registration-lifetime problem. Together with the prior
probe it suggests roughly 158 MiB requested allocation peak for a similar native
two-Python scenario after these reloads; that sum is an estimate, not a full
firmware measurement. EFI/OVMF memory use, other registered fixtures, pool
fragmentation and other simultaneous allocations still require measurement.
The observed 256 MiB failure and 512 MiB success establish neither 512 MiB as
the minimum nor a required fourfold RAM increase. No production fix was applied.

Bypassed issue: an inspection tried crates/core/src/app.rs, which does not exist;
a targeted source search found the App definition instead. The temporary reload
probe was removed after recording its output. No application acceptance run was
repeated.

## Uncommitted workspace and Shell submodule review (2026-10-21)

The existing uncommitted AGENTS.md and verification additions record the two
2026-10-12 allocation investigations. Their tables agree with the local logs.
Both logs are ignored under artifacts and must be retained separately if the
records need to be reviewed from a fresh checkout. No application test was rerun.

The Shell gitlink remains ae263171ee125bd8d0be5aff3882649cd920a7f8 and its
EDK II gitlink remains b7a715f7c03c45c6b4575bf88596bfd79658b8ce. Recursive
submodule status showed no changed commit pointers. Windows Git reports dirty
nested working trees because of filesystem access differences:

- EDK II EmulatorPkg/Unix/Host/X11IncludeHack and the pkcs11-provider README
  and tests/README are WSL symbolic links. Their targets match the committed
  link contents. Targeted WSL Git diffs pass for all three links. Windows Git
  cannot hash X11IncludeHack with its default settings; core.symlinks=true
  still reports these WSL links as type changes.
- The nested libspdm OpenSSL pyca-cryptography checkout reports 16 modified
  certificate-pair test files with default Windows Git settings. Windows Git
  diff reports Filename too long for these paths. A command-local
  core.longpaths=true makes that checkout and libspdm clean.

No actual tracked source edit was found behind the dirty Shell status. This
refines the 2026-10-12 description of retained X11/OpenSSL/libspdm changes.
The local ignored EDK II Build/Shell/RELEASE_GCC5/X64 output contains the Shell
images used by packaging. Preserve those outputs before replacing a checkout.
Keep the pinned commits; do not commit filesystem artifacts or hide the status
with submodule ignore settings. Prefer a separate WSL-local EDK II build tree
and a Windows-native vendor checkout; EFI_AGENT_SHELL can select the resulting
image after copying it to a Windows-local path. Use repository-local long-path
support where the Windows checkout needs it.

Bypassed inspection issues: Windows Git's link hashing failure was checked with
read-only WSL link inspection and targeted diffs. Command-local long-path support
resolved the deep test-file status. Broad WSL Git status was slow on this Windows
checkout and was stopped; remaining inspection used Windows Git. One child
process exited before the stop command reached it. No reset, clean, checkout,
Git configuration change, build or application test was performed.

## Workspace-relative WASM module loading (2026-10-21)

`/wasm load NAME PATH` now resolves relative paths against the configured
workspace. Both slash separators are accepted, including `./python.wasm` and
`.\python.wasm`. Paths beginning with slash or backslash resolve from the boot
volume root. The existing component validator rejects parent traversal, volume
prefixes, control characters and invalid native filename characters. Missing
file errors show the resolved path. This uses the runtime's workspace even when
model configuration is absent; the default is `\work`.

The five local guest binaries were moved from workspace/wasm into workspace
after confirming no QEMU process was running. The empty subdirectory was removed.
Python now loads with `/wasm load python python.wasm`. The documentation copy
command places guest binaries directly in the workspace. Build inputs remain
under artifacts/wasm-guests.

Windows-native verification passed:

- All 46 efi-agent-core tests, including module resolution with a custom nested
  workspace, nested relative paths, absolute paths, empty paths and invalid paths.
- Release x86_64-unknown-uefi build and Clippy with warnings denied; formatting
  and Git whitespace checks.
- Actual QEMU/OVMF with 512 MiB and TCG through scripts/smoke_wasm.py. Six path
  forms loaded real modules; missing files, volume prefixes and traversal failed
  as expected. The base process/pipe/clock/environment checks also passed.
- Actual CPython, QuickJS and ash loaded through bare workspace filenames.
  Language execution, shell capture, concurrent Python roots and independent
  saved FAT byte checks passed. Evidence is artifacts/wasm-module-paths.log and
  artifacts/wasm-module-paths. This run does not establish WHPX behavior.

Bypassed inspection issues: a search for nested AGENTS.md files returned no
matches and stopped a chained read; the source reads were rerun directly.
Two guessed source paths did not exist; targeted searches located the files.
A broad WSL Git diff was slow while traversing the pre-existing dirty Shell
submodules and was stopped; Windows Git supplied the review. One documentation
patch did not match its context and was reapplied with the exact lines.
No Shell source, submodule commit or Git configuration was changed.

## Python standard interfaces and transparency audit (2026-10-21)

This record supersedes the earlier restricted-Python interface descriptions.
The public `efi` module and custom `-e` entry point were removed. The guest uses
upstream `Py_RunMain`, FileIO and filesystem imports, and freezes upstream
`subprocess.py` with its dependencies. An internal `_posixsubprocess` backend
maps standard pipes, descriptor inheritance, environments and wait status onto
fresh cooperative WASM instances. `os.system` uses the same process ABI.
Timeouts are rejected before helper functions start a child. Process signals,
cwd and other unsupported process settings return explicit errors.

Enabling upstream posix also exposed optional WASI imports that previously
prevented module instantiation. The runtime now links advise, positioned I/O,
descriptor timestamps, remove-directory, symlink and hard-link entry points.
Unavailable operations retain descriptor/rights/range checks and return errors.
They do not claim successful file mutation. ABI integration checks verify
denied operations have no provider effects or changed output parameters.

The process tests target asymmetric boundaries: empty and quoted argument
words with shell metacharacters, strict versus replacement UTF-8 decoding,
196,613 bytes across 64 KiB pipes in both directions, reversed stderr, capture
of 524,289 bytes, descriptor 0 reused after stdin closure, reserved fd 3 and
out-of-range fd 128, valid fd 127, inherited nonstandard descriptors, and 140 child cycles.
An independent saved-file comparison verifies bytes outside the guest process.

Windows-native verification passed:

- All 11 actual CPython/ash integration tests through the Release wasmi runner.
  File imports, standard script and `-m` entry points, binary/text capture,
  environment replacement/inheritance, concurrent Python, redirection,
  `pass_fds`, status errors and rejection before effects pass. Evidence:
  `artifacts/python-transparent-native.log`.
- A final `build_cpython.ps1 -Reconfigure` run recreated the entire generated
  build tree and passed the same 11 tests, including script/-m entry points
  and fd 127. With QEMU stopped, the rebuilt binary was copied to
  `workspace/python.wasm`; no separate workspace wasm directory is used.
- All efi-agent-core and efi-agent-wasm tests, including the expanded optional
  operation ABI case; Clippy with warnings denied; Release UEFI build;
  formatting and Git whitespace checks. Evidence:
  `artifacts/python-transparent-rust-tests.log`.
- Actual QEMU/OVMF with TCG and 512 MiB: module path forms, standard Python
  script and import, file operations, 196,613-byte duplex subprocess capture,
  two concurrent Python roots, real ash capture, QuickJS, base process/pipe/
  clock/environment cases and independent saved FAT byte checks. Evidence:
  `artifacts/wasm-python-transparent-final.log` and
  `artifacts/wasm-python-transparent-final`.
- The 512 MiB TCG application combination also passes with actual curl/TLS,
  DNS/POSIX networking, WolfSSH, QuickJS, ash arithmetic and Python. Standard
  Python calls curl directly with an argument list and captures WolfSSH through
  ash while independent HTTP/SSH requests remain blocked. Cancellation and
  saved-byte checks pass. Evidence:
  `artifacts/wasm-python-transparent-applications.log` and
  `artifacts/wasm-python-transparent-applications`.

The same-root audit is in `docs/port-transparency-audit.md`. Remaining findings
include QuickJS entry/library gaps, Python socket and library packaging,
registry-only executable lookup, and whitespace-only firmware argument parsing.
The checks above do not establish WHPX, arbitrary Python package support,
filesystem executable loading, process signals or timeouts.

Bypassed implementation and environment issues:

- Targeted file searches corrected guessed test/source paths. Freezer inputs
  were corrected for Python 3.14's collections.abc alias and modules already
  frozen by upstream, which initially caused duplicate link symbols.
- CPython posix uses multiphase initialization. Adapter functions were moved
  into an appended module-exec slot instead of treating its definition as an
  initialized module. A Rust result-type inference error was also corrected.
- Old copied import.c and FileIO sources and objects remained in the build
  tree. Make's local-source preference kept the restricted importer even after
  regenerating Makefile and running clean. Removing those obsolete copies and
  rebuilding restored upstream imports and script execution.
  The build now rejects source shadows and `-Reconfigure` recreates its generated
  build tree so the same stale-source failure cannot pass unnoticed.
- A nested shell quoting error sent make to PowerShell; a literal heredoc
  corrected the shell boundary. Patch context and delete/add conflicts were
  resolved with exact contexts and sequential patches.
- The first firmware duplex test generated its data in a Python loop under
  double interpretation and exceeded its 45-second budget. The passing case
  loads independently
  generated input from FAT, retains both large pipe transfers and verifies
  saved output outside the VM. The first timeout alone did not establish a
  runtime deadlock. Upstream HACL endian-macro redefinition warnings remain
  in the successful CPython build log.
- The descriptor-127 test first used ash redirection syntax, which returned
  status 1. It now calls standard Python os.write on the inherited descriptor
  and verifies the saved bytes, so shell syntax does not mask descriptor behavior.

## Prebuilt CPython and Python process patch (2026-10-21)

This record supersedes the source-build Python implementation above. The
interpreter is Brett Cannon's CPython 3.14.7 WASI SDK 24 release, linked from
PSF webassembly. `scripts/prepare_cpython.ps1` downloads or accepts the release
archive, copies `python.wasm` unchanged, includes the complete upstream
standard library and license, and installs two Python files in site-packages.
No CPython configure, make, freezing, compiler or C adapter step remains.
The prepared and extracted binaries have identical SHA256 values in the local
check. The binary is 30,522,756 bytes. The workspace contains `python.wasm`
and `lib/python3.14` directly.

`sitecustomize` installs the private `_efi_process` monkey patch. Upstream
`subprocess.py` remains unchanged. Process creation, pipes, descriptor mapping,
inheritance flags and cooperative wait use standard WASI descriptor calls on
the process-local `.efi-process` endpoint. The endpoint defaults to
close-on-exec and runtime spawn rejects explicit inheritance. Malformed frames,
oversized arguments and invalid descriptor maps reject before effects. One
unread eight-byte response blocks further requests with AGAIN.

Windows-native verification:

- All 11 existing actual-interpreter/ash integration cases pass with the
  prepared 3.14.7 distribution. Evidence: `artifacts/python-monkey-native.log`.
  They cover CLI/script/-m, imports/files, binary/text capture, argument words,
  statuses, explicit/inherited environments, 196,613-byte duplex transfer,
  capture above 512 KiB, concurrent Python, descriptor 0/127 boundaries and
  repeated children.
- An additional site/timeout test passes: `-I` still loads the adapter, `-S`
  disables it, Decimal arithmetic works, and positional Popen wait/communicate
  timeouts raise ENOTSUP. Evidence: `artifacts/python-monkey-site.log`.
- All core/wasm tests pass; Clippy with warnings denied and formatting pass.
  Evidence: `artifacts/python-monkey-rust-final.log`. Two new control-protocol
  tests cover malformed frames, unread response bounds, protected control
  descriptor replacement and rejection without descriptor allocation. A new
  WASI socket test verifies explicit errors, range/flag validation and unchanged
  output sentinels. These targeted suites and Clippy also pass after that addition.
- The Release UEFI application builds. Actual QEMU/OVMF TCG with 512 MiB passes
  the Python/ash/QuickJS combination, standard script imports and file APIs,
  duplex subprocess capture, two concurrent Python roots, module paths, base
  process/pipe/clock/environment cases and independently verified saved FAT
  bytes. Evidence: `artifacts/wasm-python-monkey-long.log` and
  `artifacts/wasm-python-monkey-long`. Per-command timeout was 180 seconds.
- The complete 512 MiB TCG application combination also passes with the current
  prebuilt distribution: curl HTTP/TLS 1.2/1.3 and certificate rejection,
  DNS/POSIX networking, WolfSSH trust/authentication/streams/status, QuickJS,
  ash arithmetic, random, and Python. Standard Python calls actual curl and
  WolfSSH while another HTTP/SSH request remains waiting. Concurrent progress,
  cancellation with peer-observed closure and independent saved FAT bytes pass.
  Evidence: `artifacts/wasm-python-monkey-applications.log` and
  `artifacts/wasm-python-monkey-applications`. Command timeout was 180 seconds.

Limits: process names still require registration; cwd, process signals,
timeouts, credentials/groups and distinct custom argv[0] remain unsupported.
The full library includes `_decimal` and `_socket`, but socket creation lacks
a runtime backend. Native probing verifies Decimal arithmetic and explicit
socket failure (WASI NOTSUP 58). Ordinary file open defaults to inheritable
under this libc; the audit records the consequence for `close_fds=False` and
`os.system`. Dynamic extensions and native threads remain unavailable. WHPX
and minimum RAM were not verified for this distribution. Complete application
results before this section used the former custom binary.

Bypassed issues and errors:

- GitHub API rate limiting prevented the initial asset query. A direct release
  download succeeded through the available Windows proxy. Proxy configuration
  was transient and was not added to the repository.
- A misplaced initial endpoint patch and Rust result/style errors were corrected
  before verification. Protocol review added unread-response bounds, protected
  descriptor checks and overflow-safe frame-size checks.
- The original 16 MiB firmware module limit rejected the unchanged prebuilt
  binary. The loader now permits 48 MiB, matching the existing packaging bound.
- The initial QEMU run exceeded the old 45-second command wait during the Python
  script. A configurable command timeout and the 180-second rerun preserve the
  same duplex/concurrency/independent-byte checks and pass.
- A temporary native probe referenced an absent fixture. It was rerun using
  the packaged license and passed. Temporary probe source was not added.

## Filename-derived WASM registration (2026-10-21)

`/wasm load PATH` now derives the program name from the final path component,
removing a case-insensitive `.wasm` suffix. Other suffixes remain in the name.
`/wasm load ash.wasm` registers `ash`; `/wasm load tools/python.wasm` registers
`python`. `/wasm load NAME PATH` remains available for an explicit alias.
Empty filenames/stems and incorrect command arity fail before reading a module.
Help and examples use the short form.

The subtle cases are separator handling, removing only the final suffix,
uppercase suffixes, empty stems, and preserving explicit alias semantics.
The firmware integration checks relative/dot-relative/absolute paths with
both separators, `child.variant.WASM` -> `child.variant`, extensionless names,
explicit aliases, missing files, traversal/volume rejection, empty stems,
trailing separators, absent arguments and excess arguments. It clears earlier
messages before repeated markers so stale output cannot satisfy the checks.

Windows-native Release UEFI build, target Clippy with warnings denied,
formatting and whitespace checks pass. Actual QEMU/OVMF TCG at 512 MiB also
passes the new load cases, short-form ash/Python loading, standard Python
scripts/imports/files, duplex subprocess capture, concurrent Python roots,
independently verified saved FAT bytes and base process/pipe/clock/environment
checks. Evidence: `artifacts/wasm-load-filenames.log` and
`artifacts/wasm-load-filenames`. No implementation or test errors required a
workaround in this change.

## Consolidated WASM output disclosure (2026-10-22)

Each top-level `/wasm run` or `/wasm start` now owns one UI disclosure. It is
collapsed by default. Its header contains a shortened command, PID and readable
running/completed/failed/cancelled status. The body contains the full command,
standard output and standard error in separate plain-text sections, followed
by the final status. Empty stream sections are omitted. The UI updates the
same message across output chunks and completion and preserves the user's
expansion choice. It adds no second assistant completion message. Descendant
output belongs to its original root even after adoption; child exits create
no separate notifications. Concurrent roots keep independent panels.

The important boundaries are multiple output chunks versus multiple blocks,
stdout versus stderr, child versus independent-root output, clearing messages
while a background root is live, and actual cancellation versus an application
returning 130. A transcript generation prevents updates from editing an index
reused after `/clear`. Quiet long-running commands render their initial panel
before entering the dispatcher loop.

All 116 Windows-native core/wasm tests pass, including rendered UI assertions for
default collapse, merged stream chunks, keyboard expansion/collapse, retained
expansion on updates, and message isolation after clear. Release UEFI build,
target Clippy with warnings denied, host core Clippy, formatting and whitespace
checks pass. Evidence: `artifacts/wasm-output-core-tests.log`.

Actual QEMU/OVMF TCG at 512 MiB passes the dedicated output-panel integration:
actual Python subprocess capture, direct ash stdout/stderr, one collapsed
panel, hidden output before expansion, keyboard disclosure, nonzero status,
trap, Esc cancellation, independent background/root panels and continued
background status after clear. Evidence: `artifacts/wasm-output-panel-verified`
and `artifacts/wasm-output-panel-verified.log`. This is a UI integration check;
it does not rerun the full network application combination.
The final dedicated rerun also verifies an application returning 130 is
labelled Failed, while explicit cancellation is labelled Cancelled. Evidence:
`artifacts/wasm-output-panel-final-all.log` and its matching directory.
The existing base process integration also passes with the new disclosures:
module path/name cases, child status/output, traps, cancellation and recovery,
large pipelines, concurrent C capture, poll/clock and process environments.
Evidence: `artifacts/wasm-output-base-latest.log` and its matching directory.

Issues corrected during implementation and verification:

- An initial quiet-loop panel was created without an immediate redraw. An
  explicit initial render fixes the missing Running header before first output.
- The old integration expected separate process notifications. It now expands
  real disclosures and checks readable PID/status text. Repeated clicks before
  a redraw toggled the panel repeatedly; it now waits for expansion. Disclosure
  anchoring can leave the body below the viewport, so the test scrolls down and
  waits for the reported viewport to reach its bottom before checking output.
  A later regression attempt interleaved a mouse escape sequence with Esc
  cancellation; the test now waits for cancellation without sending mouse input.
  It expands the latest command panel so an older cancelled panel cannot pull
  the viewport away from the command being checked.
- A Windows Python edit used CRLF and Git reported conversion to LF. The script
  was normalized to LF. Overlapping builds briefly waited on Cargo's artifact
  lock and then completed. An interrupted test left its own QEMU child, which
  was stopped by its recorded PID; the existing user VM was left running.

## Model shell and Python tools (2026-10-22)

Both provider APIs now advertise `shell(command)` and `python(code)` alongside
the three file tools. Firmware executes the source unchanged through `ash -c`
or `python -c`. The first call loads the required workspace WASM modules;
later calls reuse the registered modules. Python also loads ash for standard
subprocess calls. Each call has fresh process state and shares workspace files.
The model prompt describes Alpine Linux-style BusyBox ash with the actual
WASM limits, including unavailable Bash syntax, Linux binaries and apk.

Checks distinguish quoted/multiline source from whitespace tokenization,
cached registration from repeated loading, stdout from stderr, a normal
nonzero exit from a host failure, exactly 65536 bytes from 65537 bytes, and a
tool descendant from an independent background root. Capture drains both
streams and retains at most 64 KiB each, with an explicit truncation marker.
Completion and cancellation cancel and reap remaining tool descendants.
The world now retains original root identity across adoption and reaping.

All 117 Windows-native core/wasm tests pass. They include source preservation,
invalid tool arguments and original-root identity after parent reaping.
UEFI and host core/wasm Clippy pass with warnings denied. Release UEFI build,
formatting and whitespace checks pass.

Actual QEMU/OVMF TCG at 256 MiB passes the Chat Completions tool loop through
a local test provider. It checks real ash and prebuilt Python execution,
Python subprocess capture, UTF-8 file content, nonzero exit, Python traceback,
capture boundaries, NUL rejection and fresh interpreter globals. Replacing
the on-disk ash.wasm with invalid text after first use does not affect later
shell or Python subprocess calls, proving that registration is reused.
Saved FAT bytes are checked independently. Evidence:
`artifacts/model-tools-chat/serial.bin` and its matching screen/QEMU records.
No external language model or provider credentials are needed for this check.

The Responses API also passes all nine code-tool calls with the 480-second
sequence deadline. Evidence: `artifacts/model-tools-responses-final/serial.bin`.
That run then failed the independent-background check because its generated
shell script had Windows CRLF endings and ash returned status 2. The fixture
now writes explicit LF bytes. The focused `--code-cancel-only` integration
runs the affected cancellation/isolation checks without repeating Python
startup or output checks.
The focused rerun passes on the final Release EFI: Esc cancels and reaps
the shell root and its child, while a separate ash loop remains Ready.
The test then cancels/reaps that independent root and verifies saved FAT
bytes, including proof that the cancelled tool started before Esc.
Evidence: `artifacts/model-tools-cancellation/serial.bin` and its matching
screen/QEMU records. The complete network application suite was not rerun.

Issues bypassed during implementation and verification:

- Initial source-location glob probes did not match files. Inspection then
  used the actual agent, environment and firmware runtime paths.
- The first integration command selected a nonexistent Release launcher.
  The test uses the existing Windows Debug launcher and the Release EFI.
- A PowerShell WMI filter lost quotes across the Bash boundary. The process
  probe used `Where-Object` with a literal program name instead.
- A documentation patch used a missing context line. It was reapplied at
  the actual end of the preceding verification section.
- The requested Alpine Linux label would overstate the implemented runtime.
  The prompt uses Alpine Linux-style BusyBox ash and states the actual limits.
- The first Responses run reached the 240-second whole-sequence test deadline
  during the third Python call under TCG. The integration timeout is now
  configurable and defaults to 480 seconds. The Chat Completions sequence
  had passed the same code and output-boundary checks.
- Windows `write_text` translated the background shell fixture to CRLF.
  The fixture now uses `write_bytes` to retain LF shell syntax.

## Filesystem environment expansion: initial interfaces (2026-10-22)

Implemented directory creation on the native and firmware providers, with WASI
rights and path validation. The write model tool creates missing parents and
reports failing paths and operations. Native rename now reaches the filesystem
provider. File writes validate all iovecs and the result range before effects,
then transfer 64 KiB chunks. Valid file calls above 512 KiB no longer fail at
the output-cache limit. Partial writes retain their completed prefix.

Finite stdin and captured output now report WASI stream types instead of
CHARACTER_DEVICE. The unchanged prebuilt CPython binary executes `python -`
with finite stdin, empty stdin and ash here-documents without a REPL banner.
Explicit native py_compile and compileall create nested caches. Automatic
import bytecode writes are enabled; native integration verifies cache
creation, reuse, explicit py_compile/compileall and -B behavior. Windows WHPX
verification also passes the full 512 MiB filesystem-tool sequence with
automatic cache creation/reuse, explicit py_compile and compileall, and -B
suppression. The guest writes its CPython magic value as an independent oracle;
saved FAT inspection confirms valid headers, source contents and absence of the
suppressed cache. Output is in `artifacts/python-cache-whpx`.

Model code tools discover workspace `.wasm` files before execution, validate
headers and sizes, and compile on first spawn. The qjs alias shares the
QuickJS image. Existing loaded programs survive source-file replacement.
Discovery commits new entries only after the complete scan validates.

The new native filesystem integration checks nested directories, trailing
slashes, existing entries, missing/non-directory parents, descriptor rights,
invalid memory before effects, cross-directory rename replacing an open file,
retained source/destination descriptors, 8 MiB multi-iovec bytes, and module
discovery/alias reuse. Actual CPython checks sizes 65535/65536/65537,
524287/524288/524289 and 8388608 against independent host bytes. The full
14-case CPython integration passes with the Release runner; evidence is in
`artifacts/filesystem-cpython-native.log`.
All 121 core/wasm tests and host/UEFI Clippy with warnings denied pass.

Actual Windows WHPX with 512 MiB passes the new model-tool sequence, tree
cancellation/background isolation, the base Chat Completions/HTTP checks,
and independent FAT verification of all seven transfer sizes. Evidence:
`artifacts/filesystem-tools-whpx-512`. This establishes WHPX execution for
these interfaces; the complete network application combination is still pending.
Actual 512 MiB QEMU TCG also passes the same filesystem, cancellation and FAT
checks through the Responses API in `artifacts/filesystem-tools-tcg-512`.
After the spawn-error correction, the final Release EFI passes the complete
512 MiB WHPX filesystem sequence again in
`artifacts/filesystem-tools-whpx-512-final`. The existing model-tool sequence
also passes final-EFI WHPX at 256 MiB in
`artifacts/model-tools-whpx-256-final`, including module reuse after source
replacement, subprocess, capture bounds, fresh interpreter state and tree
cancellation. This focused result does not resolve the larger 256 MiB
file-write/Python-to-Python combination described below.

The current release EFI was rebuilt after the virtual mount changes and passed
the complete 512 MiB WHPX Responses regression in
`artifacts/virtual-mounts-whpx-current`: nested model writes, recursive mkdir,
8 MiB transfers, finite Python stdin, cancellation/background isolation,
direct DNS/HTTPS, streamed Responses tools, FAT byte verification, and the
no-partial-tool-execution check all passed.

Remaining delivery includes filesystem PATH search, persistent interpreter
sessions, complete firmware mode/time semantics, workspace deletion/metadata
save edge cases, and the full combined firmware/WHPX application matrix.

Issues encountered and corrected:

- WSL Git status was slow on the mounted source tree. Windows Git completed
  the same inspection promptly.
- The EFI directory-type API returns Result<bool>. The first build used a
  boolean directly; propagating the API error corrected the build.
- The Debug interpreter exceeded the native runner's 60-second deadline.
  The independent Release runner passed both new Python checks in 15 seconds.
- Linking over the running Windows Debug runner failed with LNK1104. The
  Release runner uses a separate executable and avoids the active-file lock.
- A PowerShell WMI process-filter probe had incorrect nested quoting. Existing
  process handles supplied the required status; the probe was unnecessary.
- UEFI's recursive directory helper inspected an empty parent of an absolute
  path. Actual firmware exposed the error; explicit component traversal fixes
  it and checks existing components for directory type.
- Initial firmware code checks reproduced an installed Python file that ash
  could not execute. Workspace discovery and lazy compilation correct this
  registration boundary without requiring an earlier Python tool call.
- Two patch applications did not match formatted source. Reading the current
  blocks and applying exact contexts resolved them without partial edits.
- A check command accidentally passed -c to uv run. uv rejected the option;
  the separate format and whitespace checks completed successfully.
- A 256 MiB firmware check completed the 8 MiB write but failed when its
  Python oracle retained both the source payload and a whole-file read copy.
  The oracle now compares bounded chunks and also verifies saved FAT bytes.
  The subsequent 256 MiB combined sequence failed at Python-to-Python spawn;
  the 512 MiB WHPX sequence passes. The 256 MiB limitation remains under review.
- Python's private process endpoint also had a compiled-only program lookup.
  It now consults discovered modules, like ash. A raw endpoint integration
  check verifies an uncompiled alias before its first spawn.
- Spawn error handling mapped all module/instantiation failures to ENOEXEC.
  Memory allocation and memory-quota failures now map to ENOMEM in both
  process interfaces. The Python adapter translates that error explicitly.
  An actual child with a 1025-page initial memory verifies the quota failure,
  correct errno and absence of child PID allocation. The first check exposed
  Wasmi's nested Instantiation error wrapper; handling that wrapper passes.
- A dependency source lookup used an outdated file location. Searching the
  relevant crate's source located the current error definitions.
## BusyBox command registration

The command registration checks below establish basic invocation only. Most
utility implementations are handwritten subsets; their complete upstream
options and algorithms remain pending. In particular, their earlier passing
examples do not establish full sed, grep, ls, or cp behavior.

The WASM world resolves the BusyBox applet names `cat`, `cp`, `date`, `env`,
`grep`, `head`, `ls`, `mkdir`, `mv`, `rm`, `sed`, `sleep`, `yes`, and `awk` to
the registered `ash.wasm` image. The guest keeps the requested name in
`argv[0]`, so the multicall entry point selects the applet. Native Release
`run_guest` checks the applets through real ash, including `ls`, `cat`, `cp`,
`mv`, `rm`, `mkdir -p`, `head`, `grep`, `sed`, `awk`, `date`, `sleep`, and
`env`; the Windows WASI SDK 34 BusyBox build completes successfully.

The WASI preopen also exposes virtual `bin`, `dev`, `proc`, and `tmp`
directories. `/dev/null` discards writes, `/dev/zero` supplies zero bytes, and
`/proc/uptime` returns a bounded text snapshot. These entries are not exported
as workspace files.

## Upstream BusyBox awk integration

BusyBox 1.37.0 `editors/awk.c` now replaces the handwritten print-only applet.
The Windows WASI SDK 34/Binaryen 133 build passes. It uses SDK regex/math,
the upstream regex compiler helper, and cooperative ash-backed system/popen/
pclose with explicit descriptor mapping. Other applets remain incomplete.

`scripts/test_busybox_awk.py` passes 20 actual WASM cases on the rebuilt Windows
Release native runner: loop/field/array/regex/math semantics, program files,
stdin, system exit status, read/write pipes, raw pclose wait status, streams
above pipe capacity, simultaneous writers, closed stdin, explicit exit, and
regex/syntax/missing-file errors. An independent readback also matches all
195000 expected bytes. Evidence: `artifacts/awk-integration.log`.
The existing ash math/test suite passes against this same guest image;
evidence: `artifacts/awk-ash-math-regression.log`. Firmware and WHPX awk
integration are still unverified.

Encountered issues: the shell include directory shadowed the SDK math header;
quote-only header search fixes it in both BusyBox build scripts. Missing libbb
formatting helpers initially failed linking; the helper implementation fixes
the link. The existing native runner lacked awk registry support; rebuilding
it fixes command discovery. Initial diagnostic assertions assumed an applet
prefix and a different syntax diagnostic; the test now checks the actual
upstream diagnostics and exact exit status.

## Upstream BusyBox sed integration

The upstream 1.37.0 sed parser/editor replaces the handwritten literal
substitution applet. Its build uses upstream getopt32, linked-list and line
reader helpers. WASI lacks mkstemp; the platform helper uses runtime entropy
and exclusive creation. File ownership is fixed in the single-user namespace,
so the staged source omits the unavailable fchown call.

`scripts/test_busybox_sed.py` passes 23 Windows Release native WASM cases:
basic/extended regex, global substitution, captures, numeric/regex/end
addresses, multiple expressions, script files, hold space, branches,
transliteration, multiline edits, file read/write, missing final newline,
NUL bytes, a 196613-byte line, in-place replacement/backups, long options,
and missing-file/regex/parser/option errors. A separate namespace listing
checks temporary-name cleanup. Evidence: `artifacts/sed-integration.log`.
The awk integration suite also passes the resulting shared image in
`artifacts/sed-awk-regression.log`.

These checks establish content behavior. In-place mode preservation still
requires a guest metadata interface because SDK fchmod does not implement the
namespace modes. Firmware/WHPX sed checks remain pending. Initial compilation
exposed missing WASI mkstemp/fchown declarations and libbb string_array_len;
platform helpers and the fixed-ownership adaptation resolve those build errors.
The malformed bracket test reaches sed delimiter validation before regex
compilation; separate parser and regex cases now verify both diagnostics.

## Upstream BusyBox grep and anchored mount correction

The upstream 1.37.0 grep replaces the handwritten substring-only applet.
It uses upstream recursive traversal and path helpers, SDK regex, getopt32,
and context support. `scripts/test_busybox_grep.py` passes all 33 Windows
Release native WASM cases in `artifacts/grep-integration.log`. These cover
regex/literal/word/line matching, case folding, inversion, multiple patterns,
empty pattern files, stdin, match counts/limits, context, file names,
recursive directories, long lines, missing input, invalid regex/numbers,
directory read errors, and closed output. Staged upstream corrections check
the zero match limit before input consumption, retain input errors, and flush
and check output before success. These failures were reproduced by integration
cases before the corrections.

Recursive traversal exposed virtual bin/dev/proc/tmp entries being added to
each anchored ordinary directory. Ordinary directory views now use the base
namespace directly. Nested names such as a/tmp remain ordinary directories,
and open views retain identity after ancestor rename. Provider tests also
check symlink-plus-parent traversal, root-to-anchored replacement, EOF beyond
file length, temporary unlink lifetime, and temporary write/resize limits.
The temporary and virtual file reads now return EOF rather than slicing past
the data after an oversized seek. Temporary rename permits replacement.
Full mount architecture, actual proc metadata and shared temporary directories
remain incomplete.

The same final guest image passes the awk and sed suites (45 checks total)
in `artifacts/grep-editor-regression.log`. Firmware/WHPX checks for the new
grep implementation remain pending. Build issues included the missing libbb
exit_SUCCESS helper; the normal successful-exit macro resolves compilation.
One added EOF test initially referenced an undefined options variable; explicit
read-only options correct the test compilation. Rust formatting was applied
after the first format check found a multiline assertion difference.

The runtime lookup/spawn resolver accepts `/bin/NAME` for registered modules,
including already compiled modules and BusyBox aliases. The focused
`program_paths` integration plus the lazy discovery filesystem case pass:
compiled-only custom names, ash/awk aliases, discovered QuickJS/qjs reuse,
and rejection of absent or differently prefixed/cased paths. Evidence:
`artifacts/program-paths-check.log`. This is an execution prefix; real `/bin`
entries, PATH search and shebang resolution remain incomplete.

After the grep/mount changes, the complete `efi-agent-wasm` suite, test-target
Clippy with warnings denied, and formatting checks pass. Evidence:
`artifacts/grep-wasm-regression.log`, `artifacts/grep-clippy-final.log`, and
`artifacts/grep-format-final.log`. These checks do not replace actual firmware
or WHPX verification of the new applets.

## Guest permissions, creation modes and inherited working directories

BusyBox now uses upstream chmod/parse_mode/mode_string. Its libc wrappers and
the private CPython startup adapter expose namespace permissions through
stat/lstat/fstat/fstatat, chmod/fchmod, creation modes and umask. The root
preopen is named `/`, matching the workspace namespace. A process-local
creation-mode scope applies the POSIX mode omitted by Preview 1 during file
or directory creation, including through dangling links. Both clients restore
the previous scope on success and failure; existing files retain their modes.
Children inherit umask and cwd, and do not inherit a temporary creation scope.

`scripts/test_guest_permissions.py` runs unchanged CPython and actual BusyBox
against an isolated namespace. It verifies access denial for new opens,
retained access after chmod/unlink, directory search permission, symlink
following, explicit nofollow errors, fd-relative metadata after rename,
string/bytes/fd/None scandir, retained directory anchors after close/rename,
symbolic and recursive chmod, sed in-place/backup modes, cwd/umask inheritance,
explicit creation modes, dangling-link creation, error-scope restoration,
24 deterministic mode combinations, access bits, and nanosecond stat fields.
The independent namespace reader verifies saved modes/bytes and a second
Python root verifies persisted modes after restart. `-BuildFixtures` on
build_busybox.ps1 also builds the real metadata wrappers with a C integration
entry point; the permission suite checks explicit root fd 3 versus cwd,
ordinary directory fds, fchmod and dangling-link creation. Evidence:
`artifacts/permissions-scandir-integration.log` and
`artifacts/permissions-busybox-fixture-build.log`.

The CPython suite verifies normal cache creation, timestamp/size-based reuse
against deliberately invalid replacement source, and suppression under `-B`.
It retains the interpreter binary and upstream standard library unchanged.
Evidence: `artifacts/permissions-python-final.log`. Runtime ABI cases verify
mode layout/flags/fds, umask, creation-scope restoration, cwd validation,
directory snapshot rights/types, UTF-8 byte lengths, EOF, private descriptor
flags and descriptor reuse. Evidence: `artifacts/permissions-control-scandir.log`.
The final runtime suite includes all seven control cases and passes all 81
WASM tests in `artifacts/permissions-wasm-final.log`. Test-target Clippy with
warnings denied passes in `artifacts/permissions-clippy-final.log`. The final
BusyBox image passes ten descriptor cases, 20 awk cases plus a pipe oracle,
23 sed cases plus cleanup, and 33 grep cases in the corresponding
`artifacts/permissions-{descriptor,awk,sed,grep}-final.log` files. Windows
Release EFI/launcher builds and packaging pass in
`artifacts/permissions-firmware-final-build.log`.

Actual QEMU/OVMF with 512 MiB passes Windows WHPX with Responses and TCG
with Chat Completions on the final build. Both execute model write/shell/Python
tools, nested paths, all write boundaries through an 8 MiB call, finite Python
stdin, rename/unlink with live handles, symlinks, effective read-only modes,
fd scandir after caller close, inherited cwd, upstream symbolic chmod and sed
backup mode preservation, inherited umask, and explicit dangling-link target
creation mode. Independent reads of the saved FAT namespace verify bytes,
deleted names, link targets, result/backup modes, 0600 files, 0700 directories
and the 0400 link-created target. Both also pass verified HTTPS/DNS, cancellation
with child-tree reap, independent background-root preservation, truncation and
subsequent requests. Evidence: `artifacts/permissions-whpx-final.log`,
`artifacts/permissions-whpx-final`, `artifacts/permissions-tcg-final.log`, and
`artifacts/permissions-tcg-final`. The full curl/SSH/application combination
and focused 256 MiB resource gate were not rerun for this permission change.

Implementation and test issues resolved during this change:

- BusyBox symbolic parsing returns type bits as well as permissions. Wrappers
  now pass only permission bits to the metadata interface.
- A root path join produced `//renamed` and capability errors. Root joins and
  preopen slash normalization now agree. The earlier dot-preopen documentation
  was outdated and now describes the slash preopen.
- A persistent C control descriptor could occupy a user fd; C requests now
  open/close their endpoint. Raw WASI endpoint opening avoids reentering libc's
  shared path scratch buffer during chdir/fstatat.
- Explicit fstatat fd 3 was confused with AT_FDCWD. Only AT_FDCWD now adds cwd;
  the real C wrapper fixture distinguishes the two anchors.
- An exclusive-open probe lost explicit modes for dangling symlink targets.
  The runtime creation-mode scope removes that probe and preserves normal
  O_CREAT/O_EXCL semantics.
- Prebuilt CPython lacks scandir(fd). The private adapter now enumerates an
  anchored name snapshot, filters dot entries, and preserves entry lifetimes.
  A bytearray/fsdecode mismatch in its first version was corrected to bytes.
- The cache test omitted `importlib.util` and compared an absolute imported
  cache path with a relative expected path. Explicit import and absolute path
  derivation fix the test; the added reuse oracle checks actual cache behavior.
- An old ash_child fixture lacked the current descriptor command and exited
  99. Rebuilding the fixture restores the descriptor integration.
- The initial directory ABI test imported unsupported fd_fdstat_set_rights.
  It now opens a directory with restricted rights and verifies rejection
  without depending on an unrelated syscall.

Current directory inheritance is still path based. Ancestor rename/unlink
tracking and explicit subprocess cwd/umask are pending. Python scandir entries
are method-compatible private proxies, with a type identity difference from
the built-in DirEntry. Virtual mount completeness, other upstream applets,
filesystem PATH and persistent sessions remain pending. These permission
checks do not establish complete environment delivery.

## 2026-10-23 Python subprocess cwd and umask

The private CPython process adapter now sends relative/absolute `cwd` and an
optional child `umask` through process-control operations. Relative cwd is
resolved against the parent Python cwd. A child inherits the parent umask when
the option is omitted; an explicit umask applies to the child only. The parent
retains its original cwd and umask. Missing cwd reports `ENOENT` before a
child starts. Process-control spawn failures and filesystem failures use the
negative errno response convention expected by Python; the WASI `NOENT`,
`NOTDIR` and `LOOP` values map to their Python errno constants.

All 17 CPython integration tests pass on the native Release `run_guest`,
including relative and absolute cwd, cwd inheritance, absent cwd, umask
inheritance/override, normal imports, finite stdin, descriptor mapping and
large duplex pipes. `cargo test -p efi-agent-wasm --test process_control`
passes all seven cases, and `cargo fmt --all` passes. The native tests verify
the prebuilt Python module; firmware ash cwd behavior is covered by the WHPX
follow-up below.

Follow-up verification built and packaged the Windows Release EFI, then ran
`scripts/smoke_vm.py` with WHPX, 512 MiB RAM, the Responses protocol and the
current ash/Python workspace. The complete filesystem-tool sequence passed,
including inherited cwd/umask, explicit relative and absolute subprocess cwd,
missing-cwd failure, explicit child umask, filesystem mutations, upstream
applets, monotonic waits and 8 MiB writes. Independent FAT checks confirmed
saved file bytes, names, modes and deletions. The run also passed tool
cancellation, background-process isolation, HTTPS streaming and request
cancellation. Output is in `artifacts/cwd-umask-whpx`.

## 2026-10-22 upstream stream applets and output backpressure

The shared BusyBox image now builds upstream 1.37.0 cat, head, tail, wc and
tee. It uses upstream read, copyfd, xatonum, input opening, printable-byte and
numbered-line helpers. The numbered-line and negative-head staging patches
use length-aware input/output to preserve embedded NUL and missing final
newlines. Numbering crosses partial file boundaries. Tail retains complete
long lines instead of assuming a maximum line length, grows its byte buffer
from actual input, and handles unsigned counts without signed overflow.
Read, write and flush failures produce nonzero statuses. Ordinary external
applets use failure status 1; grep explicitly retains its own error status 2.

Windows-native integration passes 112 command cases and independent saved
tee bytes in `artifacts/streams-integration-final.log`. It covers files/stdin,
multiple files, numbering, binary visible output, negative and zero head
counts, tail positions and UINT_MAX counts, an unterminated 1,100,003-byte
line, wc totals/offsets, tee append/multiple destinations, closed descriptors,
directory reads, and a 196,613-byte yes/head pipeline. Twelve deterministic
structured binary inputs each check head, tail and numbering against separate
byte/index oracles. Tests use isolated temporary namespace trees.

Output queues now suspend full writes and resume after the host drains them.
Stream requests above 512 KiB return a normal short write. Every iovec is
validated before any bytes are copied. The private control endpoint retains
its atomic frame limit. `output_backpressure.rs` checks stdout and stderr,
an invalid later vector without effects, the first 524,288-byte transfer,
zero-byte success while full, suspension, and the exact remaining bytes after
drain. All 82 WASM tests and Clippy pass in `artifacts/streams-wasm-tests.log`
and `artifacts/streams-clippy.log`.

The final shared image passes 21 awk checks including its independent pipe
oracle, 24 sed checks including temporary cleanup, 33 grep cases, the guest
permission/C-wrapper suite, all 15 CPython integration cases, and 10 ash
descriptor inheritance cases. Evidence: `artifacts/streams-awk-final.log`,
`streams-sed-final.log`, `streams-grep-final.log`,
`streams-permissions-final.log`, `streams-python-final.log` and
`streams-descriptors-final.log` under artifacts. The Windows Release EFI and
launcher package builds pass in `artifacts/streams-package.log`.

Issues found and resolved during implementation:

- Fast cat output filled the root queue before a scheduler yield and lost the
  final byte of a 524,289-byte file. Backpressure replaces NOSPC rejection.
- Tail's large write hit the old request cap. Bounded copying and normal short
  writes now allow it to complete; its approximate line seek was removed.
- Upstream numbered cat added final newlines and removed NUL bytes. Upstream
  negative-line head also removed NUL bytes. Length-aware staging fixes both.
- Head ignored input errors; tee and tail ignored selected output errors.
  Input/flush/copy error checks now preserve failure status.
- Signed tail count arithmetic could corrupt +UINT_MAX byte positions and
  negative casts could misclassify small streams. Bounded skip arithmetic
  and unsigned comparisons cover both boundaries.
- The first preparation patch used a duplicated source marker. A unique
  surrounding marker restores strict upstream-source validation.
- Upstream numeric parsing changes grep's overflow diagnostic to its precise
  valid range. The test now checks that range and retains the status check.
- A descriptor test initially selected a nonexistent fixture location; it now
  uses the existing `artifacts/wasm-fixtures/ash_child.wasm`.
- Clang reports upstream string-pointer arithmetic warnings in head/tail/wc.
  These are nonfatal. One log-inspection tail invocation was parsed as an
  invalid option; explicit `tail -n` fixed the inspection.

Other upstream commands, complete mounts, filesystem PATH, real monotonic
time, persistent sessions, the full application regression and the 256 MiB
resource gate remain pending. These checks establish only the stated scope.

Actual 512 MiB QEMU/OVMF passes WHPX with Responses and TCG with Chat
Completions on the final shared image. The model calls standard Python
subprocess APIs to verify cat's 524,289-byte binary output and numbered NUL
lines, negative-line head, tail's 1,100,003-byte line and 65,537-byte suffix,
cat/tee/wc above pipe capacity, yes/head, directory reads and closed output.
Independent saved FAT reads verify the binary input, complete long line and
tee copy. Existing mutation/permission/8 MiB file-write checks, verified HTTPS
and DNS, code-tree cancellation, independent background roots and subsequent
requests also pass. Evidence: `artifacts/streams-whpx-final.log`,
`artifacts/streams-whpx-final`, `artifacts/streams-tcg-final.log`, and
`artifacts/streams-tcg-final`.

## 2026-10-22 firmware monotonic time and upstream sleep

The firmware clock retains EFI GetTime for wall time and opens a constant-rate
64-bit hardware counter for elapsed time. It uses UEFI Timestamp when present;
otherwise it uses PI CPU Architectural timer 0 with its reported femtosecond
period. Zero rates, absent timers, failed protocol operations and timestamp
counters below 64 bits return explicit errors. The provider retains its
protocol guard. `/caps` reports the selected source and nominal resolution.
Physical firmware and the Timestamp branch remain unverified.

Counter conversion accumulates unsigned wrapping deltas and retains fractions
before division. Calls must be separated by less than one full counter period.
Unit tests cross u64 wrap, exercise repeated sub-nanosecond samples, reject
zero rates and verify nanosecond overflow without resetting the epoch.
Relative realtime poll subscriptions now use monotonic elapsed time, while
absolute subscriptions retain the requested clock. A test moves realtime
forward/backward independently of monotonic time and checks both deadline
boundaries. All 85 WASM tests pass in
`artifacts/monotonic-wasm-tests-final.log`.

The BusyBox image now uses upstream sleep and duration parsing, with fractional
durations, summed operands, s/m/h/d suffixes and --. Staged checks propagate
nanosleep/sleep errors. The native standard-interface integration verifies
Python monotonic reads and time.sleep, a separate realtime delta, sleep syntax
and failures, ash timed input/timeout, and tail follow/read/append/EOF cleanup.
The tail pipeline consumes two lines, then a third append makes tail observe
EPIPE and terminate so the parent can reap all stages. The shared xwrite
helper now preserves the specific errno diagnostic. Independent host timing
measures between guest markers, excluding compilation and process startup.

The first firmware test found a real clock-domain defect: WASI SDK nanosleep
selects CLOCK_REALTIME. With second-resolution RTC readings, short sleep and
read timeout polls could expire on the same RTC update before the writer ran.
Object disassembly confirms that SDK call in
`artifacts/monotonic-libc-disassembly.log`. Relative waits now use monotonic
time. The corrected WHPX run passes the former failing timed-input case in
`artifacts/monotonic-whpx-corrected.log`. Later follow checks exposed generic
write-error text; xwrite now retains errno, allowing explicit EPIPE validation.

Build issues resolved: the initial duration source marker also matched a
comment and now requires the full line; the shared perror-and-die declaration
was moved to the common header for the xwrite caller. A patch application
initially interpreted Python byte escapes as patch newlines; a literal patch
string corrected that application. QEMU emits a nonfatal Windows Appx manifest
warning and an ignored interrupt-vector warning; the guest still completes
the recorded checks. CPython binaries and standard-library source remain
unchanged. Subprocess timeouts/signals, complete mounts and the remaining
upstream command inventory remain separate pending work.

Final Windows-native evidence: `artifacts/monotonic-native-follow-final.log`
and `artifacts/monotonic-streams-final.log` pass standard time/follow checks,
independent marker timing, and all 112 stream applet cases plus saved bytes.
The existing ash read oracle passes in `artifacts/monotonic-read.log`; all
15 CPython cases pass in `artifacts/monotonic-python-final.log`. WASM and UEFI
Clippy, formatting, and Release packaging pass in
`artifacts/monotonic-wasm-clippy-final.log`,
`artifacts/monotonic-efi-clippy-final.log`, `artifacts/monotonic-format.log`
and `artifacts/monotonic-package-final.log`.

Actual 512 MiB WHPX/Responses and TCG/Chat Completions pass the full current
file/code-tool sequence with the final BusyBox image. Both report PI CPU timer
0 with nominal 1 ns resolution through /caps. They verify Python sleep against
monotonic and independent RTC deltas, upstream sleep operands/errors, delayed
and expired ash reads, tail follow and exact EPIPE diagnostics, saved result
bytes, and cancellation/reap of a sleeping tool subtree while preserving an
independent background root. Existing permissions, stream and 8 MiB write
checks also pass. Evidence: `artifacts/monotonic-whpx-final-all.log`,
`artifacts/monotonic-whpx-final-all`, `artifacts/monotonic-tcg-final-all.log`
and `artifacts/monotonic-tcg-final-all`. The separate full network/application
combination and 256 MiB resource gates remain pending.

## 2026-10-22 upstream basic command applets

The shared image now builds upstream BusyBox 1.37.0 basename, dirname, pwd,
env, printenv, seq, yes, true and false. Upstream path-component, single-argv
and dynamically sized getcwd helpers replace local duplicates. Pwd's -L/-P
parser is enabled locally without changing other applets' DESKTOP selection.
Env invokes the existing cooperative process ABI with the modified environment
and preserves the child's status; launch errors retain 126/127 distinctions.
The process helper accepts clearenv's null environment as an empty list.
The common flush helper returns its result and existing stdio error flags.

Windows-native integration passes 51 semantic/error cases, six yes buffer
boundaries, NUL-terminated env output and seven closed-output cases: 65 cases
plus an independent saved result check. Evidence:
`artifacts/basic-applets-integration-final.log`. Cases cover empty/root/Unicode
paths, suffixes, multiple operands, logical and physical pwd, environment
clearing/unsetting/overrides/empty values, child exit propagation, absent
commands, missing printenv variables, ascending/descending/empty/fractional
and padded sequences, multiple yes words, and builtin/function precedence.
The yes oracle checks 4093/4094/4095/4096/4097/8191-byte words through a
65,537-byte bounded reader, then requires producer exit and pipe cleanup.

Shared-image regressions pass all 112 stream cases plus saved tee bytes,
21 awk checks including its pipe oracle, 24 sed checks including cleanup,
33 grep cases, guest permissions/C wrappers, all 15 Python cases and 10
descriptor inheritance cases. See `artifacts/basic-applets-streams.log`,
`basic-applets-awk.log`, `basic-applets-sed.log`, `basic-applets-grep.log`,
`basic-applets-permissions.log`, `basic-applets-python.log`, and
`basic-applets-descriptors.log` under artifacts. All 85 WASM tests, Clippy,
formatting and Windows Release EFI/launcher packaging pass in
`artifacts/basic-applets-wasm-tests.log`, `artifacts/basic-applets-clippy.log`,
`artifacts/basic-applets-format.log`, and `artifacts/basic-applets-package.log`.

Resolved implementation and verification issues:

- Upstream multiword yes could ignore fputs EPIPE while its final putchar
  succeeded, leaving the producer running after head exited. A staged stdio
  error check ends the loop. WASI has no SIGPIPE delivery, so upstream yes
  prints its Broken pipe diagnostic and exits with failure; pipelines retain
  the last stage's status. Tests check that exact behavior rather than treating
  any stderr as success or suppressing the error.
- clearenv can make environ null. The C argument packer previously dereferenced
  that pointer; it now passes an actual empty environment to the process ABI.
- The first dirname build lacked libgen.h. The shared header now includes it.
- A new nested Python byte literal needed a second level of newline escaping.
  Fixing it removes the firmware-test IndentationError. The VM script now
  compiles every Python tool source before launching QEMU.
- A header patch initially selected the wrong guard name; the include was
  applied to the correct common header. Nonfatal upstream Clang string-pointer
  arithmetic warnings remain as documented in the stream-applet verification.

The wider filesystem mount, executable PATH/manifest, packaging and retained
tool-session requirements remain incomplete.

Actual final 512 MiB QEMU/OVMF checks pass WHPX with Responses and TCG with
Chat Completions. The model drives the complete current command fixture
through Python subprocess, including environment replacement, statuses, the
multiword yes boundary cases and closed outputs. Both verify the persisted
result bytes independently from FAT, along with existing stream, monotonic,
permission/mutation and 8 MiB file-write checks, verified HTTPS/DNS, tool-tree
cancellation and independent background roots. Evidence:
`artifacts/basic-applets-whpx-fixed.log`, `artifacts/basic-applets-whpx-fixed`,
`artifacts/basic-applets-tcg-fixed.log`, and
`artifacts/basic-applets-tcg-fixed`. The wider full application and 256 MiB
resource regressions remain pending.

## 2026-10-22 upstream filesystem mutation applets

The shared image now builds upstream BusyBox 1.37.0 mkdir, rmdir, rm and touch,
with libbb make_directory, remove_file, ask_confirmation and time helpers.
The full upstream FILEUTILS flag layout is retained for subsequent copy/move
ports. Monotonic helper selection uses the actual monotonic clock.

Windows-native integration passes 63 command cases through unchanged CPython
subprocess and real ash. The independent committed-workspace reader verifies
removed names, result bytes, reference-file times and symbolic-link metadata.
Cases cover explicit/symbolic modes, umask and intermediate parents, existing
directory modes, Unicode names, multiple operands after errors, rmdir parents
and nonempty handling, recursive removal with symlinks and permissions,
interactive acceptance/rejection, retained open-file identity, verbose output
errors, touch creation/no-create, epoch/calendar/reference dates, nanosecond
precision, access-only/modify-only changes and dangling-link nofollow behavior.
Evidence: `artifacts/mutation-applets-regression.log`.

Staged upstream corrections propagate directory open/read/close failures,
avoid false verbose success after failed recursive removal, and check futimens
after file creation. The three verbose mutation applets flush/check stdout
before returning status. Mkdir rejects literal / and . without -p at the
applet boundary; its shared helper retains parent-creation semantics.

Resolved verification issues: the saved-metadata oracle initially used field
names with an incorrect _ns suffix, then expected an old atime after a later
ordinary touch had updated both times. Both expectations were corrected;
guest and independent saved checks now agree. Initial searches used absent
fixture/header names; bounded source searches found the correct files.
Upstream time parsing emits four nonfatal Clang string-pointer arithmetic
warnings; these are intentional offsets into format strings. No dependency
or Windows/WSL boundary change was required.

All 85 WASM tests, host/UEFI Clippy, formatting and Windows Release packaging
pass in `artifacts/mutation-applets-checks.log` and
`artifacts/mutation-applets-package.log`. The same final guest image passes
65 basic cases, 112 stream cases, monotonic/time/follow checks, awk/sed/grep,
permission/C wrappers, all 15 CPython cases and 10 descriptor-inheritance
cases. Evidence: `artifacts/mutation-applets-regression.log` and
`artifacts/mutation-applets-shared-regression.log`.

Actual 512 MiB QEMU/OVMF passes WHPX with Responses and TCG with Chat
Completions. Both execute all 63 new cases through model Python subprocess,
check persisted names and times independently from FAT, and pass existing
8 MiB writes, permissions/mutations, stream, clock, HTTPS/DNS, cancellation
and background-isolation checks. Evidence:
`artifacts/mutation-applets-whpx.log`, `artifacts/mutation-applets-whpx`,
`artifacts/mutation-applets-tcg.log`, `artifacts/mutation-applets-tcg`.
The full network/application combination, 256 MiB gate and remaining
command/mount/PATH/session delivery remain pending.

## 2026-10-22 upstream date applet

The shared image replaces the handwritten date subset with upstream BusyBox
1.37.0 date and compare_string_array helpers. Native integration exercises
32 operand/output/error cases, current-time bounds and an independent Python
datetime calendar oracle. The saved-workspace oracle checks reference bytes
and both nanosecond timestamps. Epoch input includes negative seconds; output
includes default UTC, RFC, ISO date/hours/minutes/seconds/nanoseconds, literal
percent and variable nanosecond precision. Reference times, strptime/calendar
input, empty format, invalid operands and closed stdout are covered.

WASI libc silently ignores non-UTC TZ values. The initial JST test exposed
that limit. Date now rejects such values explicitly; -u overrides them with
UTC0. Full timezone support remains pending. Clock-setting attempts report
ENOTSUP and fail, replacing the upstream diagnostic followed by successful
status. Clock-read/localtime failures, incomplete -D conversions, format-buffer
overflow and stdout errors also fail. Evidence:
`artifacts/date-applet-native-final.log`.

The preceding batch's Git whitespace check initially traversed the dirty
Shell submodule for an extended time. The read-only check was stopped and
rerun with --ignore-submodules=all. Only the named project changes were
committed; user workspace assets and the Shell submodule were preserved.
An SDK-header search first used the generic include path; the target-specific
wasm32-wasip1 include directory contains the required declarations.

Final Release packaging passes in `artifacts/date-applet-package.log`.
The final image passes mutation (63), basic (65), stream (112), awk (21),
sed (24) and grep (33) cases in `artifacts/date-applet-shared-regression.log`.
Ash arithmetic/test, structured random expressions and independent random
short-circuit oracles pass in `artifacts/date-applet-math.log`.
The preceding Rust checks remain applicable because this batch changes guest
C and Python verification code only.

Actual 512 MiB WHPX/Responses pass the complete
current file/code-tool sequence with the date fixture combined into the
mutation tool call, keeping the request below its twelve-round limit. The run
independently verifies saved date-reference bytes and nanosecond times from FAT,
alongside existing mutation/permission, 8 MiB write, stream, monotonic wait,
HTTPS/DNS, cancellation and background-root checks. Evidence:
`artifacts/date-applet-whpx.log`, `artifacts/date-applet-whpx`.
Full timezone, remaining commands, mounts, PATH, persistent sessions, the
full network/application regression and the 256 MiB gate remain incomplete.

## 2026-10-23 upstream hexdump applet

The shared ash image now includes upstream BusyBox 1.37.0 hexdump, registered
with the runtime and dispatched through ash. It reuses od's checked dump engine.
The staged source uses the existing xfopen helper and the upstream off_t bound.
Format-file read and close errors now fail instead of accepting a partial format.

Native Release integration verifies canonical and octal output with NUL/high
bytes, exact short-line padding, skip/length, inline and file formats, duplicate
suppression and -v. Independent byte-format oracles cover lengths
0/1/15/16/17/31/32/33/257. Missing files, invalid options/negative bounds,
directory data/format reads and closed stdout fail. Saved hexdump bytes are
checked independently through the native namespace and the WHPX FAT image.
The final 512 MiB WHPX/Responses run executes both basic and text fixtures,
including the od saved-byte assertions. WASM crate tests, Clippy with warnings
denied, formatting, guest build and Windows Release EFI/launcher packaging pass.
Evidence: `artifacts/hexdump-native.log`, `artifacts/hexdump-build.log`,
`artifacts/hexdump-rust-tests.log`, `artifacts/hexdump-whpx.log` and
`artifacts/hexdump-whpx`.

Resolved verification issues: Windows Git avoided the slow WSL-mounted status
scan; protected PowerShell quoting prevented Bash expansion of variables.
The native runner was rebuilt to include the new command registration.
Independent expected bytes were corrected for hexdump's default word format
and octal-column padding. Packaging used explicit smoke configuration and the
existing built Shell EFI. The firmware test received the OVMF CODE image after
an incorrect EFI argument caused its first boot to reset the connection.
Existing upstream string-pointer arithmetic warnings remain nonfatal.

## 2026-10-23 upstream base64 applet

The shared ash image now includes upstream BusyBox 1.37.0 base64 encoding and
decoding. It uses BusyBox's shared `uuencode.c` implementation. Decoding follows
upstream behavior and skips non-alphabet characters. The staged adapter checks
source read errors in both modes and detects failed decode writes; encoding
checks buffered output errors. Pipeline status remains the final ash stage's
status when base64 reports EPIPE.

Native Release integration passes 198 checks through real Python subprocesses
and ash. It includes RFC 4648 vectors, Python `base64` byte oracles, wrap widths
1/3/4/5/76/77/2147483647, binary records, malformed/truncated input, invalid
arguments, missing files, directories, closed descriptors, and 65535/65536/
65537 plus 524287/524288/524289 byte boundaries. File encoding/decoding,
base64-to-head broken pipe, and a base64/decoder/tee pipeline have independent
saved-byte checks. The final 512 MiB WHPX/Responses run passes basic and base64
fixtures and verifies saved encoded/decoded bytes directly from FAT.
The WASI guest build, Rust WASM tests, native integration, Python syntax,
Windows Release EFI/launcher packaging and format/Clippy checks pass.
Evidence: `artifacts/base64-build.log`, `artifacts/base64-native.log`,
`artifacts/base64-package.log`, `artifacts/base64-whpx.log` and
`artifacts/base64-whpx`.

Resolved verification issues: the initial pipeline assertion expected the
producer's status, but ash correctly returns the final `head` status while
base64 reports its broken output. The assertion now verifies both status and
diagnostic. PowerShell 7 with `-CommandWithArgs` and a Windows-local working
directory was used for native tools. Earlier nonfatal BusyBox pointer-arithmetic
warnings remain.

## 2026-10-23 upstream sha256sum applet

The shared ash image now registers upstream BusyBox 1.37.0 `sha256sum` and
enables its checksum-file parser. Hashing and check-line interpretation remain
upstream. The WASI support interface defines the SHA context layouts, native
little-endian swaps and unaligned access required by the hash translation unit;
the shared guest support adds BusyBox-compatible lowercase `bin2hex`. The
staged applet reports input close and buffered stdout failures.

Native Release integration passes independent hashlib vectors and binary file
checks, with lengths 0/1/2/3/4/5/6, SHA padding and block boundaries
55/56/63/64/65, I/O boundaries 4095/4096/4097 and 65535/65536/65537,
524287/524288/524289, and 1 MiB. It verifies multiple files, `-c`, silent `-cs`,
bad and malformed checksum files, invalid options, missing files, directories,
and closed stdout. Independent saved checksums and source files are read from
the native namespace. Actual 512 MiB WHPX/Responses runs both basic and SHA-256
fixtures and independently checks saved checksum and binary files from FAT.
The explicit WASI SDK/Binaryen guest build, Windows Release EFI/launcher
packaging, WASM crate tests, Clippy with warnings denied, format and Python
syntax checks pass. Evidence: `artifacts/sha256-build.log`,
`artifacts/sha256-native.log`, `artifacts/sha256-package.log`,
`artifacts/sha256-rust-tests.log`, `artifacts/sha256-whpx.log` and
`artifacts/sha256-whpx`.

Resolved verification issues: initially including BusyBox's complete internal
header pulled unavailable WASI networking and terminal headers. The hash build
now uses a small explicit support interface for the required context layouts,
little-endian operations and unaligned accesses. The first 8 MiB stdin hashing case exceeded the
native runner's 60-second guest limit. The useful syscall boundaries through
524289 bytes and a 1 MiB throughput check pass; the 8 MiB hash case was removed
because it is outside the storage-write requirement. Upstream BusyBox date and
time sources still emit existing non-fatal string-offset warnings.

## 2026-10-23 upstream realpath applet

The shared ash image registers upstream BusyBox 1.37.0 `realpath`. The applet
uses the WASI libc resolver for existing paths. Its platform helper retains the
upstream behavior for a missing final component and dangling final symlink by
canonicalizing the parent and resolving the link target.

Native Release integration passes paths containing `.` and `..`, absolute and
relative names, directory symlinks, multiple operands, dangling symlinks,
missing final names, missing parents, trailing slash errors and argument errors.
Expected canonical paths are independently checked with Python `os.path`
resolution. Actual 512 MiB WHPX/Responses runs the basic applet and realpath
fixtures, then reads the result and saved symlink target directly from FAT.
The Windows Release EFI/launcher package, WASI guest build, WASM tests, Clippy
with warnings denied, format and Python syntax checks pass. Evidence:
`artifacts/realpath-build.log`, `artifacts/realpath-native.log`,
`artifacts/realpath-package.log`, `artifacts/realpath-rust-tests.log`,
`artifacts/realpath-whpx.log` and `artifacts/realpath-whpx`.

Resolved verification issue: the upstream realpath applet has no option parser;
`-Q` is a pathname, not an invalid option. The test now checks its documented
operand behavior and uses a no-operand call for the usage error.

## 2026-10-23 upstream od applet

The shared BusyBox ash image now includes upstream 1.37.0 `od` and its dump
engine with `DESKTOP` disabled. It supports the upstream short-format options
such as `-b`, `-c`, `-x` and `-v`; `-A` and `-t` are unavailable. Offset operands
precede filenames. Native Release `run_guest` checks binary octal and character
output for NUL and high bytes, default 16-bit output, offset input, concatenated
files, duplicate suppression and `-v`. Independent byte oracles cover lengths
0/1/15/16/17/31/32/33/257. Missing files, invalid options, directory reads and
closed stdout return failure. The staged dump engine preserves read failure
status and detects buffered stdout failures. The focused
512 MiB Windows WHPX/Responses run passes both basic and text applet fixtures;
it independently compares both saved od outputs from the FAT image.
Evidence: `artifacts/od-native.log`, `artifacts/od-build.log`,
`artifacts/od-whpx`.

## 2026-10-22 upstream sort, uniq, cut and tr

The shared image now builds upstream BusyBox 1.37.0 sort, uniq, cut and tr,
plus its vector-growth helper. Sort enables BIG and version comparison;
memory-sharing optimization is disabled. Cut enables regex fields; tr enables
classes and equivalence sets. New bounded record helpers adapt upstream
algorithms to byte lengths, original record positions and decimal prefixes.

Native integration passes 142 checks through actual Python subprocess and
ash, plus independent committed-workspace bytes. Fixed cases cover sort
numeric/general/human/month/version modes, multiple keys, field-relative
character offsets and blanks, stable/reverse/unique order, checks and errors;
uniq counts/filters/fields/characters/width/case; cut ranges/order/fields/regex
and suppression; and tr translation/deletion/complement/squeeze/classes.
Binary and NUL-delimited records include empty, NUL and high-byte values.
Structured random oracles compare sort against Python byte ordering and exact
Decimal values, stable key sorting against independent integer keys, uniq
against groupby and cut against independently selected positions. Decimal
boundaries cross signs, zero, long fractions, adjacent 64-bit integers and
powers of ten up to 40 digits. Stream lengths cross 65535/65536/65537 and
196613 bytes. Same-file sort output, file uniq output, error continuation and
the complete cat/sort/uniq/tr/cut chain have independent saved-byte checks.
Evidence: `artifacts/text-applets-integration-final-all.log`. The focused
WHPX path uses `scripts/smoke_vm.py --text-applets` with `--code-workspace`;
it runs only this fixture and checks the saved FAT bytes in
`artifacts/text-applets-whpx`.

Corrections and resolved issues:

- Upstream C-string lengths truncated embedded NUL records. Length-bearing
  sort/uniq records and length-aware cut output retain all bytes. Uniq -z
  now selects its input delimiter as well as output delimiter.
- Sort stable positions previously followed the first NUL. They now belong
  to each record; comparator self-comparison returns zero. Sort -n avoids
  floating-point rounding and handles decimal prefixes without exponents;
  -g retains floating-point semantics. Human sorting orders negative suffixes
  and zero correctly. Invalid month input no longer reads an uninitialized tm.
- Key end-character offsets were absolute; they now use the selected field.
  Explicit end-blank handling precedes its offset. Large positions are checked
  before addition and key fields stop at the actual end of input.
- Uniq large skip counts now stop at the end of a record. Cut rejects zero
  and reversed ranges, multibyte field separators, regex NUL input and empty
  regex matches. Empty-line suppression and regex errors are handled before
  output. Sort/cut report read/close failures rather than accepting EOF.
- The first build lacked xopen. The shared helper now opens with mode 0666
  through the existing permission wrapper and reports failure. One test
  incorrectly assumed global -r overrode a key-local n option; a host sort
  comparison established the option scope, and the test now requests explicit
  key-local reverse order and stable duplicate selection.
- Existing nonfatal upstream time.c format-string offset warnings remain.
  Regex cut is byte-oriented and rejects embedded NUL explicitly; full
  timezone and the wider delivery gates remain pending.

## 2026-10-23 upstream BusyBox stat applet

BusyBox 1.37.0 `stat` now uses the upstream file-status parser and formatter in
the shared ash image. The guest supplies the namespace's fixed uid/gid name
lookup and bounded symlink-reading helper. File status, custom formats,
terse output, `-L`, and default link-target display are enabled. Filesystem
status (`-f`) is disabled because the namespace has no capacity-statistics
provider; it reports the upstream usage error.

Native Release integration through real ash and unchanged CPython passes
independent Python metadata comparisons for mode, type, size, ownership,
link count and timestamps. It also checks default human-readable file/link
output, followed and unfollowed symlinks, terse fields, unknown options,
missing paths and missing arguments. The independent saved workspace checks
the output bytes, source file bytes, modes, link targets and timestamps.
Epoch-format tests cross 2147483647/2147483648 seconds and cover 4294967296
seconds. The staged port uses 64-bit conversions because WASI has 64-bit
time_t and 32-bit long. Buffered stdout failures now return failure.
Evidence: `artifacts/stat-native.log`, `artifacts/stat-windows-native.log`,
`artifacts/stat-windows-runner-build.log` and `artifacts/stat-build.log`.

Windows Release EFI packaging and actual 512 MiB QEMU/OVMF WHPX tool flows
pass under both Responses and Chat Completions. Each run independently checks
the final FAT workspace metadata and bytes. Evidence is in
`artifacts/stat-whpx-responses-final` and
`artifacts/stat-whpx-chat_completions-final`. WASM crate tests, Clippy with
warnings denied, Rust formatting, Python syntax and whitespace checks pass.

The initial Windows packaging attempt lacked `EFI_AGENT_API_BASE`; packaging
then succeeded with local smoke-only configuration values. The first applet
build also exposed missing port declarations for passwd/group lookup and
symlink reading; those interfaces now match the namespace's fixed identity
and upstream stat behavior.

## 2026-10-23 upstream BusyBox ls applet

BusyBox 1.37.0 `ls` now uses the upstream parser, sorting and directory
formatting implementation in the shared image. Real ash and CPython integration
covers ordinary and hidden entries, `-A`, `-a`, `-d`, `-F`, `-p`, recursive
traversal, long format, missing paths and invalid options. It also checks
symbolic-link display and following, dangling links, reverse/size/time sorting,
quoting, inode/block columns, Unicode names, `-w`, and narrow/wide `COLUMNS`
layouts. The WASI terminal-width adapter uses valid `COLUMNS` values and
otherwise the 80-column default. Independent native namespace checks verify
the saved listing, file bytes and symbolic-link targets.

Windows Release packaging and the 512 MiB WHPX/Responses fixture pass with
independent saved FAT checks in `artifacts/ls-applet-whpx-final-columns`. Python
syntax checks pass. The first packaging attempt had no `EFI_AGENT_API_BASE`;
the local test provider settings resolved that configuration requirement.
Enabling `FEATURE_LS_WIDTH` exposed the missing terminal-width port declaration,
which is now implemented. Two smoke harness branches initially routed
`--ls-applet` into unrelated tool and text-applet checks; both now select the
ls-only path. The first WSL Git status query stalled while inspecting a dirty
submodule, so the final status check disabled submodule inspection. The
upstream string-offset warnings in head, tail, wc and time.c remain nonfatal.

## 2026-10-24 upstream BusyBox find applet

BusyBox 1.37.0 `find` now builds into the shared image and resolves as a
multicall command. Native Release integration exercises recursive name/type/
size/empty tests, `-maxdepth`, `-prune`, NUL output, `-delete` and missing-root
errors. Independent namespace readback confirms deleted files are absent,
remaining bytes are unchanged, and the symlink entry retains its target.
Optional `-exec`, SELinux context and user/group name matching are disabled
because the runtime has no matching execution, SELinux or account provider.

The Windows WASI SDK 34/Binaryen 133 guest build and native Release runner pass.
The dedicated `scripts/smoke_vm.py --find-applet` flow passes with a 512 MiB
WHPX/Responses guest in `artifacts/find-applet-whpx-20261024-final`. Independent
saved FAT checks confirm both `.txt` files were deleted, the empty file and
`c.bin` contents remain unchanged, and the symlink still targets `a.txt`.
The fixture checks actual NUL bytes from `-print0`. `-exec`, SELinux context
tests and user/group name matching remain unavailable because the runtime has
no execution, SELinux or account provider. The existing upstream string-offset
warnings in head, tail, wc and time.c remain nonfatal.
