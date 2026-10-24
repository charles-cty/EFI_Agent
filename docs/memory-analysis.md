# WASM and CPython memory analysis

Date: 2026-10-24. Scope: wasmi 2.0.0, the current runtime and the unchanged
CPython 3.14.7 WASI SDK 24 distribution.

The 512-MiB guest configuration is a passing configuration for the complete
application workload. It is not a measured minimum or an inherent wasmi or
CPython requirement. The baseline code had avoidable module, allocation-growth
and installation-validation costs. The implementation below addresses them.

## Measurement method

An isolated Windows Release executable used the production wasmi version and
features, fuel metering, 4096-frame recursion limit and 8-MiB stack limit.
A counting allocator recorded requested live allocation bytes and phase peaks.
Its realloc performed allocate-copy-free, matching the default GlobalAlloc
implementation used by the UEFI allocator. This accounts for old and new
buffers coexisting; it does not model firmware allocator headers, page rounding
or fragmentation. Values below are MiB (1048576 bytes).

Module probes validate real binaries. Instantiation-only probes supply inert
function imports and do not execute application code. Real execution probes
use World, the persistent native namespace, host clocks and randomness, the
private session driver and the unchanged Python/library. A retained Python
session imports subprocess and prints a marker; a second Python starts,
prints another marker and exits while the first remains alive. Both markers,
empty stderr and successful exits are checked. No source is changed in the
production runtime. Modified World code exists only in an isolated copy for
the configuration experiments. Evidence: artifacts/memory-analysis/*.tsv.

The source inspection uses the exact locally installed dependency versions.
Native allocation measurements exclude OVMF, firmware drivers, the application
UI, model history and firmware network buffers. These results do not establish
a new minimum WHPX guest configuration or full regression acceptance.

## Unused custom sections are retained

The 30522756-byte python.wasm contains 21.62 MiB of DWARF debug sections.
The complete module without custom sections is 7.28 MiB. Python is an
optimized release build with debug information (Py_DEBUG=0, -O3 and -g).
The code section is 4.23 MiB and initialized data is 3.03 MiB.

wasmi Config defaults ignore_custom_sections to false. Its module parser
copies the custom sections into a growing names_and_data Vec. The runtime
does not consume these sections. As a result, the parser retains more than
the debug payload's length because of allocation capacity.

| Python probe | Current eager | Eager, ignore custom sections |
| --- | ---: | ---: |
| Module and Engine, after dropping input bytes | 57.26 | 23.36 |
| Initial instance, before executing Python | 98.09 | 64.19 |
| Module construction peak, including input file | 103.46 | 55.28 |

Ignoring custom sections saves 33.90 MiB of live allocation in this module
probe. It preserves the original binary and Python library bytes. The
temporary full-runtime experiment also removes unused ash custom sections:

| Real World execution | Current | Ignore custom sections |
| --- | ---: | ---: |
| Initialized retained Python session | 103.73 | 69.04 |
| Session plus second instantiated Python | 144.54 | 109.85 |

This is the first change to make. Apply the setting to both execution and
installation-validation Engines, then verify firmware scheduling, error output
and the existing application tests. Normal Python tracebacks use Python code
objects; retaining WASM DWARF is a separate concern. Debug files can remain
on disk without copying them into runtime Module objects.

## Linear memory capacity doubles on small growth

wasmi_core ByteBuffer::grow_vec uses Vec::try_reserve rather than exact
reservation. For a Python instance, requesting one more 64-KiB WASM page
increases logical memory from 40 MiB to 40.0625 MiB, but the allocated Vec
capacity grows from 40 MiB to 80 MiB. The production 64-MiB memory quota limits
logical memory, not that hidden capacity.

With allocate-copy-free realloc, old 40-MiB and new 80-MiB buffers coexist.
The current instantiation probe therefore grows from 98.09 MiB to 138.09 MiB
live allocation, with a 178.09-MiB growth peak. Ignoring custom sections reduces
that peak to 144.19 MiB but does not fix the capacity policy.

A bounded growth policy in wasmi's memory backend can use exact reservation
or bounded chunks, capped at the logical memory maximum. Exact growth to
40.0625 MiB would remove about 39.94 MiB of excess capacity in this example.
It would still need old and new buffers during realloc. Exact allocation for
every page can cause repeated copying; bounded chunk growth needs throughput
and peak-memory comparisons. This requires an upstream change or a maintained
dependency patch. Reducing MAX_MEMORY_BYTES alone does not fix the problem.
The small initial ash buffer also follows the same doubling policy.

## Translation mode and encoding

The runtime eagerly translates every function. Measured module/Engine live
allocations after dropping input bytes are:

| Configuration | Python | ash |
| --- | ---: | ---: |
| Current eager, retain custom sections | 57.26 | 5.66 |
| Eager, ignore custom sections | 23.36 | 4.90 |
| Eager, ignore custom sections, indirect-dispatch | 14.43 | 2.26 |
| LazyTranslation, ignore custom sections | 10.82 | 1.55 |

indirect-dispatch is an existing wasmi feature with a more compact IR encoding.
It saves a further 8.93 MiB for Python and 2.64 MiB for ash in this probe.
The isolated World experiment with both custom sections ignored and compact
IR passes the real Python session/second-process checks at 57.53 MiB and
98.35 MiB respectively, compared with 103.73 MiB and 144.54 MiB currently.
This is approximately 46.2 MiB less live allocation in that small workload.
The documented tradeoff is execution performance; no firmware performance
or full application acceptance is established by these probes.

LazyTranslation reduces initial translation and avoids compiling unused
functions. Its allocation grows as functions execute, so construction savings
are not a steady-state guarantee. The runtime has an explicit warning about
fuel exhaustion during lazy callee translation. Any mode change must verify
resumption inside a previously untranslated call, required_fuel replenishment,
host suspension, cancellation and fair concurrent progress. A simple startup
check cannot establish those properties. Eager plus compact encoding can be
evaluated independently of lazy execution.

## Engine lifetime and caches

Module::clone shares an Arc. Python children do not each duplicate the entire
translated module; each does allocate its own linear memory and Store.
Combining aliases or eliminating Module clones will not remove those copies
because the existing aliases already share code.

Translated function storage belongs to Engine. Repeatedly constructing and
dropping the same Python Module in one Engine, with custom sections ignored,
leaves approximately 17.78, 34.39, 50.00 and 67.62 MiB after successive drops,
excluding the still-live input file. Dropping Engine releases that storage.
The Engine also retains translator/validator work buffers and reusable stacks.
The probe does not attribute every Engine allocation to one cache category.

The World compiled-file cache also retains modules for reuse. Revisions cause
new translations in the same Engine. Therefore deleting entries from that
cache is insufficient for complete reclamation. A bounded module/Engine
lifetime or rebuilding an idle World is required for full recovery. Rebuilding
a live World must preserve or explicitly end its sessions and independent roots.
An unconditional reset after each command would break retained state.

The configured 8-MiB value-stack limit is a maximum, not an 8-MiB startup
allocation for every process. wasmi's default minimum is 1000 bytes and stacks
grow on demand. Lowering this limit is not a priority memory optimization.
The Engine defaults to retaining two reusable execution stacks; this can be
tuned after measuring deep-call workloads. It is separate from Python's C stack.

## Python's fixed costs and limits

The supplied build configuration explicitly sets a 16-MiB C stack and a
40-MiB initial memory, with --stack-first. The module confirms 640 initial
WASM pages. Its data segments start at byte 16777216 and end at 19949108.
The 16-MiB C stack is inside the 40-MiB linear memory; it must not be counted
again. One initialized session plus one simultaneous Python child needs at
least 80 MiB of linear memory before module, Store, firmware or UI costs.

Keeping the distribution binary byte-identical fixes that 40-MiB initial
allocation. A Python monkey patch starts after allocation and cannot lower
the module's memory minimum or shrink WASM linear memory. Python can reuse
freed objects within its heap, but that does not return linear-memory pages
to firmware. The prebuilt build enables mimalloc and disables pymalloc;
allocator environment tuning needs measurement and cannot remove the minimum.

Reducing stack/minimum values requires another suitable upstream build or
an explicitly accepted binary transformation/relink. Arbitrarily editing
the memory header or stack pointer is not justified by data-section bounds:
the heap start, zero-initialized data, allocator and stack requirements also
matter. Reintroducing a CPython source build is unnecessary for the first
runtime optimizations above.

Trimming unused Python startup imports can lower object allocations. It will
not remove the 40-MiB minimum. Deleting packaged standard-library files also
does not imply proportional RAM savings: these files are loaded on demand.
Retaining one interpreter is useful and already avoids repeated startup;
ordinary subprocess calls still require separate instances to preserve
process isolation. Limiting their concurrency is a workload policy change,
not a substitute for fixing hidden buffers.

## Priority and validation

1. Ignore unused custom sections and defer module reading/validation until the
   corresponding executable is requested. The current ash startup validates
   unused Python, including its file buffer and custom-section copy.
2. Bound wasmi linear-memory allocation capacity and growth peaks, with
   asymmetric checks at 40 MiB, 40 MiB plus one page and the 64-MiB quota.
3. Evaluate indirect-dispatch with real Python, ash pipeline and scheduling
   workloads; compare memory and execution time.
4. Bound Engine/module lifetimes and test repeated source revisions, session
   resets and live old instances. Verify actual release rather than only exits.
5. Evaluate LazyTranslation with explicit fuel/resumption tests.
6. Reconsider the upstream Python artifact only if its remaining 40-MiB floor
   prevents the intended configuration after runtime costs are corrected.

Repeat actual WHPX checks for ash-only, unused installed Python, one retained
Python, Python-to-Python spawn and the full concurrent application workload.
Record firmware free bytes, largest successful allocation, live requested
bytes and growth peaks. Total guest RAM and native allocated bytes must remain
separate. The available evidence supports substantial reductions; it does not
yet establish a replacement 128-, 192- or 256-MiB full-environment requirement.

## Implemented memory and lifetime changes (2026-10-25)

The runtime now ignores custom sections, uses compact indirect dispatch and
keeps eager compilation. Discovery reads installation metadata only. Requested
commands still validate all bodies, imports and command exports before PID
allocation. The original CPython binary and library remain unchanged.

The local wasmi_core 2.0.0 patch uses exact linear-memory reservation. The
40-MiB-plus-one-page integration check rejects geometric excess allocation;
growth to 64 MiB succeeds and the next page fails without changing memory size.
Allocate-copy-free can still require both old and new buffers temporarily.
Exact growth can copy more often for workloads that grow one page at a time.

Each file-backed image owns an Engine and Linker. Concurrent instances share
that image through strong references; the revision cache keeps weak references.
The final execution drops the Store and image before wait/reap, which also
releases translated code and Engine storage. Explicit byte registrations remain
snapshots until replaced or the World is dropped. Retained conversation
interpreters remain live between tool calls; exit, cancellation and clear
release them. These lifetimes preserve concurrent process isolation.

Pipeline exception cleanup now closes pending pipe ends, cancels and reaps
already-started stages, and frees the task serialization buffer on failure.
The private cancel_child import accepts only a direct child's subtree, leaves
that child waitable with status 130, and reaps its descendants. Parents stop
before their children so a suspended wait cannot reap a child midway through
cleanup. Tool cancellation and the native session driver use the same ordering.

The native lifecycle driver uses a counting allocate-copy-free allocator and
runs repeated real applications in one World. Its integration script checks
ordinary exits, failures, cancellations, early-reader EOF, large pipelines,
partial pipeline startup failure and retained Python session exit/reset. HTTP
and SSH peers independently observe connection closure on cancellation.
Evidence and firmware acceptance results are recorded in docs/verification.md.

The implemented Release lifecycle measurement retains about 55.3 MiB for one
initialized Python session. Normal exit or cancellation releases its execution
and code before reap, leaving about 45 KiB of namespace/runtime metadata.
Repeated complete process cycles leave about 25 KiB after reap, without growth.
The earlier configuration tables include explicitly registered ash code as
well as Python and therefore are not an exact comparison with this single
file-backed session. Actual WHPX passes the full concurrent application workload
at 256 MiB and both model API session workloads at 192 MiB. These are measured
passing configurations. The unchanged Python 40-MiB initial memory remains.

## Complete network application RAM boundary (2026-10-25)

With the same optimized Release artifacts, the complete Windows WHPX network
application sequence passes twice at 198 MiB of total guest RAM. At 197 MiB,
the standard Python script fails twice while spawning its duplex Python
subprocess. The 192-MiB capture and 197-MiB repeat explicitly report ENOMEM.
The first 197-MiB traceback omits the final errno line. See docs/verification.md and
artifacts/network-memory-boundary for individual runs and exclusions.

This establishes an observed boundary at 1-MiB resolution for the fixed sequence.
It includes firmware/UI, registered curl/SSH/QuickJS/ash/Python images, replacement
module loading and overlapping process instances. Registered snapshots retain
compiled code; replacement loading can temporarily retain old and new images.
It therefore measures a larger peak than the native Python-plus-ash session.
256 MiB supplies 58 MiB above the lowest passing configuration. Larger scripts,
more concurrent processes and growing output/history can need additional RAM.
The previous ash-only boundary still requires a separate post-optimization run.
