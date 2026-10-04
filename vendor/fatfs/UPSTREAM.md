Source: https://github.com/rafalh/rust-fatfs
Revision: 2aefc2a027ce94ed0671752814dac203f0450e11 (fatfs 0.4.0).
License: MIT or Apache-2.0; original license files are included.

Local change: ShortNameGenerator finds the final dot on the complete UTF-8
string and excludes byte index zero. Slicing at byte index one panicked when
the first character used multiple UTF-8 bytes. The package integration test
checks a Chinese filename with independent mtools extraction.

Only library source and package metadata are included. Upstream examples and
test resources are omitted. This copy is excluded from workspace test targets.
