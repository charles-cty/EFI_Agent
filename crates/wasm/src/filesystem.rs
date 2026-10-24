//! Runtime directory descriptors over the common filesystem interfaces.
use alloc::{
    boxed::Box,
    collections::BTreeMap,
    format,
    rc::{Rc, Weak},
    string::String,
    vec::Vec,
};
use core::cell::RefCell;
pub use efi_agent_filesystem::*;

// Virtual entries have no independently allocated persistent data blocks.
fn virtual_metadata(kind: EntryType, size: u64) -> Metadata {
    let mut metadata = Metadata::new(kind, size);
    metadata.allocated = Some(0);
    metadata
}

pub const TEMP_CAPACITY: u64 = 32 * 1024 * 1024;

#[derive(Default)]
struct TempFiles {
    names: BTreeMap<String, Rc<RefCell<Vec<u8>>>>,
    objects: Vec<Weak<RefCell<Vec<u8>>>>,
}

impl TempFiles {
    fn statistics(&self) -> FileSystemStatistics {
        let mut used = 0;
        let mut files = 0;
        for file in self.objects.iter().filter_map(Weak::upgrade) {
            used += file.borrow().len() as u64;
            files += 1;
        }
        FileSystemStatistics {
            capacity: Capacity {
                total: TEMP_CAPACITY,
                free: TEMP_CAPACITY - used,
                available: TEMP_CAPACITY - used,
                block_size: 1,
            },
            files: MAX_DIRECTORY_ENTRIES as u64 + 1,
            files_free: MAX_DIRECTORY_ENTRIES as u64 - files,
            id: 3,
            kind: 0x01021994,
            name_max: 1024,
            read_only: false,
        }
    }
}

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
pub struct VirtualFileSystem {
    base: Rc<dyn FileSystem>,
    tmp: Rc<RefCell<TempFiles>>,
    mount: String,
    identity: usize,
    programs: Programs,
}

impl VirtualFileSystem {
    pub fn new(base: Rc<dyn FileSystem>) -> Rc<Self> {
        Self::with_programs(base, Rc::new(RefCell::new(BTreeMap::new())))
    }

