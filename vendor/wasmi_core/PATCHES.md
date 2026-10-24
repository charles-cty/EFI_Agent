# Local changes to wasmi_core 2.0.0

Source: https://github.com/wasmi-labs/wasmi, crates.io release 2.0.0.
License: MIT OR Apache-2.0, as declared by the upstream package.

The memory buffer uses `try_reserve_exact` when growing dynamic linear memory.
This prevents geometric spare capacity beyond the requested logical memory.
The UEFI allocator uses allocate-copy-free reallocation, so exact reservation
also reduces the temporary old-plus-new allocation peak. Growth zeroing,
static memory, failure behavior and WASM memory-size semantics are unchanged.

Integration coverage is in the parent workspace's WASM resource-lifetime tests.
