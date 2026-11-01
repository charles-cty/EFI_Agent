# BusyBox applet selection

The shared BusyBox 1.37.0 image installs 90 command names, including ash/sh/bash.
The virtual `/bin/bash`, `/bin/sh` and `/bin/ash` links all point to `/ash.wasm`.
All three names run ash with its configured features. The Python catalog also
publishes `/bin/python3` and `/bin/python` as links to `/python.wasm`.
The first 2026-11-01 extension added these 27 upstream commands:

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

The second extension adds these 11 upstream commands:

| Purpose | Added commands |
| --- | --- |
| Line editing and archives | ed, ar, cpio, lzop |
| Process information and termination | ps, pgrep, kill, pkill |
| Interactive editing and paging | vi, less, more |

ps reads the runtime's real /proc records. It supports pid, ppid, user, group,
stat, comm and args columns. pgrep supports name/command-line regular expressions,
parent filtering and newest/oldest selection. Process IDs increase in creation
order. These commands list guest processes. CPU accounting, resident-memory
counters, process groups and session IDs remain unavailable.

kill and pkill support positive PIDs. Signal 0 checks existence. INT (2), KILL
(9) and TERM (15) terminate the selected process with status 128 + signal.
Live children remain alive and are adopted by PID 0. These operations do not
invoke shell traps or user signal handlers. Other signals and process-group
targets report errors. pkill reports failed operations and returns failure.

ar lists, prints, extracts, creates and replaces members. It reads GNU long
names; upstream creation limits names to 15 bytes. cpio creates newc archives
and lists/extracts newc or crc archives. It preserves supported file modes,
timestamps, symbolic links and file hard links. Passthrough mode (-p) remains
disabled. Archive inputs open before extraction obtains its directory anchor.
lzop includes the LZO codec for compression, decompression and stream testing;
the optional high-compression encoder remains disabled.

ed accepts ordinary line input and pipes. vi, less and more use a controlling
terminal during /enter commands. The terminal supplies individual keys, a VT
screen, dimensions and a separate /dev/tty input path for piped pagers. Space,
q, Esc, arrow keys and PageUp/PageDown reach the application. vi supports colon
commands, search, undo and yank/marks. Ctrl+C leaves vi insert mode; Ctrl+Q
cancels a running raw-terminal command. Normal exit and cancellation restore
line input. less and vi observe window-size changes. The viewport is bounded
to 300 columns and 120 rows. Redirected pagers copy bytes; vi requires a live
terminal. Direct terminal input/output remain outside model history.
vi remains byte-oriented; its multibyte editing option is disabled. The VT
screen implements the editor/pager operations described in wasm-world.md.
Raw screen text does not use the line-scrollback selection interface.

The following requested commands remain deferred:

| Commands | Required work |
| --- | --- |
| mount, umount, fdisk | A guest mount-management and block-device interface |
| top | CPU/resource accounting and a process-display adapter |
| timeout | Child supervision, deadlines and termination integration |
| watch, time | Additional process-launch, terminal or resource-accounting integration |
| tree | No upstream tree applet in the vendored BusyBox 1.37.0 distribution |

The fixed feature header is guests/busybox/include/ash_config.h. The source
list is guests/busybox/sources.txt. Command dispatch is in
guests/busybox/applets.c, and installation aliases are declared in
crates/filesystem/src/installation.rs. Update these together.

scripts/test_busybox_extra.py runs the actual WASM applets through CPython.
It checks byte boundaries, quiet/error status, temporary permissions, 64-bit
and arbitrary precision arithmetic, diff/patch round trips, independently
computed hashes and independently encoded/decoded compressed bytes.

scripts/test_busybox_tools.py checks the second extension through real WASM
and CPython. Host archive readers and an independent liblzo2 check saved bytes.
scripts/test_vm_busybox_terminal.ps1 checks real Windows QEMU terminal input,
editing, paging, pipe input, resizing, cancellation and exported host files.
