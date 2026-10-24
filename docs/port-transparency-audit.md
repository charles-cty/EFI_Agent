# Application port transparency audit

Date: 2026-10-24. Scope: guest entry points, Python and JavaScript libraries,
process adapters, executable lookup, and the firmware command frontend.

The requirement is that applications use their standard public interfaces.
Firmware-specific work belongs below those interfaces. A missing host capability
must produce an explicit error with no fabricated success.

## Corrected Python interfaces

The old Python port exposed `efi.shell`, accepted `-e`, removed builtins, limited
FileIO to descriptors 0–2, and disabled filesystem imports. These choices made
ordinary scripts require source changes despite existing runtime process and
filesystem support. The correction removes the public `efi` module and restores
upstream CLI, FileIO and import behavior. The current implementation uses
unchanged prebuilt CPython 3.14.7 and its full upstream standard library.
`sitecustomize.py` installs a Python monkey patch on the process hooks used by
upstream `subprocess.py` and `os.system`. Standard WASI descriptor operations
reach the private process-local `.efi-process` endpoint. The runtime endpoint
cannot cross a spawn boundary. CPython source builds and C adapters are removed.

Source: `guests/python/sitecustomize.py`, `guests/python/_efi_process.py`,
`scripts/prepare_cpython.ps1`, and `crates/wasm/src/process_control.rs`.
Integration coverage: `scripts/test_cpython.py` and `scripts/smoke_wasm.py`.
The patch changes upstream process selection and explicitly rejects process
timeouts. `subprocess.py` and the interpreter binary remain unchanged.
Normal site initialization is required; `-S` disables the patch.

The correction also removes silent argument-list shell conversion, replacement
UTF-8 decoding as a fixed policy, and the custom 512 KiB capture limit. Standard
Python options now determine shell use, decoding and nonzero-exit handling.

## Remaining findings

| Priority | Finding and evidence | Effect and required direction |
| --- | --- | --- |
| High | QuickJS's custom `main` in `guests/quickjs/main.c` accepts only `-e`. `scripts/build_quickjs.ps1` omits upstream `qjs` and `quickjs-libc`. | Script-file entry, upstream `std`/`os` modules and file module loading remain absent. Restore those interfaces above internal host adapters. The requested zx-like `$` extension is a separate feature. |
| High | The prebuilt `_socket` module is present, but its socket creation backend remains unavailable. Runtime WASI socket imports reject unsupported operations explicitly. | Standard Python socket clients cannot use existing client TCP/DNS operations. Add the backend below standard socket interfaces. |
| Medium | Installed commands now appear in `/bin`; ash uses upstream PATH lookup and the runtime supports executable files and shebangs. General mount-boundary/link traversal and persistent installation manifests remain incomplete. | Native and WHPX cover ordinary PATH ordering, modes, cwd and shebangs. Complete mount traversal and manifest validation before claiming full executable namespace coverage. |
| Medium | `crates/uefi/src/wasm.rs` uses `split_whitespace`. | Quoted source code, filenames with spaces and empty arguments cannot pass through `/wasm run/start` normally. Add a defined quoting grammar without invoking a shell implicitly. Script-file invocation currently avoids source-code quoting. |
| Medium | The Python adapter explicitly rejects distinct custom argv[0], pre-exec callbacks, process groups and credentials. Signals and timeouts are unavailable. | Standard per-child cwd/umask and virtual `/dev/null` work. Cwd anchors preserve rename/unlink identity below standard file interfaces. Infinite `communicate`/wait and `poll()` are supported. |
| Medium | BusyBox and Python initialize libc's cwd routing and expose physical getcwd results. Other C application entry points have no verified equivalent bootstrap. A native curl probe returns 26 for a relative config in its child cwd; the absolute config reads independent file-URL bytes successfully. | Fix cwd initialization and verify inherited cwd and relative file arguments in curl, WolfSSH and QuickJS before claiming all application ports share cwd semantics. Evidence: artifacts/cwd-anchor-c-port-probe.log. |
| Medium | File-backed WASM modules are cached by resolved physical path. Cwd aliases now resolve to that path, but replacement or editing at the same physical path has no content-based cache validation. | Add executable content or generation validation with installation manifests before completing lazy module delivery. |
| Medium | Private Python scandir entry proxies expose the standard methods and correct modes, including fd anchors, but are not instances of built-in `os.DirEntry`. | Type identity remains an interface difference. Preserve the upstream binary and resolve the missing Preview 1 mode representation below the public API where feasible. |
| Medium | The full standard library is packaged, but its OS-dependent operations require runtime support. Dynamic extensions and native threads remain unavailable. | Module presence alone does not establish every operation. Validate each required capability through standard imports and calls. |
| Medium | WASI libc does not propagate CPython's internal close-on-exec setting for ordinary file opens. A native probe reports `os.get_inheritable(open(...).fileno()) == True`. | With `close_fds=False` or `os.system`, a file can cross spawn unless the caller clears inheritance with `os.set_inheritable(fd, False)`. Default `subprocess` descriptor filtering and explicit `pass_fds` are tested. Resolve default file inheritance below the standard API. |

The internal `efi_process`/`efi_net` WASM imports and ash task serialization are
implementation details. Their names alone do not make an application interface
nontransparent. Standard curl trust options and the SSH client's explicit trust
input are also outside the interface defects above.

## Verification boundary

