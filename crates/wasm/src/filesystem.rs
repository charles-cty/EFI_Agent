//! Runtime directory descriptors over the common filesystem interfaces.
use alloc::{boxed::Box, collections::BTreeMap, format, rc::Rc, string::String, vec::Vec};
use core::cell::RefCell;
pub use efi_agent_filesystem::*;

// Virtual entries have no independently allocated persistent data blocks.
fn virtual_metadata(kind: EntryType, size: u64) -> Metadata {
    let mut metadata = Metadata::new(kind, size);
    metadata.allocated = Some(0);
    metadata
}

mod walk;
use walk::Follow;

pub const TEMP_CAPACITY: u64 = 32 * 1024 * 1024;

fn special_statistics(device: u32) -> FileSystemStatistics {
    FileSystemStatistics {
        capacity: Capacity {
            total: 0,
            free: 0,
            available: 0,
            block_size: 1,
        },
        files: 3,
        files_free: 0,
        id: device,
        kind: if device == 2 { 0x1373 } else { 0x9fa0 },
        name_max: 1024,
        read_only: true,
    }
}
type Programs = Rc<RefCell<BTreeMap<String, Option<String>>>>;

/// Files and directories that exist in every guest without being persisted in
/// the workspace. The workspace remains the backing store for ordinary paths.
#[derive(Clone)]
pub struct VirtualFileSystem {
    base: Rc<dyn FileSystem>,
    tmp: Rc<namespace::Namespace>,
    mount: String,
    identity: usize,
    programs: Programs,
}

impl VirtualFileSystem {
    pub(crate) fn executable_path(&self, path: &str) -> Result<(String, Metadata), FileError> {
        let resolved = self.walk(path, Follow::Final, false)?;
        let metadata = self.metadata_raw(&resolved)?;
        Ok((resolved, metadata))
    }

    pub fn new(base: Rc<dyn FileSystem>) -> Rc<Self> {
        Self::with_programs(base, Rc::new(RefCell::new(BTreeMap::new())))
    }

    pub(crate) fn with_programs(base: Rc<dyn FileSystem>, programs: Programs) -> Rc<Self> {
        let identity = base.identity();
        Rc::new(Self {
            base,
            tmp: Rc::new(namespace::Namespace::temporary(
                TEMP_CAPACITY,
                MAX_DIRECTORY_ENTRIES + 1,
                3,
                None,
            )),
            mount: String::new(),
            identity,
            programs,
        })
    }

