use crate::{DirectoryEntries, EntryType, File, FileError, FileSystem, Metadata, OpenOptions};
use alloc::boxed::Box;
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

pub struct NativeFileSystem(pub PathBuf);

fn error(error: std::io::Error) -> FileError {
    match error.kind() {
        std::io::ErrorKind::NotFound => FileError::NotFound,
        std::io::ErrorKind::PermissionDenied => FileError::Access,
        std::io::ErrorKind::AlreadyExists => FileError::Exists,
        std::io::ErrorKind::InvalidInput => FileError::Invalid,
        std::io::ErrorKind::IsADirectory => FileError::IsDirectory,
        std::io::ErrorKind::NotADirectory => FileError::NotDirectory,
        std::io::ErrorKind::DirectoryNotEmpty => FileError::NotEmpty,
        std::io::ErrorKind::StorageFull => FileError::NoSpace,
        std::io::ErrorKind::ReadOnlyFilesystem => FileError::ReadOnly,
        _ => FileError::Io,
    }
}

fn metadata(metadata: fs::Metadata) -> Metadata {
    let mut result = Metadata::new(
        if metadata.is_dir() {
            EntryType::Directory
        } else {
            EntryType::File
        },
        metadata.len(),
    );
    let nanos = |time: Result<std::time::SystemTime, std::io::Error>| {
        time.ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .and_then(|duration| u64::try_from(duration.as_nanos()).ok())
            .unwrap_or(0)
    };
    result.accessed = nanos(metadata.accessed());
    result.modified = nanos(metadata.modified());
    result.created = nanos(metadata.created());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        result.mode = (metadata.permissions().mode() & 0o7777) as u16;
    }
    #[cfg(windows)]
    if metadata.permissions().readonly() {
        result.mode &= !0o222;
    }
    result
}

impl NativeFileSystem {
    fn path(&self, path: &str) -> Result<PathBuf, FileError> {
        let mut result = self.0.clone();
        for component in Path::new(path).components() {
            let std::path::Component::Normal(name) = component else {
                return Err(FileError::Invalid);
            };
            result.push(name);
            match fs::symlink_metadata(&result) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err(FileError::Unsupported);
                }
                Ok(_) => {}
                Err(failure) if failure.kind() == std::io::ErrorKind::NotFound => {}
                Err(failure) => return Err(error(failure)),
            }
        }
        Ok(result)
    }
}

impl FileSystem for NativeFileSystem {
    fn unlink(&self, path: &str) -> Result<(), FileError> {
        if path.is_empty() {
            return Err(FileError::Access);
        }
        fs::remove_file(self.path(path)?).map_err(error)
    }
    fn remove_directory(&self, path: &str) -> Result<(), FileError> {
        if path.is_empty() {
            return Err(FileError::Access);
        }
        fs::remove_dir(self.path(path)?).map_err(error)
    }
    fn capacity(&self) -> Result<crate::Capacity, FileError> {
        capacity(&self.0)
    }
    fn rename(&self, source: &str, destination: &str) -> Result<(), FileError> {
        if source.is_empty() || destination.is_empty() {
            return Err(FileError::Access);
        }
        fs::rename(self.path(source)?, self.path(destination)?).map_err(error)
    }

    fn create_directory(&self, path: &str) -> Result<(), FileError> {
        fs::create_dir(self.path(path)?).map_err(error)
    }

