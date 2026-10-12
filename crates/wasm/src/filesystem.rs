//! Host file operations for WASI. File objects retain open-file identity.
use alloc::{boxed::Box, rc::Rc, string::String, vec::Vec};
use core::cell::RefCell;

pub const MAX_DIRECTORY_ENTRIES: usize = 4096;
pub const MAX_DIRECTORY_BYTES: usize = 512 * 1024;
pub const MAX_RESIZE_BYTES: u64 = 48 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileError {
    Access,
    Exists,
    Io,
    IsDirectory,
    NotDirectory,
    NotFound,
    Invalid,
    Unsupported,
    Overflow,
    ResourceLimit,
    FileTooLarge,
}

impl FileError {
    pub fn errno(self) -> i32 {
        match self {
            Self::Access => 2,
            Self::Exists => 20,
            Self::Io => 29,
            Self::IsDirectory => 31,
            Self::NotDirectory => 54,
            Self::NotFound => 44,
            Self::Invalid => 28,
            Self::Unsupported => 58,
            Self::Overflow => 61,
            Self::ResourceLimit => 48,
            Self::FileTooLarge => 22,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Metadata {
    pub directory: bool,
    pub size: u64,
}

impl Metadata {
    pub(crate) fn wasi(self) -> [u8; 64] {
        let mut stat = [0; 64];
        stat[16] = if self.directory { 3 } else { 4 };
        stat[24..32].copy_from_slice(&1u64.to_le_bytes());
        stat[32..40].copy_from_slice(&self.size.to_le_bytes());
        stat
    }
}

#[derive(Clone, Copy, Debug)]
pub struct OpenOptions {
    pub read: bool,
    pub write: bool,
    pub create: bool,
    pub exclusive: bool,
    pub truncate: bool,
    pub append: bool,
}

/// A host-owned open file. Duplicated descriptors share this object's cursor.
pub trait File {
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, FileError>;
    fn write(&mut self, buffer: &[u8]) -> Result<usize, FileError>;
    fn seek(&mut self, offset: i64, whence: u32) -> Result<u64, FileError>;
    fn metadata(&mut self) -> Result<Metadata, FileError>;
    fn flush(&mut self) -> Result<(), FileError>;
    /// Change length without changing the cursor. Extended bytes are zero.
    fn resize(&mut self, size: u64) -> Result<(), FileError>;
}

/// Paths use slash-separated names relative to the provider's root.
/// Providers must preserve open-file identity across changes to path names.
/// Providers must reject symbolic links and link traversal.
pub trait FileSystem {
    fn open(&self, path: &str, options: OpenOptions) -> Result<Box<dyn File>, FileError>;
    fn metadata(&self, path: &str) -> Result<Metadata, FileError>;
    fn read_directory(&self, path: &str) -> Result<DirectoryEntries, FileError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum EntryType {
    Unknown = 0,
    Directory = 3,
    File = 4,
    Symlink = 7,
}

struct DirectoryEntry {
    name: String,
    kind: EntryType,
}

/// Bounded directory data. Providers add names before returning the list.
#[derive(Default)]
pub struct DirectoryEntries {
    entries: Vec<DirectoryEntry>,
    bytes: usize,
}

impl DirectoryEntries {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, name: String, kind: EntryType) -> Result<(), FileError> {
        assert!(
            !name.is_empty() && !name.contains(['/', '\\', '\0']),
            "Host directory entry is not a single name"
        );
        let bytes = 24 + name.len();
        if self.entries.len() == MAX_DIRECTORY_ENTRIES || bytes > MAX_DIRECTORY_BYTES - self.bytes {
            return Err(FileError::ResourceLimit);
        }
        self.entries.push(DirectoryEntry { name, kind });
        self.bytes += bytes;
        Ok(())
    }

    pub(crate) fn sort(&mut self) {
        self.entries
            .sort_unstable_by(|left, right| left.name.cmp(&right.name));
        assert!(
            !self
                .entries
                .windows(2)
                .any(|pair| pair[0].name == pair[1].name),
            "Duplicate host directory entry"
        );
    }

    pub(crate) fn encode(&self, cookie: u64, maximum: usize) -> Result<Vec<u8>, i32> {
        let index: usize = cookie.try_into().map_err(|_| 28)?;
        if index > self.entries.len() {
            return Err(28);
        }
        let mut output = Vec::new();
        for (offset, entry) in self.entries[index..].iter().enumerate() {
            let mut header = [0u8; 24];
            header[..8].copy_from_slice(&((index + offset + 1) as u64).to_le_bytes());
            // Zero inode means the provider did not supply an inode identifier.
            header[16..20].copy_from_slice(&(entry.name.len() as u32).to_le_bytes());
            header[20] = entry.kind as u8;
            for bytes in [&header[..], entry.name.as_bytes()] {
                let count = bytes.len().min(maximum - output.len());
                output.extend_from_slice(&bytes[..count]);
                if output.len() == maximum {
                    return Ok(output);
                }
            }
        }
        Ok(output)
    }
}

#[derive(Clone)]
pub(crate) struct Directory {
    pub filesystem: Rc<dyn FileSystem>,
    pub path: String,
    pub preopen: bool,
    pub rights: u64,
    pub inheriting: u64,
    pub entries: Rc<RefCell<Option<DirectoryEntries>>>,
    pub flags: u16,
}

impl Directory {
    pub fn read(&self, cookie: u64, maximum: usize) -> Result<Vec<u8>, i32> {
        if self.rights & (1 << 14) == 0 {
            return Err(76);
        }
        if maximum == 0 {
            return Ok(Vec::new());
        }
        let mut entries = self.entries.borrow_mut();
        if cookie == 0 {
            let mut listing = self
                .filesystem
                .read_directory(&self.path)
                .map_err(FileError::errno)?;
            listing.sort();
            *entries = Some(listing);
        }
        entries.as_ref().ok_or(28)?.encode(cookie, maximum)
    }

    pub fn resolve(&self, path: &str) -> Result<String, i32> {
        if path.is_empty() {
            return Err(44);
        }
        if path.starts_with('/') || path.contains(['\\', '\0']) {
            return Err(76); // NOTCAPABLE
        }
        let mut result = self.path.clone();
        let parts: alloc::vec::Vec<_> = path.split('/').collect();
        for (index, part) in parts.iter().enumerate() {
            match *part {
                "" | "." => {}
                ".." => {
                    // A directory descriptor is the path operation's root.
                    if result == self.path {
                        return Err(76);
                    }
                    result.truncate(result.rfind('/').unwrap_or(0));
                }
                name => {
                    if !result.is_empty() {
                        result.push('/');
                    }
                    result.push_str(name);
                }
            }
            if index + 1 < parts.len() || path.ends_with('/') {
                let metadata = self
                    .filesystem
                    .metadata(&result)
                    .map_err(FileError::errno)?;
                if !metadata.directory {
                    return Err(54);
                }
            }
        }
        Ok(result)
    }
}
