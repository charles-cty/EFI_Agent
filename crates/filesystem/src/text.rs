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
    write_mode(filesystem, name, text, false)
}

pub fn append(filesystem: &dyn FileSystem, name: &str, text: &str) -> Result<(), String> {
    write_mode(filesystem, name, text, true)
}

fn write_mode(
    filesystem: &dyn FileSystem,
    name: &str,
    text: &str,
    append: bool,
) -> Result<(), String> {
    let path = path(name)?;
    if text.len() > crate::MAX_RESIZE_BYTES as usize {
        return Err("File exceeds the filesystem file-size limit".into());
    }
    if let Some((parent, _)) = path.rsplit_once('/') {
        ensure_parents(filesystem, parent)?;
    }
    let mut file = filesystem
        .open(
            &path,
            OpenOptions {
                read: false,
                write: true,
                create: true,
                exclusive: false,
                truncate: !append,
                append,
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

/// Stage complete replacement bytes before changing the destination name.
/// Replacing a symlink requires an explicit shell operation to choose its target.
pub fn save(filesystem: &dyn FileSystem, name: &str, text: &str) -> Result<(), String> {
    let destination = path(name)?;
    if text.len() > crate::MAX_RESIZE_BYTES as usize {
        return Err("File exceeds the filesystem file-size limit".into());
    }
    let original = match filesystem.symlink_metadata(&destination) {
        Ok(metadata) if metadata.kind == EntryType::File => Some(metadata),
        Ok(_) => return Err("Atomic save requires a regular file destination".into()),
        Err(FileError::NotFound) => None,
        Err(error) => return Err(format!("Inspect '{name}': {error:?}")),
    };
    // A sibling stays on the destination mount. Exclusive creation prevents
    // an existing user file from being used as staging storage.
    let mut sequence = 0u64;
    let parent = destination.rsplit_once('/').map(|(parent, _)| parent);
    if let Some(parent) = parent {
        ensure_parents(filesystem, parent)?;
    }
    let temporary = loop {
        let candidate = parent.map_or_else(
            || format!(".agent-save-{sequence}"),
            |parent| format!("{parent}/.agent-save-{sequence}"),
        );
        if candidate == destination {
            sequence = sequence.checked_add(1).ok_or("Staging name exhausted")?;
            continue;
        }
        match filesystem.open(
            &candidate,
            crate::OpenOptions {
                read: false,
                write: true,
                create: true,
                exclusive: true,
                truncate: false,
                append: false,
            },
        ) {
            Ok(file) => {
                drop(file);
                break candidate;
            }
            Err(FileError::Exists) => {
                sequence = sequence.checked_add(1).ok_or("Staging name exhausted")?
            }
            Err(error) => return Err(format!("Stage '{name}': {error:?}")),
        }
    };
    let result = (|| {
        write(filesystem, &temporary, text)?;
        if let Some(metadata) = original {
            filesystem
                .set_mode(&temporary, metadata.mode)
                .map_err(|error| format!("Stage mode: {error:?}"))?;
        }
        filesystem
            .rename(&temporary, &destination)
            .map_err(|error| format!("Commit '{name}': {error:?}"))
    })();
    if result.is_err() {
        let _ = filesystem.unlink(&temporary);
    }
    result
}

fn ensure_parents(filesystem: &dyn FileSystem, parent: &str) -> Result<(), String> {
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
    Ok(())
}

/// Byte ranges return the next byte offset. UTF-8 pages end on a character
/// boundary; binary callers can request exact hexadecimal bytes.
pub fn read_range(
    filesystem: &dyn FileSystem,
    name: &str,
    offset: u64,
    limit: usize,
    hex: bool,
) -> Result<String, String> {
    if !(4..=TEXT_LIMIT).contains(&limit) {
        return Err("Invalid read range size".into());
    }
    let name = path(name)?;
    let mut file = filesystem
        .open(
            &name,
            crate::OpenOptions {
                read: true,
                write: false,
                create: false,
                exclusive: false,
                truncate: false,
                append: false,
            },
        )
        .map_err(|error| format!("Open range: {error:?}"))?;
    let total = file
        .metadata()
        .map_err(|error| format!("Range metadata: {error:?}"))?
        .size;
    let offset_i64 = i64::try_from(offset).map_err(|_| "Read offset exceeds i64")?;
    file.seek(offset_i64, 0)
        .map_err(|error| format!("Seek range: {error:?}"))?;
    let mut bytes = vec![0; limit];
    let mut count = 0;
    while count < bytes.len() {
        let read = file
            .read(&mut bytes[count..])
            .map_err(|error| format!("Read range: {error:?}"))?;
        if read == 0 {
            break;
        }
        count += read;
    }
    bytes.truncate(count);
    let content = if hex {
        use core::fmt::Write;
        let mut output = String::with_capacity(bytes.len() * 2);
        for byte in &bytes {
            write!(output, "{byte:02x}").expect("String write");
        }
        output
    } else {
        match core::str::from_utf8(&bytes) {
            Ok(text) => text.into(),
            Err(error) if error.error_len().is_none() && offset + (count as u64) < total => {
                count = error.valid_up_to();
                String::from_utf8(bytes[..count].to_vec()).expect("Validated prefix")
            }
            Err(_) => return Err(
                "Range is not UTF-8; set hex=true for binary bytes or choose a character boundary"
                    .into(),
            ),
        }
    };
    serde_json::to_string(&serde_json::json!({
        "content": content, "encoding": if hex { "hex" } else { "utf8" },
        "offset": offset, "next_offset": offset + count as u64,
        "total_bytes": total, "eof": offset + count as u64 >= total,
    }))
    .map_err(|error| format!("Range result: {error}"))
}
