//! Guest file handles on the boot volume's workspace.
use alloc::{
    boxed::Box,
    string::{String, ToString},
};
use efi_agent_wasm::filesystem::{
    DirectoryEntries, EntryType, File, FileError, FileSystem, Metadata, OpenOptions,
};
use uefi::{
    CString16, Status, boot,
    proto::media::file::{File as EfiFile, FileAttribute, FileInfo, FileMode, RegularFile},
};

pub struct FirmwareFileSystem(pub String);

fn error(status: Status) -> FileError {
    match status {
        Status::NOT_FOUND => FileError::NotFound,
        Status::ACCESS_DENIED | Status::WRITE_PROTECTED => FileError::Access,
        Status::INVALID_PARAMETER => FileError::Invalid,
        Status::UNSUPPORTED => FileError::Unsupported,
        _ => FileError::Io,
    }
}

impl FirmwareFileSystem {
    fn open_handle(
        &self,
        path: &str,
        mode: FileMode,
    ) -> Result<uefi::proto::media::file::FileHandle, FileError> {
        let mut name = self.0.clone();
        if !path.is_empty() {
            name.push('\\');
            name.push_str(&path.replace('/', "\\"));
        }
        let name = CString16::try_from(name.as_str()).map_err(|_| FileError::Invalid)?;
        let mut volume = boot::get_image_file_system(boot::image_handle())
            .map_err(|failure| error(failure.status()))?;
        let mut root = volume
            .open_volume()
            .map_err(|failure| error(failure.status()))?;
        root.open(&name, mode, FileAttribute::empty())
            .map_err(|failure| error(failure.status()))
    }
}

impl FileSystem for FirmwareFileSystem {
    fn create_directory(&self, path: &str) -> Result<(), FileError> {
        // EFI create opens an existing entry. Check first to implement mkdir.
        match self.metadata(path) {
            Ok(_) => return Err(FileError::Exists),
            Err(FileError::NotFound) => {}
            Err(failure) => return Err(failure),
        }
        let (parent, name) = path.rsplit_once('/').unwrap_or(("", path));
        if name.is_empty() {
            return Err(FileError::Exists);
        }
        let mut parent = self.open_handle(parent, FileMode::ReadWrite)?;
        if !parent
            .is_directory()
            .map_err(|failure| error(failure.status()))?
        {
            return Err(FileError::NotDirectory);
        }
        let name = CString16::try_from(name).map_err(|_| FileError::Invalid)?;
        parent
            .open(&name, FileMode::CreateReadWrite, FileAttribute::DIRECTORY)
            .map_err(|failure| error(failure.status()))?;
        Ok(())
    }

    fn open(&self, path: &str, options: OpenOptions) -> Result<Box<dyn File>, FileError> {
        // The dispatcher is single-threaded. No other guest runs between this
        // existence check and EFI Open, which has no exclusive-create flag.
        if options.create && options.exclusive {
            match self.metadata(path) {
                Ok(_) => return Err(FileError::Exists),
                Err(FileError::NotFound) => {}
                Err(failure) => return Err(failure),
            }
        }
        let mode = if options.create {
            FileMode::CreateReadWrite
        } else if options.write {
            FileMode::ReadWrite
        } else {
            FileMode::Read
        };
        let handle = self.open_handle(path, mode)?;
        let mut handle = handle.into_regular_file().ok_or(FileError::IsDirectory)?;
        if options.truncate {
            set_size(&mut handle, 0)?;
        }
        Ok(Box::new(FirmwareFile {
            handle,
            append: options.append,
        }))
    }
    fn metadata(&self, path: &str) -> Result<Metadata, FileError> {
        let mut handle = self.open_handle(path, FileMode::Read)?;
        let info = handle
            .get_boxed_info::<FileInfo>()
            .map_err(|failure| error(failure.status()))?;
        Ok(Metadata {
            directory: info.is_directory(),
            size: info.file_size(),
        })
    }
    fn read_directory(&self, path: &str) -> Result<DirectoryEntries, FileError> {
        let mut directory = self
            .open_handle(path, FileMode::Read)?
            .into_directory()
            .ok_or(FileError::NotDirectory)?;
        let mut entries = DirectoryEntries::new();
        entries.push(".".into(), EntryType::Directory)?;
        entries.push("..".into(), EntryType::Directory)?;
        while let Some(info) = directory
            .read_entry_boxed()
            .map_err(|failure| error(failure.status()))?
        {
            let name = info.file_name().to_string();
            if name == "." || name == ".." {
                continue;
            }
            let kind = if info.is_directory() {
                EntryType::Directory
            } else {
                EntryType::File
            };
            entries.push(name, kind)?;
        }
        Ok(entries)
    }
}

