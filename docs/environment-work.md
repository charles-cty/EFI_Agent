# Command and filesystem environment delivery

Status: complete for the reported A1-A8/B9-B15 environment requirements.
Acceptance evidence and limits are recorded below. Completion is based on
native integration, both Windows WHPX API formats, independent saved FAT
checks and the complete 512 MiB application combination.

The retained-cwd metadata correction restores BusyBox `find .` traversal and
`tar -C DIR ... .` archives. Shared C stat/lstat/fstatat routing preserves
original path components through explicit directory descriptors. Native C and
tar integration pass. Both 512 MiB WHPX API flows pass C application checks
and independent saved FAT bytes in artifacts/cwd-stat-whpx-final-responses and
artifacts/cwd-stat-whpx-final-chat_completions. The full
WHPX application harness now uses logical module paths and independently reads
saved namespace generations. Its Windows refusal peer sent TCP RST rather than
the timeout expected by the old fixture. Firmware reports connection reset
(WASI errno 15), rather than refusal (14); the harness expects that reset.
The final network/application combination passes in
artifacts/environment-applications-whpx-idle.

Installation manifests now declare module sources, shared command aliases,
libraries and licenses in root programs.json. Discovery validates installation
metadata before transactional alias publication. Binary bodies, supported
imports and command exports are checked when the command is requested.
Reload removes obsolete aliases and preserves explicit registrations. Standard
packaging includes the declared assets without build caches or duplicate alias
images. VM launch merges the packaged installation with the selected workspace,
and content conflicts reject before staging. Native installation, actual FAT
pack/extract and saved-asset checks pass. BusyBox non-UTC timezone support now
passes native integration and both 512 MiB WHPX model APIs. All declared
library files and licenses match independent saved FAT bytes in
artifacts/environment-final-assets-responses and
artifacts/environment-final-assets-chat_completions. The final combined
application gate passes in artifacts/environment-applications-whpx-idle.
Both actual 512 MiB WHPX API flows pass manifest-installed applications and
independent saved FAT assets/modes in artifacts/installation-final-whpx-responses
and artifacts/installation-final-whpx-chat_completions.

Conversation-owned live ash and Python sessions retain interpreter state,
environment and cwd anchors over a bounded private request channel. Command
stdin remains finite and empty. Ordinary Python exceptions and shell nonzero
results retain state. Exit, fatal shell syntax errors and cancellation report
resets. Top-level shell return stops only its current request. Clear resets both
interpreters, and each completed request stops/reaps unfinished descendants
without affecting unrelated roots. Native lifecycle/private-descriptor checks
and both actual 512 MiB WHPX APIs pass, including independent saved FAT bytes,
in artifacts/sessions-final-whpx-responses and
artifacts/sessions-final-whpx-chat_completions.

The shared image currently uses upstream ls, find, awk, sed, grep, chmod, cat, head,
tail, wc, tee, sleep, basename, dirname, pwd, env, printenv, seq, yes, true,
false, od, hexdump, base64, sha256sum and realpath, plus mkdir, rmdir, rm,
touch, date, sort, uniq, cut, tr, cp, mv, which, xargs, gzip, gunzip, zcat, unzip, du, ln, df
and tar. All 48 required commands have native and actual WHPX evidence; the
complete environment still requires the delivery gates below.
Upstream `stat` now reports file and symlink metadata, ownership, permissions,
timestamps and custom/terse formats. Filesystem mode (`stat -f`) now uses the
same capacity provider through a guest C statfs adapter. Standard statvfs
interfaces now use measured native and firmware capacity and runtime mounts.
BusyBox date now uses the upstream musl timezone engine, packaged IANA 2026e
data and POSIX TZ strings. Native checks include DST boundaries, historical
second offsets, 2038/2050 dates, all packaged TZif files and malformed inputs.
Both 512 MiB WHPX API formats pass the full expanded filesystem/date sequence
in artifacts/environment-final-filesystem-responses and
artifacts/environment-final-filesystem-chat_completions.
Firmware monotonic waits now use a reported hardware-counter rate. General mount
traversal, installation packaging and retained sessions have focused native and
both WHPX API evidence. The final combined delivery audit is recorded below.
Python's standard `os.remove`, `os.unlink` and `os.rmdir` now use the same
namespace mutation providers as WASI and BusyBox. File and directory deletion
therefore persists through the workspace namespace and passes the filesystem
tool WHPX sequence.
The private subprocess adapter also passes cwd and per-child umask through the
process ABI. Native Release integration verifies parent cwd/umask inheritance,
relative and absolute child cwd, missing-cwd errors and explicit child umask.
The full CPython suite passes 17 cases. Windows Release packaging and the
512 MiB WHPX/Responses filesystem-tool sequence pass, including inherited and
explicit subprocess cwd/umask behavior and independent saved FAT checks.
The same WHPX sequence also verifies automatic import cache creation/reuse,
py_compile, compileall and `-B` suppression with independent saved FAT checks.