    pub(crate) fn with_programs(base: Rc<dyn FileSystem>, programs: Programs) -> Rc<Self> {
        let identity = base.identity();
        Rc::new(Self {
            base,
            tmp: Rc::new(RefCell::new(TempFiles::default())),
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

struct TmpFile {
    data: Rc<RefCell<Vec<u8>>>,
    storage: Rc<RefCell<TempFiles>>,
    offset: usize,
    write: bool,
    append: bool,
}

impl File for TmpFile {
    fn filesystem_statistics(&self) -> Result<FileSystemStatistics, FileError> {
        Ok(self.storage.borrow().statistics())
    }
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, FileError> {
        let data = self.data.borrow();
        let count = buffer.len().min(data.len().saturating_sub(self.offset));
        if count == 0 {
            return Ok(0);
        }
        buffer[..count].copy_from_slice(&data[self.offset..self.offset + count]);
        self.offset += count;
        Ok(count)
    }
    fn write(&mut self, buffer: &[u8]) -> Result<usize, FileError> {
        if !self.write {
            return Err(FileError::ReadOnly);
        }
        if buffer.is_empty() {
            return Ok(0);
        }
        let length = self.data.borrow().len();
        if self.append {
            self.offset = length;
        }
        let end = self
            .offset
            .checked_add(buffer.len())
            .ok_or(FileError::FileTooLarge)?;
        if end as u64 > MAX_RESIZE_BYTES {
            return Err(FileError::FileTooLarge);
        }
        let available = self.storage.borrow().statistics().capacity.available as usize;
        let count = buffer
            .len()
            .min(length.saturating_add(available).saturating_sub(self.offset));
        if count == 0 {
            return Err(FileError::NoSpace);
        }
        let end = self.offset + count;
        let mut data = self.data.borrow_mut();
        if end > data.len() {
            let growth = end - data.len();
            data.try_reserve_exact(growth)
                .map_err(|_| FileError::NoSpace)?;
            data.resize(end, 0);
        }
        data[self.offset..end].copy_from_slice(&buffer[..count]);
        self.offset = end;
        Ok(count)
    }
    fn seek(&mut self, offset: i64, whence: u32) -> Result<u64, FileError> {
        let length = self.data.borrow().len() as i64;
        let base = match whence {
            0 => 0,
            1 => self.offset as i64,
            2 => length,
            _ => return Err(FileError::Invalid),
        };
        let next = base
            .checked_add(offset)
            .filter(|value| *value >= 0)
            .ok_or(FileError::Invalid)?;
        self.offset = next as usize;
        Ok(next as u64)
    }
    fn metadata(&mut self) -> Result<Metadata, FileError> {
        let mut metadata = virtual_metadata(EntryType::File, self.data.borrow().len() as u64);
        metadata.device = 3;
        metadata.allocated = Some(metadata.size);
        Ok(metadata)
    }
    fn flush(&mut self) -> Result<(), FileError> {
        Ok(())
    }
    fn resize(&mut self, size: u64) -> Result<(), FileError> {
        if !self.write {
            return Err(FileError::ReadOnly);
        }
        if size > MAX_RESIZE_BYTES {
            return Err(FileError::FileTooLarge);
        }
        let growth = size.saturating_sub(self.data.borrow().len() as u64);
        if growth > self.storage.borrow().statistics().capacity.available {
            return Err(FileError::NoSpace);
        }
        let size = usize::try_from(size).map_err(|_| FileError::Overflow)?;
        let mut data = self.data.borrow_mut();
        if size > data.len() {
            let growth = size - data.len();
            data.try_reserve_exact(growth)
                .map_err(|_| FileError::NoSpace)?;
        }
        data.resize(size, 0);
        data.shrink_to_fit();
        Ok(())
    }
}

impl FileSystem for VirtualFileSystem {
    fn open(&self, path: &str, options: OpenOptions) -> Result<Box<dyn File>, FileError> {
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
        if let Some(name) = path.strip_prefix("tmp/") {
            if name.is_empty() || name.contains('/') {
                return Err(FileError::NotFound);
            }
            let mut files = self.tmp.borrow_mut();
            let data = match files.names.get(name) {
                Some(_data) if options.exclusive => return Err(FileError::Exists),
                Some(data) => data.clone(),
                None if options.create => {
                    files.objects.retain(|file| file.strong_count() != 0);
                    if files.objects.len() >= MAX_DIRECTORY_ENTRIES {
                        return Err(FileError::NoSpace);
                    }
                    let data = Rc::new(RefCell::new(Vec::new()));
                    files.objects.push(Rc::downgrade(&data));
                    files.names.insert(name.into(), data.clone());
                    data
                }
                None => return Err(FileError::NotFound),
            };
            if options.truncate {
                data.borrow_mut().clear();
                data.borrow_mut().shrink_to_fit();
            }
            return Ok(Box::new(TmpFile {
                data,
                storage: self.tmp.clone(),
                offset: 0,
                write: options.write,
                append: options.append,
            }));
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

    fn metadata(&self, path: &str) -> Result<Metadata, FileError> {
        let path = self.mounted(path);
        let path = path.as_str();
        if let Some(metadata) = self.program_metadata(path) {
            return Ok(metadata);
        }
        if let Some(name) = path.strip_prefix("tmp/") {
            let data = self
                .tmp
                .borrow()
                .names
                .get(name)
                .cloned()
                .ok_or(FileError::NotFound)?;
            let mut metadata = virtual_metadata(EntryType::File, data.borrow().len() as u64);
            metadata.device = 3;
            metadata.allocated = Some(metadata.size);
            return Ok(metadata);
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

    fn read_directory(&self, path: &str) -> Result<DirectoryEntries, FileError> {
        let path = self.mounted(path);
        let path = path.as_str();
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
            } else if path == "tmp" {
                for name in self.tmp.borrow().names.keys() {
                    entries.push(name.clone(), EntryType::File)?;
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

    fn create_directory(&self, path: &str) -> Result<(), FileError> {
        let path = self.mounted(path);
        let path = path.as_str();
        if Self::virtual_directory(path)
            || self.program(path).is_some()
            || path.starts_with("dev/")
            || path.starts_with("proc/")
        {
            return Err(FileError::ReadOnly);
        }
        self.base.create_directory(path)
    }

    fn rename(&self, source: &str, destination: &str) -> Result<(), FileError> {
        let source = self.mounted(source);
        let destination = self.mounted(destination);
        let source = source.as_str();
        let destination = destination.as_str();
        if let (Some(source), Some(destination)) = (
            source.strip_prefix("tmp/"),
            destination.strip_prefix("tmp/"),
        ) {
            let mut files = self.tmp.borrow_mut();
            if source == destination {
                return files
                    .names
                    .contains_key(source)
                    .then_some(())
                    .ok_or(FileError::NotFound);
            }
            let data = files.names.remove(source).ok_or(FileError::NotFound)?;
            files.names.insert(destination.into(), data);
            return Ok(());
        }
        if Self::virtual_entry(source).is_some()
            || Self::virtual_entry(destination).is_some()
            || self.program(source).is_some()
            || self.program(destination).is_some()
        {
            return Err(FileError::ReadOnly);
        }
        self.base.rename(source, destination)
    }

    fn unlink(&self, path: &str) -> Result<(), FileError> {
        let path = self.mounted(path);
        let path = path.as_str();
        if let Some(name) = path.strip_prefix("tmp/") {
            return self
                .tmp
                .borrow_mut()
                .names
                .remove(name)
                .map(|_| ())
                .ok_or(FileError::NotFound);
        }
        if Self::virtual_entry(path).is_some() || self.program(path).is_some() {
            return Err(FileError::ReadOnly);
        }
        self.base.unlink(path)
    }

    fn remove_directory(&self, path: &str) -> Result<(), FileError> {
        let path = self.mounted(path);
        let path = path.as_str();
        if Self::virtual_directory(path) {
            return Err(FileError::ReadOnly);
        }
        self.base.remove_directory(path)
    }

    fn symlink(&self, target: &str, path: &str) -> Result<(), FileError> {
        let path = self.mounted(path);
        let path = path.as_str();
        if Self::virtual_entry(path).is_some() || self.program(path).is_some() {
            return Err(FileError::ReadOnly);
        }
        self.base.symlink(target, path)
    }

    fn read_link(&self, path: &str) -> Result<String, FileError> {
        let path = self.mounted(path);
        if let Some(source) = self.program(&path) {
            return source
                .map(|source| format!("/{source}"))
                .ok_or(FileError::Invalid);
        }
        self.base.read_link(&path)
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
        let source = self.mounted(source);
        let other_virtual = (other as &dyn core::any::Any).downcast_ref::<Self>();
        let destination =
            other_virtual.map_or_else(|| destination.into(), |other| other.mounted(destination));
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

    fn symlink_metadata(&self, path: &str) -> Result<Metadata, FileError> {
        let path = self.mounted(path);
        let path = path.as_str();
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
        if Self::virtual_entry(path).is_some() || path.starts_with("tmp/") {
            let relative = if self.mount.is_empty() {
                path
            } else {
                path.strip_prefix(&self.mount)
                    .unwrap()
                    .trim_start_matches('/')
            };
            return self.metadata(relative);
        }
        self.base.symlink_metadata(path)
    }

    fn set_times(
        &self,
        path: &str,
        follow: bool,
        access: TimeUpdate,
        modify: TimeUpdate,
    ) -> Result<(), FileError> {
        let path = self.mounted(path);
        let path = path.as_str();
        if Self::virtual_entry(path).is_some() || self.program(path).is_some() {
            return Err(FileError::ReadOnly);
        }
        self.base.set_times(path, follow, access, modify)
    }

    fn set_mode(&self, path: &str, mode: u16) -> Result<(), FileError> {
        let path = self.mounted(path);
        let path = path.as_str();
        if Self::virtual_entry(path).is_some() || self.program(path).is_some() {
            return Err(FileError::ReadOnly);
        }
        self.base.set_mode(path, mode)
    }

    fn capacity(&self) -> Result<Capacity, FileError> {
        self.base.capacity()
    }

    fn filesystem_statistics(&self, path: &str) -> Result<FileSystemStatistics, FileError> {
        self.metadata(path)?;
        let path = self.mounted(path);
        match path.split('/').next() {
            Some("tmp") => Ok(self.tmp.borrow().statistics()),
            Some("dev") => Ok(special_statistics(2)),
            Some("proc") => Ok(special_statistics(4)),
            _ => self.base.filesystem_statistics(""),
        }
    }

    fn mounts(&self) -> Result<Vec<Mount>, FileError> {
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

    fn identity(&self) -> usize {
        self.identity
    }

    fn rename_to(
        &self,
        source: &str,
        other: &dyn FileSystem,
        destination: &str,
    ) -> Result<(), FileError> {
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
        if source.starts_with("tmp/") || destination.starts_with("tmp/") {
            if !Rc::ptr_eq(&self.tmp, &other.tmp) {
                return Err(FileError::CrossDevice);
            }
            if !source.starts_with("tmp/") || !destination.starts_with("tmp/") {
                return Err(FileError::CrossDevice);
            }
            let source = source.strip_prefix("tmp/").unwrap();
            let destination = destination.strip_prefix("tmp/").unwrap();
            let mut files = self.tmp.borrow_mut();
            if source == destination {
                return files
                    .names
                    .contains_key(source)
                    .then_some(())
                    .ok_or(FileError::NotFound);
            }
            let data = files.names.remove(source).ok_or(FileError::NotFound)?;
            files.names.insert(destination.into(), data);
            return Ok(());
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

    fn resolve_path(&self, base: &str, path: &str) -> Result<String, FileError> {
        let virtual_path = !self.mount.is_empty()
            || Self::virtual_directory(base.split('/').next().unwrap_or(""))
            || Self::virtual_directory(path.split('/').next().unwrap_or(""));
        if !virtual_path {
            return self.base.resolve_path(base, path);
        }
        if path.is_empty() {
            return Err(FileError::NotFound);
        }
        if path.starts_with('/') || path.contains(['\\', '\0']) {
            return Err(FileError::NotCapable);
        }
        let result = if base.is_empty() {
            String::from(path)
        } else {
            format!("{base}/{path}")
        };
        if result.split('/').any(|part| part == "..") {
            return Err(FileError::NotCapable);
        }
        Ok(result
            .split('/')
            .filter(|part| !part.is_empty() && *part != ".")
            .collect::<Vec<_>>()
            .join("/"))
    }

    fn directory_view(&self, path: &str) -> Result<Option<Rc<dyn FileSystem>>, FileError> {
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
