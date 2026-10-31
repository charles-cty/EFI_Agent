# musl timezone rules

`__tz.c` comes from `src/time/__tz.c` in musl 1.2.5:
https://musl.libc.org/releases/musl-1.2.5.tar.gz.
COPYRIGHT contains the upstream license. The unmodified source import is a
separate Git commit.

This single file supplies POSIX TZ and IANA TZif rules for BusyBox date.
The WASI SDK supplies the remaining C library. Its UTC timezone implementation
does not supply the named zones and daylight-saving transitions used here.

The maintained source replaces musl's internal headers, locks and mmap
interfaces with the single-thread WASI adapter in `guests/lib/timezone.c`.
It shares the SDK's calendar helpers and UTC constant, accepts version-1
TZif's NUL version byte and renames the internal `index` identifier to avoid
the SDK's GNU declaration. The adapter validates and bounds timezone file
buffers before the rules engine reads them.

The BusyBox build compiles this file directly. `scripts/prepare_timezone.py`
stages IANA 2026e data from tzdata 2026.5 and the required licenses.
