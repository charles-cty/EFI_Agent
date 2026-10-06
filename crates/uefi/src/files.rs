use alloc::{
    format,
    string::{String, ToString},
};
use uefi::{CString16, boot, fs::FileSystem};

fn volume() -> Result<FileSystem, String> {
    boot::get_image_file_system(boot::image_handle())
        .map(FileSystem::new)
        .map_err(|e| format!("Boot volume: {e}"))
}

pub fn read_bytes(path: &str) -> Result<alloc::vec::Vec<u8>, String> {
    let path = CString16::try_from(path).map_err(|_| String::from("Invalid UEFI path"))?;
    let mut fs = volume()?;
    let metadata = fs
        .metadata(path.as_ref())
        .map_err(|e| format!("Metadata: {e}"))?;
    if metadata.file_size() > 512 * 1024 {
        return Err("File exceeds 512 KiB".into());
    }
    fs.read(path.as_ref()).map_err(|e| format!("Read: {e}"))
}
pub fn read(path: &str) -> Result<String, String> {
    String::from_utf8(read_bytes(path)?).map_err(|_| "File is not UTF-8".into())
}

pub fn write(path: &str, content: &str) -> Result<(), String> {
    if content.len() > efi_agent_core::agent::MAX_FILE_BYTES {
        return Err(String::from("File exceeds 512 KiB"));
    }
    let path = CString16::try_from(path).map_err(|_| String::from("Invalid UEFI path"))?;
    volume()?
        .write(path.as_ref(), content.as_bytes())
        .map_err(|e| format!("Write: {e}"))
}

pub fn read_or_list(path: &str) -> Result<String, String> {
    let path = CString16::try_from(path).map_err(|_| String::from("Invalid UEFI path"))?;
    let mut fs = volume()?;
    let metadata = fs
        .metadata(path.as_ref())
        .map_err(|e| format!("Metadata: {e}"))?;
    if !metadata.is_directory() {
        if metadata.file_size() > efi_agent_core::agent::MAX_FILE_BYTES as u64 {
            return Err(String::from("File exceeds 512 KiB"));
        }
        return fs
            .read_to_string(path.as_ref())
            .map_err(|e| format!("Read: {e}"));
    }
    let mut output = String::new();
    let entries = fs
        .read_dir(path.as_ref())
        .map_err(|e| format!("List: {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("Directory entry: {e}"))?;
        output.push_str(&entry.file_name().to_string());
        output.push('\n');
        if output.len() > efi_agent_core::agent::MAX_FILE_BYTES {
            return Err(String::from("Directory listing exceeds 512 KiB"));
        }
    }
    Ok(output)
}
