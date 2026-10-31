# EFI Agent BusyBox port

The source baseline is BusyBox 1.37.0 from
https://busybox.net/downloads/busybox-1.37.0.tar.bz2.
LICENSE contains the upstream license. The upstream import is a separate Git
commit. Later commits contain the EFI Agent changes.

The port compiles the files listed in `guests/busybox/sources.txt`. Its fixed
feature configuration is `guests/busybox/include/ash_config.h`. The C sources
in this directory contain the port changes directly. Build scripts compile
them without generating or replacing source text.

Ash starts independent WASM instances through the cooperative process ABI.
`guests/busybox/tasks.inc` transfers syntax trees and shell state between
instances. `session.inc` supplies the retained shell session. The remaining
files under `guests/busybox` implement process, descriptor, filesystem and
libbb interfaces. `shell/shell_read.c` is the extracted upstream read builtin
with the WASI clock and input interfaces.

The applet changes preserve binary records, propagate I/O errors and adapt
filesystem metadata and archive extraction. The shared nonlocal-jump runtime
is under `guests/sjlj`. The timezone rules source is under
`vendor/musl-timezone`; packaged IANA data comes from tzdata 2026.5.

Build on Windows with PowerShell 7.4 or later:

```powershell
./scripts/build_busybox.ps1 -WasiSdk <wasi-sdk-directory> -Binaryen <binaryen-directory>
```

On Linux, use the native Linux WASI SDK and Binaryen:

```sh
uv run scripts/build_busybox.py --wasi-sdk <wasi-sdk-directory> --binaryen <binaryen-directory>
```

WASI SDK 34 and Binaryen 133 are the verified toolchain versions. The
`WASI_SDK_PATH` and `BINARYEN_PATH` environment variables can supply their
directories. The default output is `artifacts/wasm-guests`. The packaging
entry points rebuild BusyBox and refresh the catalog before packaging.
Prepare the other application ports in the selected guest directory first.