Executable discovery now publishes installed commands and shared BusyBox aliases
in `/bin`, including upstream `which` and the `sh` alias. Upstream ash PATH
lookup retains builtin/function precedence and checks owner execute permission.
Runtime subprocess and env dispatch support PATH, explicit executable WASM paths
and bounded absolute-interpreter shebangs. Native Release and 512 MiB
WHPX/Responses checks pass PATH ordering, empty components, explicit child cwd,
nonexecutable-file skipping, Python shebang options and independent saved script
bytes/modes in `artifacts/program-paths-whpx-20261024`. Installation rejects
workspace command collisions before publishing aliases and preserves unrelated
workspace `/bin` entries. The manifest, mount/link and retained-session checks
below extend this coverage.

## Required commands

Build the upstream BusyBox implementations of ls, cat, cp, mv, rm, mkdir,
rmdir, touch, pwd, head, tail, wc, grep, sed, awk, sort, uniq, cut, tr,
tee, find, xargs, basename, dirname, realpath, stat, du, df, date, sleep, env,
printenv, seq, yes, true, false, which, od, hexdump, base64, sha256sum, tar,
gzip, gunzip, unzip, zcat, ln and chmod. Keep the existing ash builtins.
Each external invocation gets a fresh process, with one shared compiled
multicall module. Preserve upstream parsing and command algorithms.

Cp and mv now use upstream parsers and shared copy/stat/inode helpers. Windows
native integration and both 512 MiB WHPX model APIs pass 50 copy/move cases,
with independent saved FAT bytes, links, modes and timestamps. Same-file aliases
are checked before truncation; traversal and preservation failures report errors.
Long options, symbolic links, explicit hard links and automatic hard-link
preservation are enabled. Special node creation and reflinks remain unavailable;
the reflink feature is disabled. Cross-mount move
fallback remains unverified until mount support is complete.

