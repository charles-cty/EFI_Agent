//! Bounded raw objects for the ephemeral inode namespace.
use crate::{
    Capacity, DirectoryEntries, EntryType, File, FileError, FileSystem, Metadata, OpenOptions,
};
use alloc::{boxed::Box, collections::BTreeMap, rc::Rc, string::String, vec::Vec};
use core::cell::{Cell, RefCell};

struct Budget {
    limit: u64,
    used: Cell<u64>,
}

impl Budget {
    fn available(&self) -> u64 {
        self.limit - self.used.get()
    }
}

struct Data {
    bytes: Vec<u8>,
    budget: Rc<Budget>,
}

impl Data {
    fn resize(&mut self, size: usize) -> Result<(), FileError> {
        let previous = self.bytes.len();
        if size > previous {
            let growth = size - previous;
            if growth as u64 > self.budget.available() {
                return Err(FileError::NoSpace);
            }
            self.bytes
                .try_reserve_exact(growth)
                .map_err(|_| FileError::NoSpace)?;
        }
        self.bytes.resize(size, 0);
        self.budget
            .used
            .set(self.budget.used.get() - previous as u64 + size as u64);
        if size < previous {
            self.bytes.shrink_to_fit();
        }
        Ok(())
    }
}

impl Drop for Data {
    fn drop(&mut self) {
        self.budget
            .used
            .set(self.budget.used.get() - self.bytes.len() as u64);
    }
}

pub(super) struct MemoryStore {
    objects: RefCell<BTreeMap<String, Rc<RefCell<Data>>>>,
    budget: Rc<Budget>,
}

impl MemoryStore {
    pub(super) fn new(limit: u64) -> Self {
        Self {
            objects: RefCell::new(BTreeMap::new()),
            budget: Rc::new(Budget {
                limit,
                used: Cell::new(0),
            }),
        }
    }
}

impl FileSystem for MemoryStore {
    fn open(&self, path: &str, options: OpenOptions) -> Result<Box<dyn File>, FileError> {
        let mut objects = self.objects.borrow_mut();
        let data = match objects.get(path) {
            Some(_) if options.exclusive => return Err(FileError::Exists),
            Some(data) => data.clone(),
            None if options.create => {
                let data = Rc::new(RefCell::new(Data {
                    bytes: Vec::new(),
                    budget: self.budget.clone(),
                }));
                objects.insert(path.into(), data.clone());
                data
            }
            None => return Err(FileError::NotFound),
        };
        if options.truncate {
            data.borrow_mut().resize(0)?;
        }
        Ok(Box::new(MemoryFile { data, offset: 0 }))
    }

    fn metadata(&self, path: &str) -> Result<Metadata, FileError> {
        let objects = self.objects.borrow();
        let data = objects.get(path).ok_or(FileError::NotFound)?.borrow();
        Ok(metadata(data.bytes.len()))
    }

    fn read_directory(&self, _path: &str) -> Result<DirectoryEntries, FileError> {
        // Only Namespace operates on these opaque objects. Directory names
        // and metadata are held in its inode graph.
        Err(FileError::Unsupported)
    }

    fn unlink(&self, path: &str) -> Result<(), FileError> {
        self.objects
            .borrow_mut()
            .remove(path)
            .map(|_| ())
            .ok_or(FileError::NotFound)
    }

    fn capacity(&self) -> Result<Capacity, FileError> {
        Ok(Capacity {
            total: self.budget.limit,
            free: self.budget.available(),
            available: self.budget.available(),
            block_size: 1,
        })
    }
}

fn metadata(size: usize) -> Metadata {
    let mut metadata = Metadata::new(EntryType::File, size as u64);
    metadata.allocated = Some(size as u64);
    metadata
}

struct MemoryFile {
    data: Rc<RefCell<Data>>,
    offset: usize,
}

impl File for MemoryFile {
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, FileError> {
        let data = self.data.borrow();
        let count = buffer
            .len()
            .min(data.bytes.len().saturating_sub(self.offset));
        if count != 0 {
            buffer[..count].copy_from_slice(&data.bytes[self.offset..self.offset + count]);
            self.offset += count;
        }
        Ok(count)
    }

    fn write(&mut self, buffer: &[u8]) -> Result<usize, FileError> {
        if buffer.is_empty() {
            return Ok(0);
        }
        let mut data = self.data.borrow_mut();
        let count = buffer.len().min(
            (data.bytes.len() as u64 + data.budget.available())
                .saturating_sub(self.offset as u64)
                .min(usize::MAX as u64) as usize,
        );
        if count == 0 {
            return Err(FileError::NoSpace);
        }
        let end = self.offset.checked_add(count).ok_or(FileError::Overflow)?;
        if end > data.bytes.len() {
            data.resize(end)?;
        }
        data.bytes[self.offset..end].copy_from_slice(&buffer[..count]);
        self.offset = end;
        Ok(count)
    }

    fn seek(&mut self, offset: i64, whence: u32) -> Result<u64, FileError> {
        let base = match whence {
            0 => 0,
            1 => self.offset as i64,
            2 => self.data.borrow().bytes.len() as i64,
            _ => return Err(FileError::Invalid),
        };
        let position = base
            .checked_add(offset)
            .filter(|value| *value >= 0)
            .ok_or(FileError::Invalid)?;
        self.offset = usize::try_from(position).map_err(|_| FileError::Overflow)?;
        Ok(position as u64)
    }

    fn metadata(&mut self) -> Result<Metadata, FileError> {
        Ok(metadata(self.data.borrow().bytes.len()))
    }

    fn resize(&mut self, size: u64) -> Result<(), FileError> {
        self.data
            .borrow_mut()
            .resize(usize::try_from(size).map_err(|_| FileError::Overflow)?)
    }

    fn flush(&mut self) -> Result<(), FileError> {
        Ok(())
    }
}
