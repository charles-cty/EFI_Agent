#![no_std]
//! Filesystem interfaces shared by the guest runtime and workspace packaging.
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

use alloc::{boxed::Box, rc::Rc, string::String, vec::Vec};

pub mod namespace;
#[cfg(feature = "std")]
pub mod native;
pub mod text;

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
    NotEmpty,
    CrossDevice,
    Loop,
    NotCapable,
    ReadOnly,
    NoSpace,
    NotSeekable,
    Corrupt,
    NotPermitted,
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
            Self::NotEmpty => 55,
            Self::CrossDevice => 75,
            Self::Loop => 32,
            Self::NotCapable => 76,
            Self::ReadOnly => 69,
            Self::NoSpace => 51,
            Self::NotSeekable => 70,
            Self::Corrupt => 29,
            Self::NotPermitted => 63,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Metadata {
    pub device: u64,
    pub kind: EntryType,
    pub size: u64,
    /// Storage allocated to file data, excluding shared namespace journals.
    /// None means that the provider cannot measure allocation.
    pub allocated: Option<u64>,
    pub inode: u64,
    pub links: u64,
    pub mode: u16,
    pub accessed: u64,
    pub modified: u64,
    pub created: u64,
}

impl Metadata {
    pub fn new(kind: EntryType, size: u64) -> Self {
        Self {
            device: 0,
            kind,
            size,
            allocated: None,
            inode: 0,
            links: 1,
            mode: if kind == EntryType::Directory {
                0o755
            } else {
                0o644
            },
            accessed: 0,
            modified: 0,
            created: 0,
        }
    }

    pub fn is_directory(self) -> bool {
        self.kind == EntryType::Directory
    }