Xargs now preserves argument vectors through cooperative serial/parallel
children. Native integration and both 512 MiB WHPX model APIs verify quotes,
empty and NUL records, replacements, argument files, batching and -x, child
statuses, concurrent progress and independent saved completion files. Upstream
echo is exposed for the default command. Interactive -p/-o require the absent
/dev/tty and fail explicitly; the shared 32-process quota bounds concurrency.
Gzip, gunzip and zcat now use upstream compression/decompression and CRC32
helpers. The build enables compression levels and long options; levels 1-3 use
upstream's level-4 implementation. Zcat handles gzip streams directly; seamless
auto-detection of other compression formats is disabled. Native integration
uses independent Python gzip/zlib vectors for stored, fixed and dynamic blocks,
concatenated members, header extensions and checksums, binary stream boundaries,
file replacement/deletion, permissions, times and errors. Host Python separately
decodes the saved compressed files. Independent applets now enter the shared
Asyncify jump driver so decompressor recovery does not trap.
The same checks and independent saved FAT decoding pass both 512 MiB WHPX
model APIs in artifacts/compression-whpx-responses and
artifacts/compression-whpx-chat_completions.
Unzip now uses upstream ZIP32 central-directory parsing with stored and DEFLATE
members, listing/testing/filtering, overwrite policies and stored/deflated
symlinks. Extraction syscalls use a retained destination directory anchor;
outside ancestor links fail before writes. Modes and DOS timestamps are
restored, with directory attributes deferred until children are complete.
Stored CRC, decompressed lengths, member names, local/central signatures and
end-record bounds are checked. Native integration and both 512 MiB WHPX model
APIs pass, with independent saved FAT bytes, modes, timestamps and containment
checks in artifacts/unzip-whpx-responses and unzip-whpx-chat_completions.
ZIP64, encryption and other
compression methods remain unsupported.
Du uses upstream traversal, option parsing and inode deduplication with actual
allocated data bytes. Windows and EFI provider measurements survive namespace
lookup and open-file unlink; C and Python stat methods report those blocks.
Logical directories and symlinks have no independently allocated data blocks;
shared journal storage belongs to filesystem capacity accounting. Native
integration verifies sparse and ordinary files against independent Windows
queries. Both 512 MiB WHPX APIs verify bytes and saved FAT cluster chains in
artifacts/du-whpx-responses and du-whpx-chat_completions. Hard-link counting
uses shared inode identities; cross-mount -x behavior still needs mount gates.
Ln and Python os.link now share namespace inodes, data, permissions and times.
Namespace rename/unlink, last-close reclamation, symlink hard links and anchored
directory views retain identity. Launcher staging/export/save/reload preserves
groups and detects host link-group changes before effects. Native integration
verifies cp -l/-a, du deduplication, descriptor-relative Python links, backups,
source protection and independent saved groups. Both actual 512 MiB WHPX model
APIs pass with independent saved FAT inode/backing/byte/mode checks in
artifacts/ln-whpx-responses and artifacts/ln-whpx-chat_completions.
Df uses upstream mount selection and formatting with measured workspace bytes,
distinct free/available values, namespace slot counts and virtual mount IDs.
Python statvfs/fstatvfs use the private adapter with the unchanged interpreter.
The shared /tmp quota is 32 MiB and 4096 non-root inode/name slots; detached
open data and inodes remain charged until their last close. Native command
checks, quota boundaries and independent host volume queries pass. Both actual
512 MiB WHPX model API flows
pass with independent saved FAT capacity and byte checks in
artifacts/df-whpx-responses and artifacts/df-whpx-chat_completions.
Tar now uses upstream creation, member parsing, listing, extraction and filters.
Gzip compression/decompression use cooperative child processes with explicit fd
maps; names and links resolve below the anchored extraction directory. Directory
attributes are restored after their children. Native integration checks saved
archives, metadata, hard links, binary/pipe boundaries and path containment.
Both actual 512 MiB WHPX model API flows pass with independent saved archive,
byte, link and metadata checks in artifacts/tar-whpx-responses and
artifacts/tar-whpx-chat_completions. The final native tar regression also passes
after the common C cwd metadata correction.

The /tmp provider now uses the common inode namespace over bounded memory
objects. Nested directories, relative symlinks, hard links, enforced modes,
timestamps, replacement/unlink lifetime and anchored directory descriptors
share the workspace algorithms without writing a journal. Ordinary temporary
names do not reserve the workspace's private metadata paths. Directory listings
include dot entries in addition to the full ordinary-name quota. Native Python
and ash verify tempfile cleanup, child cwd, cross-device mv fallback and archive
extraction. Both actual 512 MiB WHPX model APIs pass with independent saved FAT
bytes/metadata and temporary mount exclusion in
artifacts/tmp-inodes-whpx-responses and artifacts/tmp-inodes-whpx-chat_completions.
The device/process, anchored cwd and retained-session checks below extend this
coverage.

