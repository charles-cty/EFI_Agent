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

mod device;
pub(crate) mod process;
mod walk;
use device::{Device, DeviceFile, Entropy};
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
        files: if device == 2 { 9 } else { 4 },
        files_free: 0,
        id: device,
        kind: if device == 2 { 0x1373 } else { 0x9fa0 },
        name_max: 1024,
        read_only: true,
    }
}
type Programs = Rc<RefCell<BTreeMap<String, Option<String>>>>;

#[derive(Debug, PartialEq, Eq)]
pub struct MountError {
    pub path: String,
    pub error: FileError,
}

impl core::fmt::Display for MountError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "Mount '{}': {:?}", self.path, self.error)
    }
}

impl core::error::Error for MountError {}

#[derive(Clone, PartialEq, Eq)]
enum Scope {
    Root,
    Workspace,
    Mount(String),
}

/// Files and directories that exist in every guest without being persisted in
/// the workspace. The workspace remains the backing store for ordinary paths.
#[derive(Clone)]
pub struct VirtualFileSystem {
    base: Rc<dyn FileSystem>,
    tmp: Rc<namespace::Namespace>,
    scope: Scope,
    identity: usize,
    programs: Programs,
    entropy: Entropy,
    process: Option<crate::Pid>,
    process_files: Rc<process::Registry>,
}

impl VirtualFileSystem {
    pub(crate) fn executable_path(&self, path: &str) -> Result<(String, Metadata), FileError> {
        let resolved = self.walk(path, Follow::Final, false)?;
        let metadata = resolved.owner(self).metadata_raw(&resolved)?;
        Ok((
            if resolved.filesystem.is_some() {
                path.into()
            } else {
                resolved.path
            },
            metadata,
        ))
    }

    pub fn new(base: Rc<dyn FileSystem>) -> Result<Rc<Self>, MountError> {
        Self::with_programs(
            base,
            Rc::new(RefCell::new(BTreeMap::new())),
            Rc::new(process::Registry::default()),
        )
    }

    pub(crate) fn with_programs(
        base: Rc<dyn FileSystem>,
        programs: Programs,
        process_files: Rc<process::Registry>,
    ) -> Result<Rc<Self>, MountError> {
        for path in ["dev", "tmp", "proc"] {
            let error = match base.symlink_metadata(path) {
                Err(FileError::NotFound) => continue,
                Ok(_) => FileError::Exists,
                Err(error) => error,
            };
            return Err(MountError {
                path: format!("/{path}"),
                error,
            });
        }
        Self::validate_bin(base.as_ref(), &programs.borrow())?;
        let identity = base.identity();
        Ok(Rc::new(Self {
            base,
            tmp: Rc::new(namespace::Namespace::temporary(
                TEMP_CAPACITY,
                MAX_DIRECTORY_ENTRIES + 1,
                3,
                None,
            )),
            scope: Scope::Root,
            identity,
            programs,
            entropy: Rc::new(RefCell::new(None)),
            process: None,
            process_files,
        }))
    }

    pub(crate) fn validate_programs(
        &self,
        programs: &BTreeMap<String, Option<String>>,
    ) -> Result<(), FileError> {
        Self::validate_bin(self.base.as_ref(), programs).map_err(|error| error.error)
    }

    fn validate_bin(
        base: &dyn FileSystem,
        programs: &BTreeMap<String, Option<String>>,
    ) -> Result<(), MountError> {
        let mount_error = |path: &str, error| MountError {
            path: format!("/{path}"),
            error,
        };
        match base.symlink_metadata("bin") {
            Err(FileError::NotFound) => return Ok(()),
            Ok(info) if info.kind == EntryType::Directory => {}
            Ok(_) => return Err(mount_error("bin", FileError::Exists)),
            Err(error) => return Err(mount_error("bin", error)),
        }
        for name in programs.keys() {
            let path = format!("bin/{name}");
            match base.symlink_metadata(&path) {
                Err(FileError::NotFound) => {}
                Ok(_) => return Err(mount_error(&path, FileError::Exists)),
                Err(error) => return Err(mount_error(&path, error)),
            }
        }
        Ok(())
    }

