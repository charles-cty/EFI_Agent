# UEFI safety

The firmware application runs on the application processor at `TPL_APPLICATION`.
It stays in the boot-services phase. It does not start application processors,
use the MP Services protocol, or create firmware worker threads. Atomics protect
notification publication against higher-TPL callbacks on that processor. They
do not authorize calls into firmware from other processors. Model requests and
TLS processing run in this same firmware application thread.

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
Each model request uses a separate TCP/TLS connection. Retiring a TCP child
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

Before opening long-lived protocols, startup snapshots firmware handles and
calls recursive `ConnectController` on each handle. Unsupported and already
connected handles are normal. This connects installed drivers; it does not
load absent NIC drivers or provide a network stack or RNG implementation.
Startup also loads an explicit optional DRIVERS.JSON list with standard
LoadImage/StartImage before connection. It accepts x64 PE32+ EFI driver images
only. The boot volume and device-path protocol views are released before the
driver entry point runs. Successful drivers stay resident. Matching full image
device paths are not loaded again. Connection repeats when firmware handles or
Driver Binding instances change. See firmware-drivers.md for bounds and errors.

Native packages boot the bundled EDK II Shell. Its adjacent `startup.nsh` uses
`homefilesystem`, not an assumed `fs0:`, to start Agent on the Shell boot volume.
The Shell remains resident while Agent runs. `/exit` returns to its prompt.
VM disks boot Agent directly, including when a generated native package is
used as VM input. The firmware configuration is still read from Agent's own
boot volume. Shell presence does not authorize command execution by the model.

EDK II installs file-backed console wrappers when starting a child application.
Those wrappers do not implement cursor-addressed drawing. The native interface
uses firmware SimpleText handles instead, preferring console splitters without
device paths. Console access uses short-lived GET_PROTOCOL opens, with no
driver disconnection or protocol reference across image execution. System-table
console pointers are not replaced. Input and rendering run on the application
thread; EFI completion callbacks do not access console protocols.
Output selection checks the far screen edge against the advertised mode.
The startup script selects 80x25, the mandatory common text mode, to keep
display and serial console bounds equal. Cursor visibility is optional in UEFI;
UNSUPPORTED when hiding or showing the cursor does not stop the interface.

Probe capabilities instead of assuming that all UEFI implementations provide
the same protocols. Startup records a capability report. `/caps`
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
or a directly accessed DHCP protocol. DNS A queries use TCP to the configured
IPv4 DNS server. Rustls with RustCrypto performs HTTPS in the firmware
application. HTTP framing and provider SSE parsing use bounded buffers.

Cryptographic random capability detection opens each available EFI RNG protocol,
reads a bounded algorithm list, and tries a 32-byte request with an explicitly
advertised SP800-90 CTR-256, HMAC-256, or HASH-256 algorithm. Unsupported
algorithms and failed requests do not qualify that named algorithm. If a named
algorithm is unavailable, detection also checks the EFI RNG protocol default.
Generated sample bytes are not displayed.
Success establishes that firmware advertises and serves that algorithm. It does
not prove the source's entropy quality or certify the implementation.

TLS uses the EFI RNG protocol's default cryptographic generator, which also
supports OVMF builds that lack named DRBG algorithms. It fails closed if no
generator can fill the requested buffer. It never substitutes timestamps,
counters, a deterministic generator, or an ordinary PRNG. Trust in firmware's
random source remains a platform requirement.

TLS verifies chains against bundled public roots and an optional configured DER
CA, verifies the provider hostname, and checks certificate validity with the
firmware clock. An unspecified firmware timezone is treated as UTC. Verification
cannot be disabled. API credentials are sent only after a successful handshake.
The config is stored on the boot volume; native packages contain credentials,
and VM launches inject them into a private temporary writable disk.

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