The virtual root now resolves absolute and relative links across runtime mounts
before provider dispatch. Link/parent ordering, stat/lstat policy, mutations,
directory enumeration, capacity and executable lookup use the same walk. A root
directory descriptor retains mounts; non-root directory capabilities retain
their bounds after rename. One 40-link budget covers all mounts; expanded paths
accept the exact 4096-byte limit. Python metadata and child cwd retain link/parent
components instead of folding them lexically. Links to installed commands reuse
the compiled module, including compiled-only registrations. Native Release and
both actual 512 MiB WHPX model API flows pass with independent saved FAT bytes,
modes and link targets in artifacts/mount-walk-whpx-responses and
artifacts/mount-walk-whpx-chat_completions.
Workspace directory fds now retain that routing layer over their inode anchors.
Rename and hard-link mutations work in both directions with the virtual root,
including after directory rename. Local tmp/dev/proc/bin names remain ordinary
directories and real cross-device operations fail before effects. Native
regressions and both actual 512 MiB WHPX model API flows pass, including
independent saved FAT hard-link groups and bytes in
artifacts/anchored-mount-whpx-responses and
artifacts/anchored-mount-whpx-chat_completions.

Mount initialization rejects existing workspace dev/tmp/proc entries before
publishing a root, including empty directories and dangling links. Errors include
the conflicting path. A real workspace bin directory is merged; non-directory
bin entries and registered-command collisions reject installation. Failed root
replacement retains the previous root and its temporary storage. Case variants
and sibling prefixes remain ordinary entries. Firmware file tools and module
loaders now obtain the same mounted root as shell and Python. Missing dev/proc
paths cannot fall through to backing storage. Native conflict checks preserve
saved bytes and names. Both actual 512 MiB WHPX model API flows verify model
write/edit/read in /tmp, shell/Python access and /wasm load from the temporary
mount, with independent saved FAT checks in artifacts/mount-layout-whpx-responses
and artifacts/mount-layout-whpx-chat_completions. The device/process, anchored
cwd, manifest and session checks below extend this coverage.

Character devices now provide null EOF, zero bytes and shared cryptographic
random/urandom reads with bounded staging. Character filestat, distinct inode
identities, discard writes, seek behavior, explicit sync/truncate errors,
non-terminal Python stdin and shared descriptor flags pass native integration.
BusyBox df uses the containing mount for these devices and links to them; its
Linux UBI special case is omitted because WASI has no st_rdev or UBI volumes.
Both actual 512 MiB WHPX model API flows pass with independent saved zero bytes,
random sample digests and device mount exclusion in
artifacts/character-devices-whpx-responses-final and
artifacts/character-devices-whpx-chat_completions-final. Current native process
and mount regressions also pass.

Process files now expose actual identity, parent/state, startup arguments and
environment, cwd/exe links, fd links and mounts. /proc/self follows each caller,
and zombie records lose their descriptors before reaping removes their names.
The runtime monotonic clock supplies elapsed and scheduler idle time through
/proc/uptime. /dev/fd and standard-descriptor aliases use weak table views and
fdescfs duplication semantics. Directory aliases route through retained anchors
after rename, including mutations and cross-directory rename/hard-link calls;
one link budget covers the full walk. Native integration and independent saved
state checks pass. Both actual 512 MiB WHPX model API flows and independent FAT
checks pass in artifacts/procfs-release-whpx-responses and
artifacts/procfs-release-whpx-chat_completions. Host/target checks and Release
packaging pass. The final combined delivery audit is recorded below.

