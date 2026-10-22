use super::state::{PRIVATE, State};
use crate::{EntryType, FileError, FileSystem, OpenOptions};
use alloc::{string::String, vec, vec::Vec};

const MAGIC: &[u8; 8] = b"EFIFS001";
const FORMAT: &[u8] = b"EFI Agent filesystem 1\n";
pub const MAX_METADATA_BYTES: usize = 4 * 1024 * 1024;

fn options(create: bool) -> OpenOptions {
    OpenOptions {
        read: !create,
        write: create,
        create,
        exclusive: false,
        truncate: create,
        append: false,
    }
}

fn read(fs: &dyn FileSystem, path: &str, limit: usize) -> Result<Vec<u8>, FileError> {
    let mut file = fs.open(path, options(false))?;
    let size = file.metadata()?.size;
    if size > limit as u64 {
        return Err(FileError::Corrupt);
    }
    let mut bytes = vec![0; size as usize];
    let mut used = 0;
    while used < bytes.len() {
        let end = (used + 65536).min(bytes.len());
        let count = file.read(&mut bytes[used..end])?;
        if count == 0 {
            return Err(FileError::Corrupt);
        }
        assert!(count <= end - used, "Provider read exceeds buffer");
        used += count;
    }
    Ok(bytes)
}

fn write(fs: &dyn FileSystem, path: &str, bytes: &[u8]) -> Result<(), FileError> {
    let mut file = fs.open(path, options(true))?;
    let mut used = 0;
    while used < bytes.len() {
        let end = (used + 65536).min(bytes.len());
        let count = file.write(&bytes[used..end])?;
        if count == 0 {
            return Err(FileError::Io);
        }
        assert!(count <= end - used, "Provider write exceeds buffer");
        used += count;
    }
    file.flush()
}

pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

pub fn initialize(fs: &dyn FileSystem, resume: bool) -> Result<(), FileError> {
    if !resume {
        fs.create_directory(PRIVATE)?;
        write(fs, ".efi-agent-fs/format", FORMAT)?;
    }
    match fs.metadata(".efi-agent-fs/objects") {
        Err(FileError::NotFound) => fs.create_directory(".efi-agent-fs/objects"),
        Ok(info) if resume && info.kind == EntryType::Directory => Ok(()),
        Ok(_) => Err(FileError::Corrupt),
        Err(error) => Err(error),
    }
}

pub fn load(fs: &dyn FileSystem) -> Result<Option<(u64, State)>, FileError> {
    match fs.metadata(PRIVATE) {
        Err(FileError::NotFound) => return Ok(None),
        Ok(info) if info.kind == EntryType::Directory => (),
        Ok(_) => return Err(FileError::Exists),
        Err(error) => return Err(error),
    }
    match read(fs, ".efi-agent-fs/format", FORMAT.len()) {
        Ok(bytes) if bytes == FORMAT => (),
        Ok(_) | Err(FileError::NotFound | FileError::Corrupt) => return Err(FileError::Exists),
        Err(error) => return Err(error),
    }
    let mut latest: Option<(u64, State)> = None;
    let mut found = false;
    for slot in 0..2 {
        let path = alloc::format!(".efi-agent-fs/state{slot}");
        let bytes = match read(fs, &path, MAX_METADATA_BYTES + 24) {
            Ok(bytes) => {
                found = true;
                bytes
            }
            Err(FileError::NotFound) => continue,
            Err(FileError::Corrupt) => {
                found = true;
                continue;
            }
            Err(error) => return Err(error),
        };
        if let Ok((generation, state)) = decode(&bytes) {
            if latest.as_ref().is_some_and(|(old, _)| *old == generation) {
                return Err(FileError::Corrupt);
            }
            if latest.as_ref().is_none_or(|(old, _)| *old < generation) {
                latest = Some((generation, state));
            }
        }
    }
    if found && latest.is_none() {
        return Err(FileError::Corrupt);
    }
    Ok(latest)
}

pub fn encode(generation: u64, state: &State) -> Result<Vec<u8>, FileError> {
    state.validate()?;
    let payload = serde_json::to_vec(state).map_err(|_| FileError::Corrupt)?;
    if payload.len() > MAX_METADATA_BYTES {
        return Err(FileError::ResourceLimit);
    }
    let mut bytes = Vec::with_capacity(payload.len() + 24);
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&generation.to_le_bytes());
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&crc32(&payload).to_le_bytes());
    bytes.extend_from_slice(&payload);
    Ok(bytes)
}

pub fn decode(bytes: &[u8]) -> Result<(u64, State), FileError> {
    if bytes.len() < 24 || &bytes[..8] != MAGIC {
        return Err(FileError::Corrupt);
    }
    let generation = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
    let length = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;
    let crc = u32::from_le_bytes(bytes[20..24].try_into().unwrap());
    if generation == 0
        || length > MAX_METADATA_BYTES
        || length != bytes.len() - 24
        || crc != crc32(&bytes[24..])
    {
        return Err(FileError::Corrupt);
    }
    let state: State = serde_json::from_slice(&bytes[24..]).map_err(|_| FileError::Corrupt)?;
    state.validate()?;
    Ok((generation, state))
}

pub fn commit(fs: &dyn FileSystem, generation: u64, bytes: &[u8]) -> Result<(), FileError> {
    let path: String = alloc::format!(".efi-agent-fs/state{}", generation % 2);
    write(fs, &path, bytes)
}
