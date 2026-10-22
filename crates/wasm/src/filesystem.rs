//! Runtime directory descriptors over the common filesystem interfaces.
use alloc::{boxed::Box, collections::BTreeMap, format, rc::Rc, string::String, vec::Vec};
use core::cell::RefCell;
pub use efi_agent_filesystem::*;

type TempFiles = BTreeMap<String, Rc<RefCell<Vec<u8>>>>;

/// Files and directories that exist in every guest without being persisted in
/// the workspace. The workspace remains the backing store for ordinary paths.
pub struct VirtualFileSystem {
    base: Rc<dyn FileSystem>,
    tmp: Rc<RefCell<TempFiles>>,
    mount: String,
    identity: usize,
}

impl VirtualFileSystem {
    pub fn new(base: Rc<dyn FileSystem>) -> Rc<Self> {
        let identity = base.identity();
        Rc::new(Self {
            base,
            tmp: Rc::new(RefCell::new(BTreeMap::new())),
            mount: String::new(),
            identity,
        })
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
}

impl File for VirtualFile {
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
        Ok(Metadata::new(EntryType::File, self.data.len() as u64))
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
    offset: usize,
    write: bool,
    append: bool,
}

impl File for TmpFile {
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
        let mut data = self.data.borrow_mut();
        if self.append {
            self.offset = data.len();
        }
        let end = self
            .offset
            .checked_add(buffer.len())
            .ok_or(FileError::FileTooLarge)?;
        if end as u64 > MAX_RESIZE_BYTES {
            return Err(FileError::FileTooLarge);
        }
        if self.offset > data.len() {
            data.resize(self.offset, 0);
        }
        if self.offset + buffer.len() > data.len() {
            data.resize(self.offset + buffer.len(), 0);
        }
        data[self.offset..self.offset + buffer.len()].copy_from_slice(buffer);
        self.offset += buffer.len();
        Ok(buffer.len())
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
        Ok(Metadata::new(
            EntryType::File,
            self.data.borrow().len() as u64,
        ))
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
        let size = usize::try_from(size).map_err(|_| FileError::Overflow)?;
        self.data.borrow_mut().resize(size, 0);
        Ok(())
    }
}

impl FileSystem for VirtualFileSystem {
    fn open(&self, path: &str, options: OpenOptions) -> Result<Box<dyn File>, FileError> {
        let path = self.mounted(path);
        let path = path.as_str();
        if let Some(name) = path.strip_prefix("tmp/") {
            if name.is_empty() || name.contains('/') {
                return Err(FileError::NotFound);
            }
            let mut files = self.tmp.borrow_mut();
            let data = match files.get(name) {
                Some(_data) if options.exclusive => return Err(FileError::Exists),
                Some(data) => data.clone(),
                None if options.create => {
                    let data = Rc::new(RefCell::new(Vec::new()));
                    files.insert(name.into(), data.clone());
                    data
                }
                None => return Err(FileError::NotFound),
            };
            if options.truncate {
                data.borrow_mut().clear();
            }
            return Ok(Box::new(TmpFile {
                data,
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
            }));
        }
        self.base.open(path, options)
    }

    fn metadata(&self, path: &str) -> Result<Metadata, FileError> {
        let path = self.mounted(path);
        let path = path.as_str();
        if let Some(name) = path.strip_prefix("tmp/") {
            let data = self
                .tmp
                .borrow()
                .get(name)
                .cloned()
                .ok_or(FileError::NotFound)?;
            return Ok(Metadata::new(EntryType::File, data.borrow().len() as u64));
        }
        if let Some(kind) = Self::virtual_entry(path) {
            return Ok(Metadata::new(kind, 0));
        }
        self.base.metadata(path)
    }

    fn read_directory(&self, path: &str) -> Result<DirectoryEntries, FileError> {
        let path = self.mounted(path);
        let path = path.as_str();
        let mut entries = if path.is_empty() || !Self::virtual_directory(path) {
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
            if path == "tmp" {
                for name in self.tmp.borrow().keys() {
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
        if Self::virtual_directory(path) || path.starts_with("dev/") || path.starts_with("proc/") {
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
                    .contains_key(source)
                    .then_some(())
                    .ok_or(FileError::NotFound);
            }
            let data = files.remove(source).ok_or(FileError::NotFound)?;
            files.insert(destination.into(), data);
            return Ok(());
        }
        if Self::virtual_entry(source).is_some() || Self::virtual_entry(destination).is_some() {
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
                .remove(name)
                .map(|_| ())
                .ok_or(FileError::NotFound);
        }
        if Self::virtual_entry(path).is_some() {
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
        if Self::virtual_entry(path).is_some() {
            return Err(FileError::ReadOnly);
        }
        self.base.symlink(target, path)
    }

    fn read_link(&self, path: &str) -> Result<String, FileError> {
        let path = self.mounted(path);
        self.base.read_link(&path)
    }

    fn symlink_metadata(&self, path: &str) -> Result<Metadata, FileError> {
        let path = self.mounted(path);
        let path = path.as_str();
        if let Some(kind) = Self::virtual_entry(path) {
            return Ok(Metadata::new(kind, 0));
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
        if Self::virtual_entry(path).is_some() {
            return Err(FileError::ReadOnly);
        }
        self.base.set_times(path, follow, access, modify)
    }

    fn set_mode(&self, path: &str, mode: u16) -> Result<(), FileError> {
        let path = self.mounted(path);
        let path = path.as_str();
        if Self::virtual_entry(path).is_some() {
            return Err(FileError::ReadOnly);
        }
        self.base.set_mode(path, mode)
    }

    fn capacity(&self) -> Result<Capacity, FileError> {
        self.base.capacity()
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
                    .contains_key(source)
                    .then_some(())
                    .ok_or(FileError::NotFound);
            }
            let data = files.remove(source).ok_or(FileError::NotFound)?;
            files.insert(destination.into(), data);
            return Ok(());
        }
        if Self::virtual_entry(&source).is_some() || Self::virtual_entry(&destination).is_some() {
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
            path.strip_prefix('/').unwrap_or(path)
        } else {
            path
        };
        self.filesystem
            .resolve_path(&self.path, path)
            .map_err(FileError::errno)
    }
}