    pub(crate) fn validate_programs(
        &self,
        programs: &BTreeMap<String, Option<String>>,
    ) -> Result<(), FileError> {
        match self.base.symlink_metadata("bin") {
            Err(FileError::NotFound) => return Ok(()),
            Ok(info) if info.kind == EntryType::Directory => {}
            Ok(_) => return Err(FileError::Exists),
            Err(error) => return Err(error),
        }
        for name in programs.keys() {
            match self.base.symlink_metadata(&format!("bin/{name}")) {
                Err(FileError::NotFound) => {}
                Ok(_) => return Err(FileError::Exists),
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    fn program(&self, path: &str) -> Option<Option<String>> {
        self.programs
            .borrow()
            .get(path.strip_prefix("bin/")?)
            .cloned()
    }

    fn program_metadata(&self, path: &str) -> Option<Metadata> {
        let source = self.program(path)?;
        let mut metadata = source
            .and_then(|source| self.base.metadata(&source).ok())
            .unwrap_or_else(|| virtual_metadata(EntryType::File, 0));
        metadata.mode = 0o555;
        Some(metadata)
    }

    fn virtual_directory(path: &str) -> bool {
        matches!(path, "dev" | "tmp" | "proc" | "bin")
    }

    fn virtual_entry(path: &str) -> Option<EntryType> {
        match path {
            "dev/null" | "dev/zero" | "proc/self" | "proc/uptime" => Some(EntryType::File),
            "dev" | "tmp" | "proc" | "bin" => Some(EntryType::Directory),
            _ => None,
        }
    }
}

struct VirtualFile {
    data: Vec<u8>,
    offset: usize,
    zero: bool,
    discard: bool,
    device: u32,
}

struct ProgramFile(Box<dyn File>);

impl File for ProgramFile {
    fn filesystem_statistics(&self) -> Result<FileSystemStatistics, FileError> {
        self.0.filesystem_statistics()
    }
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, FileError> {
        self.0.read(buffer)
    }
    fn write(&mut self, _buffer: &[u8]) -> Result<usize, FileError> {
        Err(FileError::ReadOnly)
    }
    fn seek(&mut self, offset: i64, whence: u32) -> Result<u64, FileError> {
        self.0.seek(offset, whence)
    }
    fn metadata(&mut self) -> Result<Metadata, FileError> {
        let mut info = self.0.metadata()?;
        info.mode = 0o555;
        Ok(info)
    }
    fn flush(&mut self) -> Result<(), FileError> {
        self.0.flush()
    }
    fn resize(&mut self, _size: u64) -> Result<(), FileError> {
        Err(FileError::ReadOnly)
    }
}

impl File for VirtualFile {
    fn filesystem_statistics(&self) -> Result<FileSystemStatistics, FileError> {
        Ok(special_statistics(self.device))
    }
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, FileError> {
        if self.zero {
            buffer.fill(0);
            return Ok(buffer.len());
        }
        let count = buffer
            .len()
            .min(self.data.len().saturating_sub(self.offset));
        if count == 0 {
            return Ok(0);
        }
        buffer[..count].copy_from_slice(&self.data[self.offset..self.offset + count]);
        self.offset += count;
        Ok(count)
    }

    fn write(&mut self, buffer: &[u8]) -> Result<usize, FileError> {
        if self.discard {
            return Ok(buffer.len());
        }
        Err(FileError::ReadOnly)
    }

    fn seek(&mut self, offset: i64, whence: u32) -> Result<u64, FileError> {
        let base = match whence {
            0 => 0,
            1 => self.offset as i64,
            2 => self.data.len() as i64,
            _ => return Err(FileError::Invalid),
        };
        let next = base.checked_add(offset).ok_or(FileError::Overflow)?;
        if next < 0 {
            return Err(FileError::Invalid);
        }
        self.offset = next as usize;
        Ok(self.offset as u64)
    }

    fn metadata(&mut self) -> Result<Metadata, FileError> {
        let mut metadata = virtual_metadata(EntryType::File, self.data.len() as u64);
        metadata.device = u64::from(self.device);
        Ok(metadata)
    }

    fn flush(&mut self) -> Result<(), FileError> {
        Ok(())
    }

    fn resize(&mut self, _size: u64) -> Result<(), FileError> {
        Err(FileError::ReadOnly)
    }
}

impl FileSystem for VirtualFileSystem {
    fn open(&self, path: &str, options: OpenOptions) -> Result<Box<dyn File>, FileError> {
        let follow = if options.create && options.exclusive {
            Follow::Parent
        } else {
            Follow::Final
        };
        self.open_raw(&self.walk(path, follow, options.create)?, options)
    }

    fn metadata(&self, path: &str) -> Result<Metadata, FileError> {
        self.metadata_raw(&self.walk(path, Follow::Final, false)?)
    }

    fn symlink_metadata(&self, path: &str) -> Result<Metadata, FileError> {
        self.symlink_metadata_raw(&self.walk(path, Follow::Inspect, false)?)
    }

    fn read_directory(&self, path: &str) -> Result<DirectoryEntries, FileError> {
        self.read_directory_raw(&self.walk(path, Follow::Final, false)?)
    }

    fn create_directory(&self, path: &str) -> Result<(), FileError> {
        self.create_directory_raw(&self.walk(path, Follow::Parent, true)?)
    }

    fn unlink(&self, path: &str) -> Result<(), FileError> {
        self.unlink_raw(&self.walk(path, Follow::Parent, false)?)
    }

    fn remove_directory(&self, path: &str) -> Result<(), FileError> {
        self.remove_directory_raw(&self.walk(path, Follow::Parent, false)?)
    }

    fn rename(&self, source: &str, destination: &str) -> Result<(), FileError> {
        self.rename_to(source, self, destination)
    }

    fn rename_to(
        &self,
        source: &str,
        other: &dyn FileSystem,
        destination: &str,
    ) -> Result<(), FileError> {
        let source = self.walk(source, Follow::Parent, false)?;
        let destination = match (other as &dyn core::any::Any).downcast_ref::<Self>() {
            Some(other) => other.walk(destination, Follow::Parent, true)?,
            None => destination.into(),
        };
        self.rename_to_raw(&source, other, &destination)
    }

    fn symlink(&self, target: &str, path: &str) -> Result<(), FileError> {
        self.symlink_raw(target, &self.walk(path, Follow::Parent, true)?)
    }

    fn read_link(&self, path: &str) -> Result<String, FileError> {
        self.read_link_raw(&self.walk(path, Follow::Inspect, false)?)
    }

    fn hard_link(&self, source: &str, destination: &str, follow: bool) -> Result<(), FileError> {
        self.hard_link_to(source, self, destination, follow)
    }

    fn hard_link_to(
        &self,
        source: &str,
        other: &dyn FileSystem,
        destination: &str,
        follow: bool,
    ) -> Result<(), FileError> {
        let source = self.walk(
            source,
            if follow {
                Follow::Final
            } else {
                Follow::Inspect
            },
            false,
        )?;
        let destination = match (other as &dyn core::any::Any).downcast_ref::<Self>() {
            Some(other) => other.walk(destination, Follow::Parent, true)?,
            None => destination.into(),
        };
        self.hard_link_to_raw(&source, other, &destination, follow)
    }

    fn set_mode(&self, path: &str, mode: u16) -> Result<(), FileError> {
        self.set_mode_raw(&self.walk(path, Follow::Final, false)?, mode)
    }

    fn set_times(
        &self,
        path: &str,
        follow: bool,
        access: TimeUpdate,
        modify: TimeUpdate,
    ) -> Result<(), FileError> {
        self.set_times_raw(
            &self.walk(
                path,
                if follow {
                    Follow::Final
                } else {
                    Follow::Inspect
                },
                false,
            )?,
            follow,
            access,
            modify,
        )
    }

    fn directory_view(&self, path: &str) -> Result<Option<Rc<dyn FileSystem>>, FileError> {
        self.directory_view_raw(&self.walk(path, Follow::Final, false)?)
    }

    fn resolve_path(&self, base: &str, path: &str) -> Result<String, FileError> {
        if path.is_empty() {
            return Err(FileError::NotFound);
        }
        if path.starts_with('/') || path.contains(['\\', '\0']) {
            return Err(FileError::NotCapable);
        }
        let path = if base.is_empty() {
            path.into()
        } else {
            format!("{base}/{path}")
        };
        if path.len() > walk::MAX_PATH {
            return Err(FileError::ResourceLimit);
        }
        Ok(path)
    }

    fn capacity(&self) -> Result<Capacity, FileError> {
        self.capacity_raw()
    }

    fn filesystem_statistics(&self, path: &str) -> Result<FileSystemStatistics, FileError> {
        self.filesystem_statistics_raw(&self.walk(path, Follow::Final, false)?)
    }

    fn mounts(&self) -> Result<Vec<Mount>, FileError> {
        self.mounts_raw()
    }

    fn identity(&self) -> usize {
        self.identity_raw()
    }
}

impl VirtualFileSystem {
    fn open_raw(&self, path: &str, options: OpenOptions) -> Result<Box<dyn File>, FileError> {
        let path = self.mounted(path);
        let path = path.as_str();
        if let Some(source) = self.program(path) {
            if options.write || options.create || options.truncate || options.append {
                return Err(FileError::ReadOnly);
            }
            return Ok(Box::new(ProgramFile(
                self.base
                    .open(&source.ok_or(FileError::Unsupported)?, options)?,
            )));
        }
        if let Some(path) = Self::temporary_path(path) {
            return self.tmp.open(path, options);
        }
        if let Some(kind) = Self::virtual_entry(path) {
            if kind == EntryType::Directory {
                return Err(FileError::IsDirectory);
            }
            if (options.write || options.create || options.truncate || options.append)
                && path != "dev/null"
            {
                return Err(FileError::ReadOnly);
            }
            let (data, zero, discard) = match path {
                "dev/null" => (Vec::new(), false, true),
                "dev/zero" => (Vec::new(), true, false),
                "proc/uptime" => (b"0.00 0.00\n".to_vec(), false, false),
                _ => (Vec::new(), false, false),
            };
            return Ok(Box::new(VirtualFile {
                data,
                offset: 0,
                zero,
                discard,
                device: if path.starts_with("dev/") { 2 } else { 4 },
            }));
        }
        self.base.open(path, options)
    }

    fn metadata_raw(&self, path: &str) -> Result<Metadata, FileError> {
        let path = self.mounted(path);
        let path = path.as_str();
        if let Some(metadata) = self.program_metadata(path) {
            return Ok(metadata);
        }
        if let Some(path) = Self::temporary_path(path) {
            return self.tmp.metadata(path);
        }
        if let Some(kind) = Self::virtual_entry(path) {
            let mut metadata = virtual_metadata(kind, 0);
            metadata.device = match path.split('/').next() {
                Some("dev") => 2,
                Some("tmp") => 3,
                Some("proc") => 4,
                _ => 0,
            };
            return Ok(metadata);
        }
        self.base.metadata(path)
    }

    fn read_directory_raw(&self, path: &str) -> Result<DirectoryEntries, FileError> {
        let path = self.mounted(path);
        let path = path.as_str();
        if let Some(path) = Self::temporary_path(path) {
            return self.tmp.read_directory(path);
        }
        let mut entries = if path == "bin" {
            match self.base.read_directory(path) {
                Ok(entries) => entries,
                Err(FileError::NotFound) => DirectoryEntries::new(),
                Err(error) => return Err(error),
            }
        } else if path.is_empty() || !Self::virtual_directory(path) {
            self.base.read_directory(path)?
        } else {
            DirectoryEntries::new()
        };
        if path.is_empty() {
            for name in ["bin", "dev", "proc", "tmp"] {
                if !entries.iter().any(|(entry, _)| entry == name) {
                    entries.push(name.into(), EntryType::Directory)?;
                }
            }
        } else if Self::virtual_directory(path) {
            if path == "bin" {
                for (name, source) in self.programs.borrow().iter() {
                    entries.push(
                        name.clone(),
                        if source.is_some() {
                            EntryType::Symlink
                        } else {
                            EntryType::File
                        },
                    )?;
                }
            } else if path == "dev" {
                entries.push("null".into(), EntryType::File)?;
                entries.push("zero".into(), EntryType::File)?;
            } else if path == "proc" {
                entries.push("self".into(), EntryType::File)?;
                entries.push("uptime".into(), EntryType::File)?;
            }
        }
        Ok(entries)
    }

    fn create_directory_raw(&self, path: &str) -> Result<(), FileError> {
        let path = self.mounted(path);
        let path = path.as_str();
        if let Some(path) = Self::temporary_path(path) {
            return self.tmp.create_directory(path);
        }
        if Self::virtual_directory(path)
            || self.program(path).is_some()
            || path.starts_with("dev/")
            || path.starts_with("proc/")
        {
            return Err(FileError::ReadOnly);
        }
        self.base.create_directory(path)
    }

    fn unlink_raw(&self, path: &str) -> Result<(), FileError> {
        let path = self.mounted(path);
        let path = path.as_str();
        if let Some(path) = Self::temporary_path(path) {
            return self.tmp.unlink(path);
        }
        if Self::virtual_entry(path).is_some() || self.program(path).is_some() {
            return Err(FileError::ReadOnly);
        }
        self.base.unlink(path)
    }

    fn remove_directory_raw(&self, path: &str) -> Result<(), FileError> {
        let path = self.mounted(path);
        let path = path.as_str();
        if let Some(path) = Self::temporary_path(path) {
            return self.tmp.remove_directory(path);
        }
        if Self::virtual_directory(path) {
            return Err(FileError::ReadOnly);
        }
        self.base.remove_directory(path)
    }

    fn symlink_raw(&self, target: &str, path: &str) -> Result<(), FileError> {
        let path = self.mounted(path);
        let path = path.as_str();
        if let Some(path) = Self::temporary_path(path) {
            return self.tmp.symlink(target, path);
        }
        if Self::virtual_entry(path).is_some() || self.program(path).is_some() {
            return Err(FileError::ReadOnly);
        }
        self.base.symlink(target, path)
    }

    fn read_link_raw(&self, path: &str) -> Result<String, FileError> {
        let path = self.mounted(path);
        if let Some(path) = Self::temporary_path(&path) {
            return self.tmp.read_link(path);
        }
        if let Some(source) = self.program(&path) {
            return source
                .map(|source| format!("/{source}"))
                .ok_or(FileError::Invalid);
        }
        self.base.read_link(&path)
    }

    fn hard_link_to_raw(
        &self,
        source: &str,
        other: &dyn FileSystem,
        destination: &str,
        follow: bool,
    ) -> Result<(), FileError> {
        let source = self.mounted(source);
        let other_virtual = (other as &dyn core::any::Any).downcast_ref::<Self>();
        let destination =
            other_virtual.map_or_else(|| destination.into(), |other| other.mounted(destination));
        if let Some(source) = Self::temporary_path(&source) {
            return match other_virtual {
                Some(other) => self.tmp.hard_link_to(
                    source,
                    other.tmp.as_ref(),
                    Self::temporary_path(&destination).ok_or(FileError::CrossDevice)?,
                    follow,
                ),
                None => self.tmp.hard_link_to(source, other, &destination, follow),
            };
        }
        let special = |path: &str| matches!(path.split('/').next(), Some("dev" | "tmp" | "proc"));
        if special(&source) || special(&destination) {
            return Err(FileError::CrossDevice);
        }
        if self.program(&source).is_some()
            || other_virtual.is_some_and(|other| other.program(&destination).is_some())
            || Self::virtual_directory(&source)
            || Self::virtual_directory(&destination)
        {
            return Err(FileError::ReadOnly);
        }
        self.base.hard_link_to(
            &source,
            other_virtual.map_or(other, |other| other.base.as_ref()),
            &destination,
            follow,
        )
    }

    fn symlink_metadata_raw(&self, path: &str) -> Result<Metadata, FileError> {
        let path = self.mounted(path);
        let path = path.as_str();
        if let Some(path) = Self::temporary_path(path) {
            return self.tmp.symlink_metadata(path);
        }
        if let Some(source) = self.program(path) {
            return Ok(match source {
                Some(source) => {
                    let mut info = virtual_metadata(EntryType::Symlink, (source.len() + 1) as u64);
                    info.mode = 0o777;
                    info
                }
                None => self
                    .program_metadata(path)
                    .expect("Registered executable has metadata"),
            });
        }
        if Self::virtual_entry(path).is_some() {
            let relative = if self.mount.is_empty() {
                path
            } else {
                path.strip_prefix(&self.mount)
                    .unwrap()
                    .trim_start_matches('/')
            };
            return self.metadata_raw(relative);
        }
        self.base.symlink_metadata(path)
    }

    fn set_times_raw(
        &self,
        path: &str,
        follow: bool,
        access: TimeUpdate,
        modify: TimeUpdate,
    ) -> Result<(), FileError> {
        let path = self.mounted(path);
        let path = path.as_str();
        if let Some(path) = Self::temporary_path(path) {
            return self.tmp.set_times(path, follow, access, modify);
        }
        if Self::virtual_entry(path).is_some() || self.program(path).is_some() {
            return Err(FileError::ReadOnly);
        }
        self.base.set_times(path, follow, access, modify)
    }

    fn set_mode_raw(&self, path: &str, mode: u16) -> Result<(), FileError> {
        let path = self.mounted(path);
        let path = path.as_str();
        if let Some(path) = Self::temporary_path(path) {
            return self.tmp.set_mode(path, mode);
        }
        if Self::virtual_entry(path).is_some() || self.program(path).is_some() {
            return Err(FileError::ReadOnly);
        }
        self.base.set_mode(path, mode)
    }

    fn capacity_raw(&self) -> Result<Capacity, FileError> {
        if self.mount == "tmp" {
            self.tmp.capacity()
        } else {
            self.base.capacity()
        }
    }

    fn filesystem_statistics_raw(&self, path: &str) -> Result<FileSystemStatistics, FileError> {
        self.metadata_raw(path)?;
        let path = self.mounted(path);
        match path.split('/').next() {
            Some("tmp") => self
                .tmp
                .filesystem_statistics(Self::temporary_path(&path).unwrap()),
            Some("dev") => Ok(special_statistics(2)),
            Some("proc") => Ok(special_statistics(4)),
            _ => self.base.filesystem_statistics(""),
        }
    }

    fn mounts_raw(&self) -> Result<Vec<Mount>, FileError> {
        let mut mounts = self.base.mounts()?;
        for (source, path, kind, read_only) in [
            ("devfs", "/dev", "devfs", true),
            ("tmpfs", "/tmp", "tmpfs", false),
            ("proc", "/proc", "proc", true),
        ] {
            mounts.push(Mount {
                source: source.into(),
                path: path.into(),
                kind: kind.into(),
                read_only,
            });
        }
        Ok(mounts)
    }

    fn identity_raw(&self) -> usize {
        self.identity
    }

    fn rename_to_raw(
        &self,
        source: &str,
        other: &dyn FileSystem,
        destination: &str,
    ) -> Result<(), FileError> {
        if let Some(source) = Self::temporary_path(&self.mounted(source)) {
            return match (other as &dyn core::any::Any).downcast_ref::<Self>() {
                Some(other) => self.tmp.rename_to(
                    source,
                    other.tmp.as_ref(),
                    Self::temporary_path(&other.mounted(destination))
                        .ok_or(FileError::CrossDevice)?,
                ),
                None => self.tmp.rename_to(source, other, destination),
            };
        }
        let Some(other) = (other as &dyn core::any::Any).downcast_ref::<Self>() else {
            let source = self.mounted(source);
            if Self::virtual_directory(source.split('/').next().unwrap_or("")) {
                return Err(FileError::CrossDevice);
            }
            return self.base.rename_to(&source, other, destination);
        };
        if self.identity != other.identity {
            return Err(FileError::CrossDevice);
        }
        let source = self.mounted(source);
        let destination = other.mounted(destination);
        if Self::temporary_path(&destination).is_some() {
            return Err(FileError::CrossDevice);
        }
        if Self::virtual_entry(&source).is_some()
            || Self::virtual_entry(&destination).is_some()
            || self.program(&source).is_some()
            || other.program(&destination).is_some()
        {
            return Err(FileError::ReadOnly);
        }
        self.base
            .rename_to(&source, other.base.as_ref(), &destination)
    }

    fn directory_view_raw(&self, path: &str) -> Result<Option<Rc<dyn FileSystem>>, FileError> {
        if path.is_empty() && self.mount.is_empty() {
            return Ok(Some(Rc::new(self.clone())));
        }
        let path = self.mounted(path);
        let path = path.as_str();
        if let Some(path) = Self::temporary_path(path) {
            let view = self
                .tmp
                .directory_view(path)?
                .expect("Namespace anchors directories");
            let namespace = (view.as_ref() as &dyn core::any::Any)
                .downcast_ref::<namespace::Namespace>()
                .expect("Namespace returns its own directory views")
                .clone();
            return Ok(Some(Rc::new(Self {
                base: self.base.clone(),
                tmp: Rc::new(namespace),
                mount: "tmp".into(),
                identity: self.identity,
                programs: self.programs.clone(),
            })));
        }
        if Self::virtual_directory(path) {
            return Ok(Some(Rc::new(VirtualFileSystem {
                base: self.base.clone(),
                tmp: self.tmp.clone(),
                mount: path.into(),
                identity: self.identity,
                programs: self.programs.clone(),
            })));
        }
        self.base.directory_view(path)
    }
}

impl VirtualFileSystem {
    fn temporary_path(path: &str) -> Option<&str> {
        if path == "tmp" {
            Some("")
        } else {
            path.strip_prefix("tmp/")
        }
    }

    pub(crate) fn set_clock(&self, clock: Rc<dyn crate::clock::Clock>) {
        self.tmp.set_clock(Some(Rc::new(move || {
            clock
                .now(crate::clock::ClockId::Realtime)
                .map_err(|error| match error {
                    crate::clock::ClockError::InvalidClock => FileError::Invalid,
                    crate::clock::ClockError::Unavailable => FileError::Unsupported,
                    crate::clock::ClockError::Io => FileError::Io,
                    crate::clock::ClockError::Overflow => FileError::Overflow,
                })
        })));
    }

    fn mounted(&self, path: &str) -> String {
        if self.mount.is_empty() {
            path.into()
        } else if path.is_empty() {
            self.mount.clone()
        } else {
            format!("{}/{}", self.mount, path)
        }
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
        let path = if self.preopen {
            let relative = path.trim_start_matches('/');
            if !path.is_empty() && relative.is_empty() {
                "."
            } else {
                relative
            }
        } else {
            path
        };
        self.filesystem
            .resolve_path(&self.path, path)
            .map_err(FileError::errno)
    }
}