    fn open(&self, path: &str, options: OpenOptions) -> Result<Box<dyn File>, FileError> {
        let path = self.path(path)?;
        let exists = match fs::metadata(&path) {
            Ok(metadata) if metadata.is_dir() => return Err(FileError::IsDirectory),
            Ok(_) => true,
            Err(failure) if failure.kind() == std::io::ErrorKind::NotFound => false,
            Err(failure) => return Err(error(failure)),
        };
        // The new host handle can write during creation. The guest descriptor
        // still grants only its requested rights.
        let create = options.create && (!exists || options.write || options.exclusive);
        let handle = fs::OpenOptions::new()
            .read(options.read || !options.write)
            .write(options.write || create)
            .create(create)
            .create_new(options.create && options.exclusive)
            .truncate(options.truncate)
            .open(path)
            .map_err(error)?;
        Ok(Box::new(NativeFile {
            handle,
            append: options.append,
            writable: options.write || create,
        }))
    }
    fn metadata(&self, path: &str) -> Result<Metadata, FileError> {
        fs::metadata(self.path(path)?).map(metadata).map_err(error)
    }
    fn read_directory(&self, path: &str) -> Result<DirectoryEntries, FileError> {
        let mut entries = DirectoryEntries::new();
        entries.push(".".into(), EntryType::Directory)?;
        entries.push("..".into(), EntryType::Directory)?;
        for entry in fs::read_dir(self.path(path)?).map_err(error)? {
            let entry = entry.map_err(error)?;
            let kind = entry.file_type().map_err(error)?;
            let kind = if kind.is_dir() {
                EntryType::Directory
            } else if kind.is_file() {
                EntryType::File
            } else if kind.is_symlink() {
                EntryType::Symlink
            } else {
                EntryType::Unknown
            };
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| FileError::Invalid)?;
            entries.push(name, kind)?;
        }
        Ok(entries)
    }
}

#[cfg(windows)]
fn capacity(path: &Path) -> Result<crate::Capacity, FileError> {
    use std::os::windows::ffi::OsStrExt;
    let path: std::vec::Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let (mut available, mut total, mut free) = (0, 0, 0);
    // The path and all three output values remain valid for the synchronous call.
    if unsafe {
        windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
            path.as_ptr(),
            &mut available,
            &mut total,
            &mut free,
        )
    } == 0
    {
        return Err(error(std::io::Error::last_os_error()));
    }
    Ok(crate::Capacity {
        total,
        available,
        block_size: 4096,
    })
}

#[cfg(unix)]
fn capacity(path: &Path) -> Result<crate::Capacity, FileError> {
    use std::os::unix::ffi::OsStrExt;
    let path =
        std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(|_| FileError::Invalid)?;
    let mut info = core::mem::MaybeUninit::<libc::statvfs>::uninit();
    // statvfs initializes the output on success and does not retain its pointers.
    if unsafe { libc::statvfs(path.as_ptr(), info.as_mut_ptr()) } != 0 {
        return Err(error(std::io::Error::last_os_error()));
    }
    let info = unsafe { info.assume_init() };
    let block = if info.f_frsize != 0 {
        info.f_frsize
    } else {
        info.f_bsize
    };
    Ok(crate::Capacity {
        total: info.f_blocks.saturating_mul(block),
        available: info.f_bavail.saturating_mul(block),
        block_size: block.try_into().map_err(|_| FileError::Overflow)?,
    })
}

struct NativeFile {
    handle: fs::File,
    append: bool,
    writable: bool,
}
impl File for NativeFile {
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, FileError> {
        self.handle.read(buffer).map_err(error)
    }
    fn write(&mut self, buffer: &[u8]) -> Result<usize, FileError> {
        if self.append {
            self.handle.seek(SeekFrom::End(0)).map_err(error)?;
        }
        self.handle.write(buffer).map_err(error)
    }
    fn seek(&mut self, offset: i64, whence: u32) -> Result<u64, FileError> {
        let position = match whence {
            0 => SeekFrom::Start(offset.try_into().map_err(|_| FileError::Invalid)?),
            1 => SeekFrom::Current(offset),
            2 => SeekFrom::End(offset),
            _ => return Err(FileError::Invalid),
        };
        self.handle.seek(position).map_err(error)
    }
    fn metadata(&mut self) -> Result<Metadata, FileError> {
        self.handle.metadata().map(metadata).map_err(error)
    }
    fn flush(&mut self) -> Result<(), FileError> {
        if self.writable {
            self.handle.sync_all().map_err(error)
        } else {
            Ok(())
        }
    }
    fn resize(&mut self, size: u64) -> Result<(), FileError> {
        if size > crate::MAX_RESIZE_BYTES {
            return Err(FileError::FileTooLarge);
        }
        self.handle.set_len(size).map_err(error)
    }
}
