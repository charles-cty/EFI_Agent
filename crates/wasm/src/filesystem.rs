//! Runtime directory descriptors over the common filesystem interfaces.
use alloc::{rc::Rc, string::String, vec::Vec};
use core::cell::RefCell;
pub use efi_agent_filesystem::*;

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
        self.filesystem
            .resolve_path(&self.path, path)
            .map_err(FileError::errno)
    }
}
