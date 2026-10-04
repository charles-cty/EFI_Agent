# UEFI safety

The firmware application runs on the application processor at `TPL_APPLICATION`.
It stays in the boot-services phase. It does not start application processors,
use the MP Services protocol, or create firmware worker threads. Atomics protect
notification publication against higher-TPL callbacks on that processor. They
do not authorize calls into firmware from other processors. HostBridge threads
run in the host operating system and are outside this firmware execution model.

## Event processing

`WaitForEvent()` is an application-level service. Call it only at
`TPL_APPLICATION`. Do not assume it is reentrant or safe for multiple processors.
The application never raises TPL. Its event dispatcher rejects reentry and is
the only place that calls `WaitForEvent()`. It waits on a plain timer event;
`EVT_NOTIFY_SIGNAL` events cannot be passed to that service.

An EFI completion callback has one job:

```text
EFI callback -> mark completion -> enqueue notification -> return
TPL_APPLICATION -> drain notifications -> process token result -> advance work
```

The TCP4 callback uses a stable, preallocated intrusive queue node. It performs
no allocation, logging, rendering, file access, model processing, protocol calls,
or waits. Duplicate notification of the same operation does not enqueue it twice.
The application dispatcher marks a drained operation ready. Token status,
received bytes, cancellation, and tool execution are processed at application
TPL. Notification nodes remain allocated until their events are closed and the
queue is drained. Tokens and packet buffers remain live until completion or
confirmed cancellation. A firmware failure to release a token must stop the
application rather than free storage that firmware still owns.

The single application thread cooperatively pumps asynchronous TCP4 tokens,
input, resize, cancellation, and rendering. Timer waits yield to firmware between
polls. Main input loops, DHCP waits, and TCP4 waits use the same dispatcher.
These operations use stack-based cooperative waits, not OS threads or SMP.
Idle heartbeats use this same application-level pump. They do not create a
firmware thread or run network logic in a timer callback. Retiring a TCP child
first requests an abortive close, so the peer can discard its old connection,
then resets and destroys the child after its token is retired.
Firmware file and console protocol calls are synchronous; cancellation cannot
preempt a firmware call. Keep their work bounded and do not call them from EFI
callbacks. Serial short-write backoff uses `Stall()` at application TPL and does
not enter a nested `WaitForEvent()` call.

## Watchdog

The UEFI boot manager arms a five-minute watchdog before it starts a boot option.
If the timer expires, firmware can reset the system. A long-running UEFI image
must reset or disable that timer; normal input or network traffic does not do so.

The entry point disables the watchdog with `SetWatchdogTimer(0, ...)` before
loading configuration or starting the UI. `EFI_UNSUPPORTED` means the platform
does not provide that watchdog service. Any other failure stops startup and
reports the firmware status. The application does not silently continue with a
possibly active timer. `uefi::helpers::init()` does not perform this operation.

## Firmware capability requirements

Probe capabilities instead of assuming that all UEFI implementations provide
the same protocols. Startup records a capability report. `/capabilities`
refreshes it and shows network interface counts and cryptographic RNG results.
Missing optional capabilities do not prevent local console and file use.

Model transport requires IPv4 TCP service binding and TCP4. It preserves an
existing firmware address. When IPv4 Config2 is available and no address exists,
it requests DHCP
before creating the TCP child. Some firmware retains an unresolved default
mapping if a child is created before address setup. If TCP4 reports
`EFI_NO_MAPPING`, address mapping is retried with a deadline. IPv4 Config2 is not
required when TCP4 already has a usable address. Interface discovery, child
creation, configuration, DHCP, connect,
transmit, and receive failures produce explicit errors. Each candidate TCP4
interface is tried without assuming a specific NIC. The application does not
implement a second TCP/IP stack or require firmware DNS, HTTP, TLS, PXE, IPv6,
or a directly accessed DHCP protocol. Native relay configuration uses an IPv4
address; HTTPS and provider DNS resolution run on the host.

Cryptographic random capability detection opens each available EFI RNG protocol,
reads a bounded algorithm list, and tries a 32-byte request with an explicitly
advertised SP800-90 CTR-256, HMAC-256, or HASH-256 algorithm. Unsupported
algorithms, oversized or malformed lists, failed protocol opens, and failed
requests do not qualify a source. Generated sample bytes are not displayed.
Success establishes that firmware advertises and serves that algorithm. It does
not prove the source's entropy quality or certify the implementation.

A raw entropy interface or an unknown default RNG is not treated as a usable
cryptographic DRBG. Future use of raw entropy requires a separately reviewed
conditioning and DRBG design. Future SSH must require a usable, trusted
cryptographic random source for keys, nonces, and protocol randomness. It must
fail closed if that source is unavailable or a request fails. It must not use
timestamps, counters, a deterministic test generator, or an ordinary PRNG as a
replacement. SSH is not implemented by capability detection.

## Verification

Run `scripts/smoke_vm.py` for asynchronous transport, live output, cancellation,
resize, and file tool checks. `scripts/smoke_native.py` checks native SimpleText,
partial-frame cancellation, and local FAT tools. The shared completion queue has
structured randomized tests for duplicate notifications, batching, and draining.

`scripts/smoke_safety.py` boots without a NIC, queries capabilities, and checks
that the application remains interactive after 310 seconds. `--rng` adds a
virtual RNG device; `--hold-seconds 0` performs a short capability check. The
report reflects the protocols exposed by the selected OVMF image, not just the
presence of a QEMU device. Physical firmware and NIC verification still require
the target hardware.
