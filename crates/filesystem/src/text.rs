//! Bounded model file operations over the same namespace as guest processes.
use crate::{EntryType, FileError, FileSystem, OpenOptions};
use alloc::{format, string::String, vec, vec::Vec};

const TEXT_LIMIT: usize = 512 * 1024;

fn path(path: &str) -> Result<String, String> {
    let normalized = path.replace('\\', "/");
    if normalized.len() > 4096
        || normalized
            .chars()
            .any(|c| c.is_control() || matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        || normalized
            .split('/')
            .any(|part| part == ".." || part.len() > 1024)
    {
        return Err(format!("Invalid workspace path: {path:?}"));
    }
    let relative = normalized.trim_start_matches('/');
    Ok(if relative.is_empty() { "." } else { relative }.into())
}

pub fn read_bytes(
    filesystem: &dyn FileSystem,
    name: &str,
    limit: usize,
) -> Result<Vec<u8>, String> {
    let path = path(name)?;
    let mut file = filesystem
        .open(
            &path,
            OpenOptions {
                read: true,
                write: false,
                create: false,
                exclusive: false,
                truncate: false,
                append: false,
            },
        )
        .map_err(|error| format!("Open '{name}': {error:?}"))?;
    let size = file
        .metadata()
        .map_err(|error| format!("Stat '{name}': {error:?}"))?
        .size;
    if size > limit as u64 {
        return Err(format!("Read '{name}': file exceeds {limit} bytes"));
    }
    let mut bytes = Vec::new();
    let mut buffer = vec![0; 65536];
    loop {
        // Read one byte beyond the limit to detect files that grew after stat.
        let capacity = buffer.len().min(limit - bytes.len() + 1);
        let count = file
            .read(&mut buffer[..capacity])
            .map_err(|error| format!("Read '{name}': {error:?}"))?;
        if count == 0 {
            return Ok(bytes);
        }
        if count > limit - bytes.len() {
            return Err(format!("Read '{name}': file exceeds {limit} bytes"));
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
}

pub fn read(filesystem: &dyn FileSystem, name: &str) -> Result<String, String> {
    String::from_utf8(read_bytes(filesystem, name, TEXT_LIMIT)?)
        .map_err(|_| format!("Read '{name}': file is not UTF-8"))
}

pub fn read_or_list(filesystem: &dyn FileSystem, name: &str) -> Result<String, String> {
    let path = path(name)?;
    let metadata = filesystem
        .metadata(&path)
        .map_err(|error| format!("Stat '{name}': {error:?}"))?;
    if metadata.kind != EntryType::Directory {
        return read(filesystem, name);
    }
    let mut entries = filesystem
        .read_directory(&path)
        .map_err(|error| format!("List '{name}': {error:?}"))?;
    entries.sort();
    let mut output = String::new();
    for (name, _) in entries.iter() {
        if matches!(name, "." | "..") {
            continue;
        }
        if output.len() + name.len() + 1 > TEXT_LIMIT {
            return Err("Directory listing exceeds 512 KiB".into());
        }
        output.push_str(name);
        output.push('\n');
    }
    Ok(output)
}

pub fn write(filesystem: &dyn FileSystem, name: &str, text: &str) -> Result<(), String> {
    let path = path(name)?;
    if text.len() > TEXT_LIMIT {
        return Err("File exceeds 512 KiB".into());
    }
    if let Some((parent, _)) = path.rsplit_once('/') {
        let mut prefix = String::new();
        for part in parent
            .split('/')
            .filter(|part| !part.is_empty() && *part != ".")
        {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            match filesystem.metadata(&prefix) {
                Ok(info) if info.kind == EntryType::Directory => (),
                Ok(_) => return Err(format!("Parent '{prefix}': NotDirectory")),
                Err(FileError::NotFound) => filesystem
                    .create_directory(&prefix)
                    .map_err(|error| format!("Create parent '{prefix}': {error:?}"))?,
                Err(error) => return Err(format!("Inspect parent '{prefix}': {error:?}")),
            }
        }
    }
    let mut file = filesystem
        .open(
            &path,
            OpenOptions {
                read: false,
                write: true,
                create: true,
                exclusive: false,
                truncate: true,
                append: false,
            },
        )
        .map_err(|error| format!("Open '{name}': {error:?}"))?;
    let mut bytes = text.as_bytes();
    while !bytes.is_empty() {
        let count = file
            .write(&bytes[..bytes.len().min(65536)])
            .map_err(|error| format!("Write '{name}': {error:?}"))?;
        if count == 0 {
            return Err(format!("Write '{name}': no progress"));
        }
        bytes = &bytes[count..];
    }
    file.flush()
        .map_err(|error| format!("Flush '{name}': {error:?}"))
}
