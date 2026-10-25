# zx WASI integration

This directory contains the unchanged npm distribution of `zx@8.8.5-lite`.
Source: https://registry.npmjs.org/zx/-/zx-8.8.5-lite.tgz
License: Apache-2.0. Dependency notices are in `build/3rd-party-licenses`.

`scripts/prepare_quickjs.mjs` selects unchanged code for `quote`, `buildCmd`,
`substitute`, `isStringLiteral`, `Fail`, `ProcessOutput` and their portable helpers.
The generated C header embeds that code with `guests/quickjs/zx.js`.
The build needs Node.js but does not download packages or run npm.

The WASI adapter supplies process execution and a thenable ProcessPromise.
It supports asynchronous commands, upstream interpolation, ProcessOutput errors,
nothrow/quiet/verbose, text/JSON/lines/UTF-8 bytes, cwd, environment overlays,
text input, and bounded completed-output pipelines through stdout/stderr/stdall.
Combined output follows
dispatcher read order. Each stream is limited to 512 KiB; input is limited to
512 KiB. A pipeline waits for its producer and then starts its consumer.

Full zx@lite still requires Node APIs. Live Node streams, synchronous execution,
signals, timers/timeouts, filesystem helpers, AsyncLocalStorage/within, shell
selection and alternate text encodings are not implemented. Unsupported options
fail explicitly. buffer() returns a Uint8Array. Blob needs a user polyfill.
The selected upstream text() method is replaced to reject unsupported encodings.
Default prefix is empty so ordinary ash errors keep their usual semantics.
Commands are captured by default (verbose is false); quiet suppresses verbose
output. Quoting requires the enabled BusyBox ASH_BASH_COMPAT feature.

For an update, replace this distribution, review the selected sections, and run
the real-ash quote, process, session and firmware integration checks. Keep this
file and the adapter separate from the upstream distribution.