Processes now own cwd anchors and inherit them directly. Python and BusyBox
initialize libc's relative-path routing through /proc/self/cwd, while standard
getcwd queries return physical paths. Rename, replacement and old-name reuse
cannot redirect the anchor. Unlinked cwd queries return ENOENT; child inheritance
and actual-parent navigation remain available. Other directory fds stay bounded.
Weak proc views retain no cwd after process termination. Native integration
checks ordinary open/FileIO/import caches, ash, relative scripts, permission and
umask boundaries, 40/41 links and long physical paths. Both actual 512 MiB WHPX
API flows and independent saved FAT checks pass in
artifacts/cwd-anchor-release-whpx-responses and
artifacts/cwd-anchor-release-whpx-chat_completions. BusyBox, curl, QuickJS and
WolfSSH now share the C cwd constructor and standard wrappers. Native C checks
cover long paths, deleted cwd and inherited search permissions. Actual curl
relative configuration/download/upload files, WolfSSH relative password
authentication and QuickJS shell inheritance pass with independent saved bytes
in artifacts/c-port-cwd-native.log. Full application staging now selects a
64/128 MiB ESP and allows a 64 MiB boot tree; the workspace limit remains 48 MiB.
Both actual 512 MiB WHPX API flows and independent saved FAT checks pass in
artifacts/c-port-cwd-whpx-responses and artifacts/c-port-cwd-whpx-chat_completions.
Executable lookup now
canonicalizes cwd/descriptor routes before selecting cached modules or compiled
command aliases. File-backed cache validation now includes inode/device identity,
size and namespace content revisions, so same-size edits with restored times,
replacement, truncate and unlink cannot select stale code. Unchanged aliases
share the current compiled module. Live Stores retain their loaded image;
explicit byte registrations remain snapshots. Native and real Python checks
pass in artifacts/executable-revision-host-tests.log and
artifacts/executable-revision-native.log. Manifest evidence is recorded above.
Both actual 512 MiB WHPX API flows pass the same live-image and independent
saved FAT checks in artifacts/executable-revision-whpx-responses and
artifacts/executable-revision-whpx-chat_completions.

## Delivery requirements

- One namespace shared by shell, Python, C, model files and module resolution.
  Workspace is `/`; mount `/bin`, `/dev`, `/tmp` and read-only `/proc` without
  hiding colliding workspace entries. Provide null, zero, random and fd devices,
  bounded shared temporary storage, and actual process information.
- Directory creation, unlink/rmdir, rename replacement, symlink/readlink,
  timestamps, capacity and enforced single-user POSIX modes. Preserve open file
  identity through unlink/replacement and directory rename. Use protected,
  versioned FAT metadata with recoverable updates and deferred reclamation.
- Relative and absolute link traversal, stat/lstat differences, dangling links,
  link loops, mount boundaries and archive destination containment.
- Writes create missing parent directories and report specific operation/path
  errors. File transfer uses bounded buffers, validated iovecs and normal short
  transfer semantics. Calls above 512 KiB, including 8 MiB, must work.
- Finite stdin is not a terminal. Unchanged prebuilt CPython supports `python -`
  with files, pipes, here-documents and empty stdin. Enable normal import caches
  when firmware mutation support is complete. Keep private Python adaptation;
  retain the upstream interpreter binary and standard library unchanged.
- A verified cooperative monotonic clock for sleep and timed operations.
- Executable entries in `/bin`, consistent PATH, command -v/type/which,
  subprocess/env/xargs, absolute and relative executable paths, and shebangs.
  A validated installation manifest supports lazy modules and shared aliases.
  Prepared curl, QuickJS/qjs and WolfSSH work without manual registration.
  Additional supplied node/git modules follow the same rules; advertise them
  only when working binaries are installed.
- Conversation-owned live shell and Python sessions retain interpreter state,
  environment and cwd. Use bounded private framing separate from finite command
  stdin. Preserve readable captured results and one collapsed tool disclosure.
  Clear resets sessions; exit, failure and cancellation report any state reset,
  reap abandoned children, and preserve unrelated background roots.
- Packaging and save preserve additions, modifications, deletions, empty
  directories, links, modes and times. Detect host conflicts before changes,
  retain recovery disks on failure, and preserve Windows-safe virtual metadata.
  Include program manifests, libraries and licenses without duplicating modules.

