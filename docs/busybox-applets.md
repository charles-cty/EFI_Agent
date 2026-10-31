# BusyBox applet selection

The shared BusyBox 1.37.0 image installs 78 command names, including ash/sh.
The 2026-11-01 extension adds these 27 names through upstream implementations:

| Purpose | Added commands |
| --- | --- |
| Files and text | dd, mktemp, readlink, truncate, split, paste, comm, tac, nl, fold, strings |
| Compare and edit | cmp, diff, patch |
| Arithmetic and checksums | expr, bc, dc, cksum, crc32, md5sum, sha1sum, sha512sum |
| Compression | bzip2, bunzip2, xz, unxz, lzma |

sha256sum was already installed. bzip2 encodes and decodes; xz/unxz and lzma
decode and test streams only, as provided by upstream BusyBox. Tar automatic
decompression retains its existing gzip support. bc and dc use arbitrary
precision arithmetic; bc terminal editing and dd signal-status handling are
disabled. dd works on the files and devices exposed by this runtime.

All commands share the existing filesystem, process dispatcher and command
catalog. Named files accept O_NONBLOCK because their operations complete
synchronously. The flag remains visible in descriptor metadata. Temporary
files use exclusive creation and WASI randomness; temporary directories use
mode 0700. cmp/readlink quiet modes suppress diagnostics through the shared
logging helper. diff, patch and bc keep globals in their own WASM instances,
following the existing awk/gzip port pattern.

The following requested commands remain deferred:

| Commands | Required work |
| --- | --- |
| mount, umount, fdisk | A guest mount-management and block-device interface |
| ps, top, pgrep | Linux-compatible process information or an explicit applet adapter |
| pkill, kill, timeout | Guest signal delivery and process-control semantics |
| watch, time | Additional process-launch, terminal or resource-accounting integration |
| vi, ed, less, more | Editor/pager porting and interactive terminal integration |
| ar, cpio | Additional archive extraction helpers; ownership and process paths need review |
| lzop | An additional codec and its build integration |
| tree | No upstream tree applet in the vendored BusyBox 1.37.0 distribution |

The fixed feature header is guests/busybox/include/ash_config.h. The source
list is guests/busybox/sources.txt. Command dispatch is in
guests/busybox/applets.c, and installation aliases are declared in
crates/filesystem/src/installation.rs. Update these together.

scripts/test_busybox_extra.py runs the actual WASM applets through CPython.
It checks byte boundaries, quiet/error status, temporary permissions, 64-bit
and arbitrary precision arithmetic, diff/patch round trips, independently
computed hashes and independently encoded/decoded compressed bytes.