The common persistent namespace and WASI mutation adapters have native-backed
tests for inode lifetime, directory rename, links, modes, timestamps and
recovery. Firmware and the native runner now mount this namespace, shared with
model file tools and module loading. Launcher logical staging/export/save has
an actual FAT round-trip test. Focused QEMU evidence verifies Python unlink,
rename, symlinks, open-file identity and directory mutations against saved
logical FAT bytes. The private metadata adapter now supplies Python and BusyBox
mode reporting/mutation, single-owner access checks and inherited umask/cwd.
Windows native checks verify real denial, retained file access, fd-relative
metadata after rename, symbolic/recursive chmod and sed mode preservation.
The dedicated permission integration independently verifies saved modes and
process restart. Firmware evidence is recorded separately in verification.md.
Allocation reporting now also uses the private adapter below standard
stat/fstat/scandir. Windows handle queries and EFI physical sizes replace
WASI libc's synthesized blocks. Native sparse-file checks use an independent
Windows path query; both WHPX model APIs compare saved FAT cluster chains.
Unknown allocation fails explicitly. Shared namespace journals are accounted
at filesystem level, outside per-file allocated data bytes.
Hard-link support uses the same namespace inode graph across C link/linkat,
BusyBox ln/cp and Python os.link. The prebuilt interpreter omits directory-fd
link operations, so its private startup adapter supplies that standard method
through the internal endpoint, including explicit symlink-following control.
The launcher retains shared identities through FAT staging and host save;
equal contents alone do not imply a hard-link group.
Standard Python statvfs/fstatvfs now report the runtime filesystem statistics
through private startup adapters. BusyBox df uses those same measured capacity
and namespace-slot providers through C statvfs/fstatvfs. Snapshot descriptors
remain private, close on spawn and close after each reply. General mounts and
cross-mount traversal still need their delivery gates.
The temporary mount now reuses the inode namespace over bounded memory storage.
Native Python/ash checks cover nested directories, relative symlinks, hard links,
permissions, timestamps, retained directory fds, tempfile cleanup and cross-device
move fallback. Both 512 MiB WHPX model API flows verify this subset with
independent saved FAT bytes/metadata and temporary mount exclusion.
The global root now resolves absolute/relative links across runtime mounts,
including link/parent ordering, a shared 40-link limit and entry mutation
policy. Python metadata and child cwd preserve these components. Root directory
fds retain mounts; non-root directory fds remain bounded after rename. Native
and both actual 512 MiB WHPX model API flows include independent saved FAT
bytes, modes and link targets. Anchored workspace views also retain the routing
layer: rename/hard-link operations work in both directions with the root after
directory rename, while local tmp/dev/proc/bin names remain ordinary directories.
Mount construction now rejects existing workspace dev/tmp/proc entries,
non-directory bin entries and installed-command conflicts before publication.
The error retains the path; failed replacement preserves the previous root.
Ordinary bin children remain merged. Firmware model file tools and module
loaders now use the same mounted root as guest code.
Null/zero/random/urandom now report character metadata and fixed 0666 modes.
Reads provide EOF, zero bytes or the configured cryptographic provider; writes
discard data without seeding entropy. Random reads use bounded staging and do
not publish bytes on provider failure. Device fdstat uses UNKNOWN to avoid
WASI libc's character-device-as-terminal assumption; filestat retains the
character type. Standard isatty and unchanged Python stdin remain noninteractive.
Seek returns zero; truncate and explicit sync fail with EINVAL, while mode/time
mutations fail with EROFS. BusyBox mount selection omits the Linux UBI character
device special case and uses the containing filesystem.
Process files now expose actual PID/parent/state, NUL-separated startup
arguments/environment, cwd/exe links, descriptors and mounts. /proc/self is
bound per process; descriptor tables publish weak references, so introspection
retains no execution Stores or endpoints. Zombie records release startup data
and descriptors; reaping removes their directory. /proc/uptime measures runtime
monotonic elapsed time and scheduler idle time. Its clock failures are explicit.
Files are snapshots captured at open and report zero filestat size.
Descriptor aliases use fdescfs duplication semantics, with shared offsets/status
and no O_TRUNC effect. Their link text uses stream labels or device/inode pairs.
Directory aliases retain anchors after rename and share the global link budget.
Cooperative stream aliases use the guest ABI; model file helpers can open
regular-file aliases. Proc cwd links retain directory anchors through ancestor
rename/unlink. Their registry references are weak. Physical getcwd fails after
unlink while relative access, parent navigation and inherited child identity
remain available. Ordinary directory-fd bounds remain unchanged. Python keeps
the upstream interpreter and ordinary open/FileIO operations unchanged; the
private startup adapter initializes libc's routing prefix and supplies standard
cwd queries. BusyBox, curl, QuickJS and WolfSSH share a constructor-time libc
cwd adapter and standard chdir/getcwd wrappers in guests/lib/cwd.c. Native
application-relative config, transfer and authentication checks pass, along
with C allocation/buffer, rename/unlink and long-physical-path cases.
Both actual 512 MiB WHPX API flows also pass these checks and independent saved
FAT bytes in artifacts/c-port-cwd-whpx-responses and
artifacts/c-port-cwd-whpx-chat_completions.

The 2026-10-22 filesystem expansion connects directory creation to native and
firmware providers, and rename to the native provider. File writes now use
64 KiB chunks after complete iovec validation, including calls above 512 KiB.
Finite stdin and captured output no longer report character-device types.
Native unchanged CPython verifies `python -`, nested directories, 8 MiB writes,
py_compile and compileall. Automatic import bytecode writes are enabled.
Native integration verifies cache creation, reuse with unchanged size/mtime,
and suppression under `-B`.

Native integration uses actual prebuilt CPython 3.14.7, actual ash and the wasmi runner.
Firmware integration uses actual QEMU/OVMF, standard script files, imports,
subprocess pipes, and an independent read of saved FAT bytes. Exact results,
artifact locations and unresolved limits are in `verification.md`.
This audit covers the paths listed above; it does not establish complete POSIX,
Python or QuickJS conformance.
