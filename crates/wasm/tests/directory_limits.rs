use efi_agent_wasm::filesystem::{DirectoryEntries, EntryType, FileError, MAX_DIRECTORY_ENTRIES};

#[test]
fn entry_limit_rejects_the_next_name() {
    let mut entries = DirectoryEntries::new();
    for index in 0..MAX_DIRECTORY_ENTRIES {
        entries.push(format!("n{index}"), EntryType::File).unwrap();
    }
    assert_eq!(
        entries.push("extra".into(), EntryType::Directory),
        Err(FileError::ResourceLimit)
    );
}

#[test]
fn byte_limit_counts_headers_and_rejection_preserves_available_space() {
    let mut entries = DirectoryEntries::new();
    // 2047 records of 256 encoded bytes leave exactly 256 bytes.
    for index in 0..2047 {
        entries
            .push(format!("{index:04}{}", "x".repeat(228)), EntryType::File)
            .unwrap();
    }
    assert_eq!(
        entries.push("y".repeat(233), EntryType::File),
        Err(FileError::ResourceLimit)
    );
    entries.push("z".repeat(232), EntryType::File).unwrap();
    assert_eq!(
        entries.push("end".into(), EntryType::File),
        Err(FileError::ResourceLimit)
    );
}
