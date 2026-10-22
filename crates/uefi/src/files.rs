use alloc::{format, string::String};
use uefi::{CString16, boot, fs::FileSystem};

fn volume() -> Result<FileSystem, String> {
    boot::get_image_file_system(boot::image_handle())
        .map(FileSystem::new)
        .map_err(|e| format!("Boot volume: {e}"))
}

pub fn read_bytes(path: &str) -> Result<alloc::vec::Vec<u8>, String> {
    read_bounded(path, 512 * 1024)
}

fn read_bounded(path: &str, limit: u64) -> Result<alloc::vec::Vec<u8>, String> {
    let path = CString16::try_from(path).map_err(|_| String::from("Invalid UEFI path"))?;
    let mut fs = volume()?;
    let metadata = fs
        .metadata(path.as_ref())
        .map_err(|e| format!("Metadata: {e}"))?;
    if metadata.file_size() > limit {
        return Err(format!("File exceeds {limit} bytes"));
    }
    fs.read(path.as_ref()).map_err(|e| format!("Read: {e}"))
}
pub fn read(path: &str) -> Result<String, String> {
    String::from_utf8(read_bytes(path)?).map_err(|_| "File is not UTF-8".into())
}
