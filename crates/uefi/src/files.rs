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
    read_bounded(path, 512 * 1024)
}

pub fn read_wasm(path: &str) -> Result<alloc::vec::Vec<u8>, String> {
    read_bounded(path, 48 * 1024 * 1024)
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

pub fn write(path: &str, content: &str) -> Result<(), String> {
    if content.len() > efi_agent_core::agent::MAX_FILE_BYTES {
        return Err(String::from("File exceeds 512 KiB"));
    }
    let target = CString16::try_from(path).map_err(|_| String::from("Invalid UEFI path"))?;
    let mut fs = volume()?;
    if let Some((parent, _)) = path.rsplit_once('\\')
        && !parent.is_empty()
    {
        let mut prefix = String::new();
        for component in parent.split('\\').filter(|part| !part.is_empty()) {
            prefix.push('\\');
            prefix.push_str(component);
            let directory = CString16::try_from(prefix.as_str())
                .map_err(|_| format!("Invalid parent path: {prefix}"))?;
            if fs
                .try_exists(directory.as_ref())
                .map_err(|e| format!("Inspect parent '{prefix}': {e}"))?
            {
                let info = fs
                    .metadata(directory.as_ref())
                    .map_err(|e| format!("Inspect parent '{prefix}': {e}"))?;
                if !info.is_directory() {
                    return Err(format!("Parent '{prefix}' is not a directory"));
                }
            } else {
                fs.create_dir(directory.as_ref())
                    .map_err(|e| format!("Create parent directory '{prefix}': {e}"))?;
            }
        }
    }
    fs.write(target.as_ref(), content.as_bytes())
        .map_err(|e| format!("Write '{path}': {e}"))
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
