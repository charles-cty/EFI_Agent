//! Non-terminal character devices backed by the world's entropy provider.
use super::{EntryType, File, FileError, FileSystemStatistics, Metadata, OpenOptions, TimeUpdate};
use crate::random::{Random, RandomError};
use alloc::{rc::Rc, vec};
use core::cell::RefCell;

pub(super) type Entropy = Rc<RefCell<Option<Rc<dyn Random>>>>;

#[derive(Clone, Copy)]
pub(super) enum Device {
    Null,
    Zero,
    Random,
    Urandom,
}

impl Device {
    pub(super) fn from_path(path: &str) -> Option<Self> {
        match path {
            "dev/null" => Some(Self::Null),
            "dev/zero" => Some(Self::Zero),
            "dev/random" => Some(Self::Random),
            "dev/urandom" => Some(Self::Urandom),
            _ => None,
        }
    }

    pub(super) fn metadata(self) -> Metadata {
        let mut metadata = super::virtual_metadata(EntryType::CharacterDevice, 0);
        metadata.mode = 0o666;
        metadata.device = 2;
        metadata.inode = match self {
            Self::Null => 3,
            Self::Zero => 5,
            Self::Random => 8,
            Self::Urandom => 9,
        };
        metadata
    }
}

pub(super) struct DeviceFile {
    pub kind: Device,
    pub options: OpenOptions,
    pub entropy: Entropy,
}

impl File for DeviceFile {
    fn filesystem_statistics(&self) -> Result<FileSystemStatistics, FileError> {
        Ok(super::special_statistics(2))
    }

    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, FileError> {
        if !self.options.read {
            return Err(FileError::Access);
        }
        if buffer.is_empty() {
            return Ok(0);
        }
        match self.kind {
            Device::Null => Ok(0),
            Device::Zero => {
                buffer.fill(0);
                Ok(buffer.len())
            }
            Device::Random | Device::Urandom => {
                let source = self
                    .entropy
                    .borrow()
                    .clone()
                    .ok_or(FileError::Unsupported)?;
                // Bound generation and publish bytes only after provider success.
                let count = buffer.len().min(crate::io::PIPE_CAPACITY);
                let mut bytes = vec![0; count];
                source.fill(&mut bytes).map_err(|error| match error {
                    RandomError::Unavailable => FileError::Unsupported,
                    RandomError::Io => FileError::Io,
                })?;
                buffer[..count].copy_from_slice(&bytes);
                Ok(count)
            }
        }
    }

    fn write(&mut self, buffer: &[u8]) -> Result<usize, FileError> {
        if !self.options.write {
            return Err(FileError::Access);
        }
        // Writes do not seed the provider or credit entropy.
        Ok(buffer.len())
    }

    fn seek(&mut self, _offset: i64, whence: u32) -> Result<u64, FileError> {
        if whence > 2 {
            return Err(FileError::Invalid);
        }
        Ok(0)
    }

    fn metadata(&mut self) -> Result<Metadata, FileError> {
        Ok(self.kind.metadata())
    }

    fn flush(&mut self) -> Result<(), FileError> {
        Ok(())
    }

    fn sync(&mut self, _data_only: bool) -> Result<(), FileError> {
        Err(FileError::Invalid)
    }

    fn resize(&mut self, _size: u64) -> Result<(), FileError> {
        Err(FileError::Invalid)
    }

    fn set_mode(&mut self, _mode: u16) -> Result<(), FileError> {
        Err(FileError::ReadOnly)
    }

    fn set_times(&mut self, _access: TimeUpdate, _modify: TimeUpdate) -> Result<(), FileError> {
        Err(FileError::ReadOnly)
    }
}