## Verification gates

Use meaningful normal/error cases for all commands. Check transfer boundaries
65535/65536/65537 and 524287/524288/524289, binary bytes, 8 MiB calls, invalid
later iovecs before effects, partial writes, and open descriptors during mutation.
Check pipelines above capacity, yes/head, text transformations, archive and
compression round trips, extraction containment, modes/times and interrupted
saves. Check consecutive sessions, failures, cancellation and reset.

Run the Windows-native guest and independent saved FAT checks through WHPX at
512 MiB. Run existing process, descriptor, filesystem, ash, Python and
networking regressions, host/target Clippy, format and whitespace checks. Record
actual evidence and failures in verification.md. No stub, registered name or
unverified provider operation satisfies these requirements.

## Reported-issue acceptance evidence

The current native regressions use the rebuilt shared image. The final
filesystem and asset runs below use 512 MiB Windows WHPX under both API formats.
The complete application combination also passes in
artifacts/environment-applications-whpx-idle.

| Issue | Verified behavior | Evidence |
| --- | --- | --- |
| A1 | Nested directories work through Python, ash and model file tools. | environment-final-filesystem-*; environment-complete-native-mount_paths.log |
| A2 | File/directory deletion persists; open descriptors retain their inode. | environment-final-filesystem-*; timezone-host-tests.log namespace/open-lifetime cases |
| A3 | Rename, replacement and links retain path/anchor semantics. | cwd-stat-whpx-final-*; mount-walk-whpx-*; environment-complete-native-cwd.log |
| A4 | Modes affect metadata and access, survive save and inherit umask. | environment-final-filesystem-*; permissions-whpx-final; current native mutation/copy/tar suites |
| A5 | Real device, temporary and process mounts share the namespace. | character-devices-whpx-*-final; procfs-release-whpx-*; tmp-inodes-whpx-*; current native proc/mount suites |
| A6 | 8 MiB writes, short transfers and complete iovec validation work. | environment-final-filesystem-*; timezone-host-tests.log file_writes_accept_eight_mib_and_reject_a_late_invalid_vector_without_effects and output-backpressure cases |
| A7 | Imports, py_compile and compileall write/reuse valid caches. | environment-final-filesystem-*; environment-complete-native-cpython.log |
| A8 | Model writes create parent directories and expose operation/path errors. | environment-final-filesystem-*; mount-layout-whpx-* |
| B9 | All 48 required upstream applets run through ash with error cases. | All 21 environment-complete-native-test_busybox_*.log suites; per-command WHPX records above |
| B10 | /bin contains actual executable aliases and PATH resolves files/scripts. | program-paths-whpx-20261024; environment-final-assets-* |
| B11 | Shell discovery and process execution use the same installed commands. | environment-final-assets-*; program-paths-whpx-20261024 |
| B12 | /dev/null redirection and builtin detection work. | character-devices-whpx-*-final; current native basic suite |
| B13 | Tool state persists; exit/clear/cancel report resets and isolate other roots. | sessions-final-whpx-*; sessions-default-whpx-*; environment-complete-native-sessions.log |
| B14 | python - consumes finite stdin without a REPL banner. | environment-final-filesystem-*; environment-complete-native-cpython.log |
| B15 | Validated manifests install lazy commands, libraries and licenses. | environment-final-assets-*; timezone-host-tests.log installation/revision/cache cases |

Artifact paths in this table are under artifacts/. Existing save/recovery gates
also pass current host tests: interrupted journal generations, deferred inode
reclamation, actual FAT save/reload/restage, and host conflicts before effects.
The resource check passes the C application sequence at 256 MiB; an additional
Python child exceeds that configuration in the larger session sequence.
The complete environment uses 512 MiB. Final packages are in
artifacts/environment-final-package; artifact hashes are recorded in
artifacts/environment-final-builds.tsv.