    fn program(&self, path: &str) -> Option<Option<String>> {
        if self.scope == Scope::Workspace {
            return None;
        }
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

    fn virtual_directory(&self, path: &str) -> bool {
        self.scope != Scope::Workspace && matches!(path, "dev" | "tmp" | "proc" | "bin")
    }

    fn virtual_entry(&self, path: &str) -> Option<EntryType> {
        if self.scope == Scope::Workspace {
            return None;
        }
        if Device::from_path(path).is_some() {
            return Some(EntryType::CharacterDevice);
        }
        match path {
            "dev" | "tmp" | "proc" | "bin" => Some(EntryType::Directory),
            _ => None,
        }
    }
}

struct VirtualFile {
    data: Vec<u8>,
    offset: usize,
    read: bool,
    metadata: Metadata,
    process_files: Rc<process::Registry>,
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
        Ok(self.process_files.statistics())
    }
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, FileError> {
        if !self.read {
            return Err(FileError::Access);
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

    fn write(&mut self, _buffer: &[u8]) -> Result<usize, FileError> {
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
        Ok(self.metadata)
    }

    fn flush(&mut self) -> Result<(), FileError> {
        Ok(())
    }

    fn sync(&mut self, _data_only: bool) -> Result<(), FileError> {
        Err(FileError::Invalid)
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
        let path = self.walk(path, follow, options.create)?;
        path.owner(self).open_raw(&path, options)
    }

    fn metadata(&self, path: &str) -> Result<Metadata, FileError> {
        let path = self.walk(path, Follow::Final, false)?;
        path.owner(self).metadata_raw(&path)
    }

    fn symlink_metadata(&self, path: &str) -> Result<Metadata, FileError> {
        let path = self.walk(path, Follow::Inspect, false)?;
        path.owner(self).symlink_metadata_raw(&path)
    }

    fn read_directory(&self, path: &str) -> Result<DirectoryEntries, FileError> {
        let path = self.walk(path, Follow::Final, false)?;
        path.owner(self).read_directory_raw(&path)
    }

    fn create_directory(&self, path: &str) -> Result<(), FileError> {
        let path = self.walk(path, Follow::Parent, true)?;
        path.owner(self).create_directory_raw(&path)
    }

    fn unlink(&self, path: &str) -> Result<(), FileError> {
        let path = self.walk(path, Follow::Parent, false)?;
        path.owner(self).unlink_raw(&path)
    }

    fn remove_directory(&self, path: &str) -> Result<(), FileError> {
        let path = self.walk(path, Follow::Parent, false)?;
        path.owner(self).remove_directory_raw(&path)
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
        let other = destination
            .filesystem
            .as_deref()
            .map_or(other, |owner| owner as &dyn FileSystem);
        source
            .owner(self)
            .rename_to_raw(&source, other, &destination)
    }

    fn symlink(&self, target: &str, path: &str) -> Result<(), FileError> {
        let path = self.walk(path, Follow::Parent, true)?;
        path.owner(self).symlink_raw(target, &path)
    }

    fn read_link(&self, path: &str) -> Result<String, FileError> {
        let path = self.walk(path, Follow::Inspect, false)?;
        path.owner(self).read_link_raw(&path)
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
        let other = destination
            .filesystem
            .as_deref()
            .map_or(other, |owner| owner as &dyn FileSystem);
        source
            .owner(self)
            .hard_link_to_raw(&source, other, &destination, follow)
    }

    fn set_mode(&self, path: &str, mode: u16) -> Result<(), FileError> {
        let path = self.walk(path, Follow::Final, false)?;
        path.owner(self).set_mode_raw(&path, mode)
    }

    fn set_times(
        &self,
        path: &str,
        follow: bool,
        access: TimeUpdate,
        modify: TimeUpdate,
    ) -> Result<(), FileError> {
        let path = self.walk(
            path,
            if follow {
                Follow::Final
            } else {
                Follow::Inspect
            },
            false,
        )?;
        path.owner(self)
            .set_times_raw(&path, follow, access, modify)
    }

    fn directory_view(&self, path: &str) -> Result<Option<Rc<dyn FileSystem>>, FileError> {
        let path = self.walk(path, Follow::Final, false)?;
        path.owner(self).directory_view_raw(&path)
    }

    fn directory_path(&self) -> Result<String, FileError> {
        match &self.scope {
            Scope::Root => Ok(String::new()),
            Scope::Workspace => self.base.directory_path(),
            Scope::Mount(mount) if mount == "tmp" => {
                let path = self.tmp.directory_path()?;
                Ok(if path.is_empty() {
                    mount.clone()
                } else {
                    format!("{mount}/{path}")
                })
            }
            Scope::Mount(mount) => {
                self.metadata_raw("")?;
                Ok(mount.clone())
            }
        }
    }

    fn parent_directory(&self) -> Result<Rc<dyn FileSystem>, FileError> {
        let mut parent = self.clone();
        match &self.scope {
            Scope::Root => (),
            Scope::Workspace => {
                parent.base = self.base.parent_directory()?;
                match parent.base.directory_path() {
                    Ok(path) if path.is_empty() => parent.scope = Scope::Root,
                    Ok(_) | Err(FileError::NotFound) => (),
                    Err(error) => return Err(error),
                }
            }
            Scope::Mount(mount) if mount == "tmp" => match self.tmp.directory_path() {
                Ok(path) if path.is_empty() => parent.scope = Scope::Root,
                Ok(_) | Err(FileError::NotFound) => {
                    let namespace = self.tmp.parent_directory()?;
                    parent.tmp = Rc::new(
                        (namespace.as_ref() as &dyn core::any::Any)
                            .downcast_ref::<namespace::Namespace>()
                            .expect("Namespace returns its own parent views")
                            .clone(),
                    );
                }
                Err(error) => return Err(error),
            },
            Scope::Mount(mount) => {
                self.metadata_raw("")?;
                parent.scope = match mount.rsplit_once('/') {
                    Some((path, _)) => Scope::Mount(path.into()),
                    None => Scope::Root,
                };
            }
        }
        Ok(Rc::new(parent))
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
        let path = self.walk(path, Follow::Final, false)?;
        path.owner(self).filesystem_statistics_raw(&path)
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
        let process_node = match self.process_node(path) {
            Err(FileError::NotFound) if options.create && self.read_only_mount(path) => {
                return Err(FileError::ReadOnly);
            }
            result => result?,
        };
        if let Some(node) = process_node {
            return self.process_open(node, options);
        }
        if let Some(source) = self.program(path) {
            if options.write || options.create || options.truncate || options.append {
                return Err(FileError::ReadOnly);
            }
            return Ok(Box::new(ProgramFile(
                self.base
                    .open(&source.ok_or(FileError::Unsupported)?, options)?,
            )));
        }
        if let Some(path) = self.temporary_path(path) {
            return self.tmp.open(path, options);
        }
        if self.scope != Scope::Workspace
            && let Some(kind) = Device::from_path(path)
        {
            if options.create && options.exclusive {
                return Err(FileError::Exists);
            }
            if options.exclusive && !options.create {
                return Err(FileError::Invalid);
            }
            if (options.truncate || options.append) && !options.write {
                return Err(FileError::Access);
            }
            return Ok(Box::new(DeviceFile {
                kind,
                options,
                entropy: self.entropy.clone(),
            }));
        }
        if let Some(kind) = self.virtual_entry(path) {
            if kind == EntryType::Directory {
                return Err(FileError::IsDirectory);
            }
            if options.write || options.create || options.truncate || options.append {
                return Err(FileError::ReadOnly);
            }
            return Err(FileError::Unsupported);
        }
        if self.read_only_mount(path) {
            return Err(
                if options.write || options.create || options.truncate || options.append {
                    FileError::ReadOnly
                } else {
                    FileError::NotFound
                },
            );
        }
        self.base.open(path, options)
    }

    fn metadata_raw(&self, path: &str) -> Result<Metadata, FileError> {
        let path = self.mounted(path);
        let path = path.as_str();
        if let Some(node) = self.process_node(path)? {
            return self.process_metadata(node, true);
        }
        if let Some(metadata) = self.program_metadata(path) {
            return Ok(metadata);
        }
        if let Some(path) = self.temporary_path(path) {
            return self.tmp.metadata(path);
        }
        if self.scope != Scope::Workspace
            && let Some(kind) = Device::from_path(path)
        {
            return Ok(kind.metadata());
        }
        if let Some(kind) = self.virtual_entry(path) {
            let mut metadata = virtual_metadata(kind, 0);
            metadata.device = match path.split('/').next() {
                Some("dev") => 2,
                Some("tmp") => 3,
                Some("proc") => 4,
                _ => 0,
            };
            return Ok(metadata);
        }
        if self.read_only_mount(path) {
            return Err(FileError::NotFound);
        }
        self.base.metadata(path)
    }

    fn read_directory_raw(&self, path: &str) -> Result<DirectoryEntries, FileError> {
        let path = self.mounted(path);
        let path = path.as_str();
        if let Some(node) = self.process_node(path)? {
            if let Some(crate::io::Descriptor::Directory(directory)) =
                self.process_descriptor(node)?
            {
                return directory.filesystem.read_directory(&directory.path);
            }
            return self.process_directory(node);
        }
        if let Some(path) = self.temporary_path(path) {
            return self.tmp.read_directory(path);
        }
        let mut entries = if path == "bin" && self.scope != Scope::Workspace {
            match self.base.read_directory(path) {
                Ok(entries) => entries,
                Err(FileError::NotFound) => DirectoryEntries::new(),
                Err(error) => return Err(error),
            }
        } else if path.is_empty() || !self.virtual_directory(path) {
            self.base.read_directory(path)?
        } else {
            DirectoryEntries::new()
        };
        if path.is_empty() && self.scope == Scope::Root {
            for name in ["bin", "dev", "proc", "tmp"] {
                if !entries.iter().any(|(entry, _)| entry == name) {
                    entries.push(name.into(), EntryType::Directory)?;
                }
            }
        } else if self.virtual_directory(path) {
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
                entries.push(".".into(), EntryType::Directory)?;
                entries.push("..".into(), EntryType::Directory)?;
                for name in ["null", "zero", "random", "urandom"] {
                    entries.push(name.into(), EntryType::CharacterDevice)?;
                }
                for name in ["fd", "stdin", "stdout", "stderr"] {
                    entries.push(name.into(), EntryType::Symlink)?;
                }
            }
        }
        Ok(entries)
    }

    fn create_directory_raw(&self, path: &str) -> Result<(), FileError> {
        let path = self.mounted(path);
        let path = path.as_str();
        if let Some(path) = self.temporary_path(path) {
            return self.tmp.create_directory(path);
        }
        if self.virtual_directory(path)
            || self.program(path).is_some()
            || self.read_only_mount(path)
        {
            return Err(FileError::ReadOnly);
        }
        self.base.create_directory(path)
    }

    fn unlink_raw(&self, path: &str) -> Result<(), FileError> {
        let path = self.mounted(path);
        let path = path.as_str();
        if let Some(path) = self.temporary_path(path) {
            return self.tmp.unlink(path);
        }
        if self.read_only_mount(path)
            || self.virtual_entry(path).is_some()
            || self.program(path).is_some()
        {
            return Err(FileError::ReadOnly);
        }
        self.base.unlink(path)
    }

    fn remove_directory_raw(&self, path: &str) -> Result<(), FileError> {
        let path = self.mounted(path);
        let path = path.as_str();
        if let Some(path) = self.temporary_path(path) {
            return self.tmp.remove_directory(path);
        }
        if self.read_only_mount(path) || self.virtual_directory(path) {
            return Err(FileError::ReadOnly);
        }
        self.base.remove_directory(path)
    }

    fn symlink_raw(&self, target: &str, path: &str) -> Result<(), FileError> {
        let path = self.mounted(path);
        let path = path.as_str();
        if let Some(path) = self.temporary_path(path) {
            return self.tmp.symlink(target, path);
        }
        if self.read_only_mount(path)
            || self.virtual_entry(path).is_some()
            || self.program(path).is_some()
        {
            return Err(FileError::ReadOnly);
        }
        self.base.symlink(target, path)
    }

    fn read_link_raw(&self, path: &str) -> Result<String, FileError> {
        let path = self.mounted(path);
        if let Some(node) = self.process_node(&path)? {
            return self.process_link(node);
        }
        if let Some(path) = self.temporary_path(&path) {
            return self.tmp.read_link(path);
        }
        if let Some(source) = self.program(&path) {
            return source
                .map(|source| format!("/{source}"))
                .ok_or(FileError::Invalid);
        }
        if self.read_only_mount(&path) {
            return Err(FileError::NotFound);
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
        if let Some(source) = self.temporary_path(&source) {
            return match other_virtual {
                Some(other) => self.tmp.hard_link_to(
                    source,
                    other.tmp.as_ref(),
                    other
                        .temporary_path(&destination)
                        .ok_or(FileError::CrossDevice)?,
                    follow,
                ),
                None => self.tmp.hard_link_to(source, other, &destination, follow),
            };
        }
        if self.special_mount(&source)
            || other_virtual.is_some_and(|other| other.special_mount(&destination))
        {
            return Err(FileError::CrossDevice);
        }
        if self.program(&source).is_some()
            || other_virtual.is_some_and(|other| other.program(&destination).is_some())
            || self.virtual_directory(&source)
            || other_virtual.is_some_and(|other| other.virtual_directory(&destination))
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
        if let Some(node) = self.process_node(path)? {
            return self.process_metadata(node, false);
        }
        if let Some(path) = self.temporary_path(path) {
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
        if self.virtual_entry(path).is_some() {
            let relative = match &self.scope {
                Scope::Root | Scope::Workspace => path,
                Scope::Mount(mount) => path.strip_prefix(mount).unwrap().trim_start_matches('/'),
            };
            return self.metadata_raw(relative);
        }
        if self.read_only_mount(path) {
            return Err(FileError::NotFound);
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
        if let Some(node) = self.process_node(path)? {
            return match self.process_descriptor(node)? {
                Some(crate::io::Descriptor::File { handle, .. }) => {
                    handle.borrow_mut().set_times(access, modify)
                }
                Some(crate::io::Descriptor::Directory(directory)) => directory
                    .filesystem
                    .set_times(&directory.path, follow, access, modify),
                Some(_) => Err(FileError::Unsupported),
                None => Err(FileError::ReadOnly),
            };
        }
        if let Some(path) = self.temporary_path(path) {
            return self.tmp.set_times(path, follow, access, modify);
        }
        if self.virtual_entry(path).is_some() || self.program(path).is_some() {
            return Err(FileError::ReadOnly);
        }
        self.base.set_times(path, follow, access, modify)
    }

    fn set_mode_raw(&self, path: &str, mode: u16) -> Result<(), FileError> {
        let path = self.mounted(path);
        let path = path.as_str();
        if let Some(node) = self.process_node(path)? {
            return match self.process_descriptor(node)? {
                Some(crate::io::Descriptor::File { handle, .. }) => {
                    handle.borrow_mut().set_mode(mode)
                }
                Some(crate::io::Descriptor::Directory(directory)) => {
                    directory.filesystem.set_mode(&directory.path, mode)
                }
                Some(_) => Err(FileError::Unsupported),
                None => Err(FileError::ReadOnly),
            };
        }
        if let Some(path) = self.temporary_path(path) {
            return self.tmp.set_mode(path, mode);
        }
        if self.virtual_entry(path).is_some() || self.program(path).is_some() {
            return Err(FileError::ReadOnly);
        }
        self.base.set_mode(path, mode)
    }

    fn capacity_raw(&self) -> Result<Capacity, FileError> {
        if matches!(&self.scope, Scope::Mount(mount) if mount == "tmp") {
            self.tmp.capacity()
        } else {
            self.base.capacity()
        }
    }

    fn filesystem_statistics_raw(&self, path: &str) -> Result<FileSystemStatistics, FileError> {
        self.metadata_raw(path)?;
        if self.scope == Scope::Workspace {
            return self.base.filesystem_statistics(path);
        }
        let path = self.mounted(path);
        if let Some(node) = self.process_node(&path)?
            && let Some(descriptor) = self.process_descriptor(node)?
        {
            return match descriptor {
                crate::io::Descriptor::File { handle, .. } => {
                    handle.borrow().filesystem_statistics()
                }
                crate::io::Descriptor::Directory(directory) => {
                    directory.filesystem.filesystem_statistics(&directory.path)
                }
                _ => Ok(special_statistics(4)),
            };
        }
        match path.split('/').next() {
            Some("tmp") => self
                .tmp
                .filesystem_statistics(self.temporary_path(&path).unwrap()),
            Some("dev") => Ok(special_statistics(2)),
            Some("proc") => Ok(self.process_files.statistics()),
            _ => self.base.filesystem_statistics(""),
        }
    }

    fn mounts_raw(&self) -> Result<Vec<Mount>, FileError> {
        let mut mounts = self.base.mounts()?;
        if self.scope == Scope::Workspace {
            return Ok(mounts);
        }
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
        if let Some(source) = self.temporary_path(&self.mounted(source)) {
            return match (other as &dyn core::any::Any).downcast_ref::<Self>() {
                Some(other) => self.tmp.rename_to(
                    source,
                    other.tmp.as_ref(),
                    other
                        .temporary_path(&other.mounted(destination))
                        .ok_or(FileError::CrossDevice)?,
                ),
                None => self.tmp.rename_to(source, other, destination),
            };
        }
        let Some(other) = (other as &dyn core::any::Any).downcast_ref::<Self>() else {
            let source = self.mounted(source);
            if self.virtual_directory(source.split('/').next().unwrap_or("")) {
                return Err(FileError::CrossDevice);
            }
            return self.base.rename_to(&source, other, destination);
        };
        if self.identity != other.identity {
            return Err(FileError::CrossDevice);
        }
        let source = self.mounted(source);
        let destination = other.mounted(destination);
        if other.temporary_path(&destination).is_some() {
            return Err(FileError::CrossDevice);
        }
        if self.read_only_mount(&source) || other.read_only_mount(&destination) {
            return Err(FileError::CrossDevice);
        }
        if self.virtual_entry(&source).is_some()
            || other.virtual_entry(&destination).is_some()
            || self.program(&source).is_some()
            || other.program(&destination).is_some()
        {
            return Err(FileError::ReadOnly);
        }
        self.base
            .rename_to(&source, other.base.as_ref(), &destination)
    }

    fn directory_view_raw(&self, path: &str) -> Result<Option<Rc<dyn FileSystem>>, FileError> {
        if path.is_empty() {
            return Ok(Some(Rc::new(self.clone())));
        }
        let path = self.mounted(path);
        let path = path.as_str();
        if let Some(node) = self.process_node(path)?
            && let Some(crate::io::Descriptor::Directory(directory)) =
                self.process_descriptor(node)?
        {
            return directory.filesystem.directory_view(&directory.path);
        }
        if let Some(path) = self.temporary_path(path) {
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
                scope: Scope::Mount("tmp".into()),
                identity: self.identity,
                programs: self.programs.clone(),
                entropy: self.entropy.clone(),
                process: self.process,
                process_files: self.process_files.clone(),
            })));
        }
        if self.virtual_directory(path)
            || self
                .process_node(path)?
                .is_some_and(|node| node.kind() == EntryType::Directory)
        {
            return Ok(Some(Rc::new(VirtualFileSystem {
                base: self.base.clone(),
                tmp: self.tmp.clone(),
                scope: Scope::Mount(path.into()),
                identity: self.identity,
                programs: self.programs.clone(),
                entropy: self.entropy.clone(),
                process: self.process,
                process_files: self.process_files.clone(),
            })));
        }
        Ok(self.base.directory_view(path)?.map(|base| {
            Rc::new(Self {
                base,
                tmp: self.tmp.clone(),
                scope: Scope::Workspace,
                identity: self.identity,
                programs: self.programs.clone(),
                entropy: self.entropy.clone(),
                process: self.process,
                process_files: self.process_files.clone(),
            }) as Rc<dyn FileSystem>
        }))
    }
}

impl VirtualFileSystem {
    pub(crate) fn set_random(&self, random: Rc<dyn crate::random::Random>) {
        *self.entropy.borrow_mut() = Some(random);
    }

    fn temporary_path<'a>(&self, path: &'a str) -> Option<&'a str> {
        if self.scope == Scope::Workspace {
            return None;
        }
        if path == "tmp" {
            Some("")
        } else {
            path.strip_prefix("tmp/")
        }
    }

    pub(crate) fn set_clock(&self, clock: Rc<dyn crate::clock::Clock>) {
        self.process_files.set_clock(clock.clone());
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
        match &self.scope {
            Scope::Root | Scope::Workspace => path.into(),
            Scope::Mount(mount) if path.is_empty() => mount.clone(),
            Scope::Mount(mount) => format!("{mount}/{path}"),
        }
    }

    fn special_mount(&self, path: &str) -> bool {
        self.scope != Scope::Workspace
            && matches!(path.split('/').next(), Some("dev" | "tmp" | "proc"))
    }

    fn read_only_mount(&self, path: &str) -> bool {
        self.scope != Scope::Workspace && matches!(path.split('/').next(), Some("dev" | "proc"))
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
