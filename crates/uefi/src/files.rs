use alloc::{format, string::String};
use uefi::{CString16, boot, fs::FileSystem};

fn volume() -> Result<FileSystem, String> {
    boot::get_image_file_system(boot::image_handle())
        .map(FileSystem::new)
        .map_err(|e| format!("Boot volume: {e}"))
}

pub fn read(path: &str) -> Result<String, String> {
    let path = CString16::try_from(path).map_err(|_| String::from("Invalid UEFI path"))?;
    let mut fs = volume()?;
    let metadata = fs
        .metadata(path.as_ref())
        .map_err(|e| format!("Metadata: {e}"))?;
    if metadata.file_size() > 512 * 1024 {
        return Err(String::from("File exceeds 512 KiB"));
    }
    fs.read_to_string(path.as_ref())
        .map_err(|e| format!("Read: {e}"))
}

pub fn write(path: &str, content: &str) -> Result<(), String> {
    let path = CString16::try_from(path).map_err(|_| String::from("Invalid UEFI path"))?;
    volume()?
        .write(path.as_ref(), content.as_bytes())
        .map_err(|e| format!("Write: {e}"))
}
