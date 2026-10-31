# EFI Agent QuickJS port

The source baseline is QuickJS 2026-06-04 from
https://bellard.org/quickjs/quickjs-2026-06-04.tar.xz.
All 63 distribution files are included. LICENSE contains the upstream license.
Commit 83713b0 is the unchanged upstream import, with the original executable
file modes. Later changes contain the EFI Agent port.

The maintained source disables threaded Atomics under WASI, includes malloc.h
for WASI allocator declarations and removes dtoa's unused setjmp.h include.
scripts/build_guests.py compiles the engine files directly. It generates the
zx bootstrap header from vendor/zx and the separate guest adapter, and applies
no source rewrites.

The process, shell and retained-session integration stays under guests/quickjs
and guests/lib. The port links quickjs.c, dtoa.c, libregexp.c, libunicode.c and
cutils.c with those adapters. It does not link quickjs-libc.c.

Build from the project directory with native WASI SDK 34, uv and Node.js
available. On Windows, use PowerShell 7.4 or later:

```powershell
./scripts/build_quickjs.ps1 -WasiSdk <wasi-sdk-directory>
```

On Linux:

```sh
uv run scripts/build_guests.py --component quickjs --wasi-sdk <wasi-sdk-directory>
```
