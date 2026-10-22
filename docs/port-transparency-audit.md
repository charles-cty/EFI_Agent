# Application port transparency audit

Date: 2026-10-21. Scope: guest entry points, Python and JavaScript libraries,
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
| High | `scripts/prepare_busybox.py` checks the program registry before upstream PATH lookup; `guests/busybox/exec.c` selects modules by the registered name. Python uses that same namespace. | A file placed in PATH does not become executable. Add executable-file loading behind normal lookup. Manual registration remains a runtime constraint. |
| Medium | `crates/uefi/src/wasm.rs` uses `split_whitespace`. | Quoted source code, filenames with spaces and empty arguments cannot pass through `/wasm run/start` normally. Add a defined quoting grammar without invoking a shell implicitly. Script-file invocation currently avoids source-code quoting. |
| Medium | The Python adapter explicitly rejects cwd, custom argv[0], pre-exec callbacks, process groups, credentials and umask changes. Signals and timeouts are unavailable. `/dev/null` is absent. | Some standard subprocess options still fail. Add each capability below the API, and retain explicit errors until verified. Infinite `communicate`/wait and `poll()` are supported; this does not establish process termination or timeout support. |
| Medium | The full standard library is packaged, but its OS-dependent operations require runtime support. Dynamic extensions and native threads remain unavailable. | Module presence alone does not establish every operation. Validate each required capability through standard imports and calls. |
| Medium | WASI libc does not propagate CPython's internal close-on-exec setting for ordinary file opens. A native probe reports `os.get_inheritable(open(...).fileno()) == True`. | With `close_fds=False` or `os.system`, a file can cross spawn unless the caller clears inheritance with `os.set_inheritable(fd, False)`. Default `subprocess` descriptor filtering and explicit `pass_fds` are tested. Resolve default file inheritance below the standard API. |

The internal `efi_process`/`efi_net` WASM imports and ash task serialization are
implementation details. Their names alone do not make an application interface
nontransparent. Standard curl trust options and the SSH client's explicit trust
input are also outside the interface defects above.

## Verification boundary

The common persistent namespace and WASI mutation adapters have native-backed
tests for inode lifetime, directory rename, links, modes, timestamps and
recovery. These checks install the namespace explicitly. Normal firmware and
native runner mounting, Python mode reporting and mutation adaptation, and
launcher logical export remain incomplete. This foundation does not establish
that the reported firmware unlink, rename or chmod defects are resolved.

The 2026-10-22 filesystem expansion connects directory creation to native and
firmware providers, and rename to the native provider. File writes now use
64 KiB chunks after complete iovec validation, including calls above 512 KiB.
Finite stdin and captured output no longer report character-device types.
Native unchanged CPython verifies `python -`, nested directories, 8 MiB writes,
py_compile and compileall. Automatic import bytecode writes remain disabled
until firmware rename and deletion support is complete.

Native integration uses actual prebuilt CPython 3.14.7, actual ash and the wasmi runner.
Firmware integration uses actual QEMU/OVMF, standard script files, imports,
subprocess pipes, and an independent read of saved FAT bytes. Exact results,
artifact locations and unresolved limits are in `verification.md`.
This audit covers the paths listed above; it does not establish complete POSIX,
Python or QuickJS conformance.
