# Command and filesystem environment delivery

Status: in progress. The checks below define completion of the environment
expansion. Initial native/firmware interfaces do not establish full completion.

The shared image currently uses upstream awk, sed, grep, chmod, cat, head,
tail, wc, tee, sleep, basename, dirname, pwd, env, printenv, seq, yes, true,
false, od, hexdump, base64, sha256sum and realpath, plus mkdir, rmdir, rm,
touch, date, sort, uniq, cut and tr. Other
utility applets still need upstream replacement or addition.
Date currently supports UTC; other TZ values fail explicitly and timezone
support remains pending.
Firmware monotonic waits now use a reported hardware-counter rate. The mount,
PATH, packaging and persistent-session gates remain incomplete.
Python's standard `os.remove`, `os.unlink` and `os.rmdir` now use the same
namespace mutation providers as WASI and BusyBox. File and directory deletion
therefore persists through the workspace namespace; focused WHPX application
coverage remains pending.
The private subprocess adapter also passes cwd and per-child umask through the
process ABI. Native Release integration verifies parent cwd/umask inheritance,
relative and absolute child cwd, missing-cwd errors and explicit child umask.
The full CPython suite passes 17 cases. Windows Release packaging and the
512 MiB WHPX/Responses filesystem-tool sequence pass, including inherited and
explicit subprocess cwd/umask behavior and independent saved FAT checks.
The same WHPX sequence also verifies automatic import cache creation/reuse,
py_compile, compileall and `-B` suppression with independent saved FAT checks.

## Required commands

Build the upstream BusyBox implementations of ls, cat, cp, mv, rm, mkdir,
rmdir, touch, pwd, head, tail, wc, grep, sed, awk, find, sort, uniq, cut, tr,
tee, xargs, basename, dirname, realpath, stat, du, df, date, sleep, env,
printenv, seq, yes, true, false, which, od, hexdump, base64, sha256sum, tar,
gzip, gunzip, unzip, zcat, ln and chmod. Keep the existing ash builtins.
Each external invocation gets a fresh process, with one shared compiled
multicall module. Preserve upstream parsing and command algorithms.

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