    pub fn wasi(self) -> [u8; 64] {
        let mut stat = [0; 64];
        stat[0..8].copy_from_slice(&self.device.to_le_bytes());
        stat[8..16].copy_from_slice(&self.inode.to_le_bytes());
        stat[16] = self.kind as u8;
        stat[24..32].copy_from_slice(&self.links.to_le_bytes());
        stat[32..40].copy_from_slice(&self.size.to_le_bytes());
        stat[40..48].copy_from_slice(&self.accessed.to_le_bytes());
        stat[48..56].copy_from_slice(&self.modified.to_le_bytes());
        stat[56..64].copy_from_slice(&self.created.to_le_bytes());
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
    fn filesystem_statistics(&self) -> Result<FileSystemStatistics, FileError> {
        Err(FileError::Unsupported)
    }
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, FileError>;
    fn write(&mut self, buffer: &[u8]) -> Result<usize, FileError>;
    fn seek(&mut self, offset: i64, whence: u32) -> Result<u64, FileError>;
    fn metadata(&mut self) -> Result<Metadata, FileError>;
    fn flush(&mut self) -> Result<(), FileError>;
    /// Synchronize stored data for an explicit fsync or fdatasync request.
    fn sync(&mut self, _data_only: bool) -> Result<(), FileError> {
        self.flush()
    }
    /// Change length without changing the cursor. Extended bytes are zero.
    fn resize(&mut self, size: u64) -> Result<(), FileError>;
    fn set_times(&mut self, _access: TimeUpdate, _modify: TimeUpdate) -> Result<(), FileError> {
        Err(FileError::Unsupported)
    }
    fn set_mode(&mut self, _mode: u16) -> Result<(), FileError> {
        Err(FileError::Unsupported)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum TimeUpdate {
    Keep,
    Now,
    Set(u64),
}

#[derive(Clone, Copy, Debug)]
pub struct Capacity {
    pub total: u64,
    pub free: u64,
    pub available: u64,
    pub block_size: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct FileSystemStatistics {
    pub capacity: Capacity,
    pub files: u64,
    pub files_free: u64,
    pub id: u32,
    pub kind: u32,
    pub name_max: u32,
    pub read_only: bool,
}

impl FileSystemStatistics {
    /// Fixed-width private statvfs reply, independent of the guest C ABI.
    pub fn encode(self) -> Result<[u8; 96], FileError> {
        let capacity = self.capacity;
        if capacity.block_size == 0
            || capacity.free > capacity.total
            || capacity.available > capacity.free
            || self.files_free > self.files
        {
            return Err(FileError::Invalid);
        }
        let block = u64::from(capacity.block_size);
        let fields = [
            block,
            block,
            capacity.total / block,
            capacity.free / block,
            capacity.available / block,
            self.files,
            self.files_free,
            self.files_free,
            u64::from(self.id),
            u64::from(self.read_only),
            u64::from(self.name_max),
            u64::from(self.kind),
        ];
        let mut bytes = [0; 96];
        for (chunk, field) in bytes.chunks_exact_mut(8).zip(fields) {
            chunk.copy_from_slice(&field.to_le_bytes());
        }
        Ok(bytes)
    }
}

#[derive(Clone, Debug)]
pub struct Mount {
    pub source: String,
    pub path: String,
    pub kind: String,
    pub read_only: bool,
}

/// Paths use slash-separated names relative to the provider's root.
/// Providers must preserve open-file identity across changes to path names.
pub trait FileSystem: core::any::Any {
    fn filesystem_statistics(&self, _path: &str) -> Result<FileSystemStatistics, FileError> {
        Err(FileError::Unsupported)
    }
    fn mounts(&self) -> Result<Vec<Mount>, FileError> {
        Err(FileError::Unsupported)
    }
    fn open(&self, path: &str, options: OpenOptions) -> Result<Box<dyn File>, FileError>;
    fn metadata(&self, path: &str) -> Result<Metadata, FileError>;
    fn read_directory(&self, path: &str) -> Result<DirectoryEntries, FileError>;
    /// Create one directory. The parent must exist; existing entries are errors.
    fn create_directory(&self, _path: &str) -> Result<(), FileError> {
        Err(FileError::Unsupported)
    }
    /// Move an entry within this filesystem, replacing a permitted destination.
    fn rename(&self, _source: &str, _destination: &str) -> Result<(), FileError> {
        Err(FileError::Unsupported)
    }
    fn unlink(&self, _path: &str) -> Result<(), FileError> {
        Err(FileError::Unsupported)
    }
    fn remove_directory(&self, _path: &str) -> Result<(), FileError> {
        Err(FileError::Unsupported)
    }
    fn symlink(&self, _target: &str, _path: &str) -> Result<(), FileError> {
        Err(FileError::Unsupported)
    }
    fn hard_link(&self, _source: &str, _destination: &str, _follow: bool) -> Result<(), FileError> {
        Err(FileError::Unsupported)
    }
    fn hard_link_to(
        &self,
        source: &str,
        other: &dyn FileSystem,
        destination: &str,
        follow: bool,
    ) -> Result<(), FileError> {
        if self.identity() != other.identity() {
            return Err(FileError::CrossDevice);
        }
        self.hard_link(source, destination, follow)
    }
    fn read_link(&self, path: &str) -> Result<String, FileError> {
        self.symlink_metadata(path)?;
        Err(FileError::Invalid)
    }
    fn symlink_metadata(&self, path: &str) -> Result<Metadata, FileError> {
        self.metadata(path)
    }
    fn set_times(
        &self,
        _path: &str,
        _follow: bool,
        _access: TimeUpdate,
        _modify: TimeUpdate,
    ) -> Result<(), FileError> {
        Err(FileError::Unsupported)
    }
    fn set_mode(&self, _path: &str, _mode: u16) -> Result<(), FileError> {
        Err(FileError::Unsupported)
    }
    fn capacity(&self) -> Result<Capacity, FileError> {
        Err(FileError::Unsupported)
    }
    /// An anchored view retains directory identity when its name changes.
    fn directory_view(&self, _path: &str) -> Result<Option<Rc<dyn FileSystem>>, FileError> {
        Ok(None)
    }
    fn identity(&self) -> usize {
        self as *const Self as *const () as usize
    }
    fn rename_to(
        &self,
        source: &str,
        other: &dyn FileSystem,
        destination: &str,
    ) -> Result<(), FileError> {
        if self.identity() != other.identity() {
            return Err(FileError::CrossDevice);
        }
        self.rename(source, destination)
    }
    /// Resolve a WASI path below an open directory. Raw providers reject links.
    fn resolve_path(&self, base: &str, path: &str) -> Result<String, FileError> {
        if path.is_empty() {
            return Err(FileError::NotFound);
        }
        if path.starts_with('/') || path.contains(['\\', '\0']) {
            return Err(FileError::NotCapable);
        }
        let mut result = String::from(base);
        let parts: Vec<_> = path.split('/').collect();
        for (index, part) in parts.iter().enumerate() {
            match *part {
                "" | "." => (),
                ".." => {
                    if result == base {
                        return Err(FileError::NotCapable);
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
            if (index + 1 < parts.len() || path.ends_with('/'))
                && !self.metadata(&result)?.is_directory()
            {
                return Err(FileError::NotDirectory);
            }
        }
        Ok(result)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum EntryType {
    Unknown = 0,
    CharacterDevice = 2,
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
    names: usize,
}

impl DirectoryEntries {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, EntryType)> {
        self.entries
            .iter()
            .map(|entry| (entry.name.as_str(), entry.kind))
    }

    pub fn push(&mut self, name: String, kind: EntryType) -> Result<(), FileError> {
        assert!(
            !name.is_empty() && !name.contains(['/', '\\', '\0']),
            "Host directory entry is not a single name"
        );
        let bytes = 24 + name.len();
        let ordinary = name != "." && name != "..";
        if (ordinary && self.names == MAX_DIRECTORY_ENTRIES)
            || self.entries.len() == MAX_DIRECTORY_ENTRIES + 2
            || bytes > MAX_DIRECTORY_BYTES - self.bytes
        {
            return Err(FileError::ResourceLimit);
        }
        self.entries.push(DirectoryEntry { name, kind });
        self.bytes += bytes;
        self.names += usize::from(ordinary);
        Ok(())
    }

    pub fn sort(&mut self) {
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

    pub fn encode(&self, cookie: u64, maximum: usize) -> Result<Vec<u8>, i32> {
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