struct FirmwareFile {
    handle: RegularFile,
    append: bool,
}

fn set_size(handle: &mut RegularFile, size: u64) -> Result<(), FileError> {
    let info = handle
        .get_boxed_info::<FileInfo>()
        .map_err(|failure| error(failure.status()))?;
    #[repr(align(8))]
    struct InfoBuffer([u8; 1024]);
    let mut storage = InfoBuffer([0; 1024]);
    let updated = FileInfo::new(
        &mut storage.0,
        size,
        info.physical_size(),
        *info.create_time(),
        *info.last_access_time(),
        *info.modification_time(),
        info.attribute(),
        info.file_name(),
    )
    .map_err(|_| FileError::Invalid)?;
    handle
        .set_info(updated)
        .map_err(|failure| error(failure.status()))
}

impl File for FirmwareFile {
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, FileError> {
        // EFI FAT can reject a read beyond EOF after truncation. Unix keeps
        // the cursor and returns an empty read in that position.
        let position = self
            .handle
            .get_position()
            .map_err(|failure| error(failure.status()))?;
        if buffer.is_empty() || position >= self.metadata()?.size {
            return Ok(0);
        }
        self.handle
            .read(buffer)
            .map_err(|failure| error(failure.status()))
    }
    fn write(&mut self, buffer: &[u8]) -> Result<usize, FileError> {
        if self.append {
            self.handle
                .set_position(RegularFile::END_OF_FILE)
                .map_err(|failure| error(failure.status()))?;
        }
        match self.handle.write(buffer) {
            Ok(()) => Ok(buffer.len()),
            Err(failure) if *failure.data() != 0 => Ok(*failure.data()),
            Err(failure) => Err(error(failure.status())),
        }
    }
    fn seek(&mut self, offset: i64, whence: u32) -> Result<u64, FileError> {
        let base = match whence {
            0 => 0,
            1 => self
                .handle
                .get_position()
                .map_err(|failure| error(failure.status()))?,
            2 => self.metadata()?.size,
            _ => return Err(FileError::Invalid),
        };
        let position = base.checked_add_signed(offset).ok_or(FileError::Invalid)?;
        if position == RegularFile::END_OF_FILE {
            return Err(FileError::Overflow);
        }
        self.handle
            .set_position(position)
            .map_err(|failure| error(failure.status()))?;
        Ok(position)
    }
    fn metadata(&mut self) -> Result<Metadata, FileError> {
        let info = self
            .handle
            .get_boxed_info::<FileInfo>()
            .map_err(|failure| error(failure.status()))?;
        Ok(Metadata {
            directory: false,
            size: info.file_size(),
        })
    }
    fn flush(&mut self) -> Result<(), FileError> {
        self.handle
            .flush()
            .map_err(|failure| error(failure.status()))
    }
    fn resize(&mut self, size: u64) -> Result<(), FileError> {
        // Package/workspace volumes are bounded to 48 MiB. Reject larger
        // resize requests before modifying the file or entering a zero-fill loop.
        if size > efi_agent_wasm::filesystem::MAX_RESIZE_BYTES {
            return Err(FileError::FileTooLarge);
        }
        let old_size = self.metadata()?.size;
        let position = self
            .handle
            .get_position()
            .map_err(|failure| error(failure.status()))?;
        let result = if size <= old_size {
            set_size(&mut self.handle, size)
        } else {
            // EFI SetInfo does not promise zero contents for newly allocated
            // bytes. Write each extended byte explicitly, bypassing append.
            (|| {
                self.handle
                    .set_position(old_size)
                    .map_err(|failure| error(failure.status()))?;
                let zeros = [0u8; 4096];
                let mut remaining = size - old_size;
                while remaining != 0 {
                    let count = remaining.min(zeros.len() as u64) as usize;
                    self.handle
                        .write(&zeros[..count])
                        .map_err(|failure| error(failure.status()))?;
                    remaining -= count as u64;
                }
                Ok(())
            })()
        };
        let restore = self
            .handle
            .set_position(position)
            .map_err(|failure| error(failure.status()));
        result.and(restore)
    }
}
