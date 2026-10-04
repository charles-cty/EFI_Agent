//! Create a GPT disk with a FAT32 EFI System Partition from a boot tree.
use fatfs::Write;
use std::{
    collections::HashSet,
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
};

const MIB: u64 = 1024 * 1024;
const ESP_BYTES: u64 = 64 * MIB;
const DISK_BYTES: u64 = ESP_BYTES + 2 * MIB;
const MAX_SOURCE_BYTES: u64 = 48 * MIB;
const MAX_ENTRIES: usize = 4096;

struct Entry {
    source: PathBuf,
    destination: String,
    directory: bool,
    length: u64,
}

fn collect(root: &Path, prefix: &str, depth: usize, entries: &mut Vec<Entry>) -> io::Result<()> {
    if depth > 32 {
        return Err(io::Error::other("Boot tree exceeds 32 directory levels"));
    }
    let mut children = fs::read_dir(root)?.collect::<io::Result<Vec<_>>>()?;
    children.sort_by_key(|entry| entry.file_name());
    let mut names = HashSet::new();
    for child in children {
        let name = child.file_name();
        let name = name
            .to_str()
            .ok_or_else(|| io::Error::other("FAT names must be UTF-8"))?;
        if name.encode_utf16().count() > 255
            || name.ends_with([' ', '.'])
            || name.chars().any(|c| {
                c.is_control()
                    || u32::from(c) > 0xffff
                    || matches!(c, '"' | '*' | '/' | ':' | '<' | '>' | '?' | '\\' | '|')
            })
        {
            return Err(io::Error::other(format!("Invalid FAT filename: {name:?}")));
        }
        if !names.insert(name.to_uppercase()) {
            return Err(io::Error::other(format!(
                "Case-insensitive filename collision: {name}"
            )));
        }
        let source = child.path();
        let metadata = fs::symlink_metadata(&source)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() && !metadata.is_dir() {
            return Err(io::Error::other(format!(
                "Boot tree contains a link or special file: {}",
                source.display()
            )));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(io::Error::other(
                    "Boot tree contains a Windows reparse point",
                ));
            }
        }
        let destination = if prefix.is_empty() {
            name.into()
        } else {
            format!("{prefix}/{name}")
        };
        let directory = metadata.is_dir();
        entries.push(Entry {
            source: source.clone(),
            destination: destination.clone(),
            directory,
            length: metadata.len(),
        });
        if entries.len() > MAX_ENTRIES {
            return Err(io::Error::other("Boot tree exceeds 4096 entries"));
        }
        if directory {
            collect(&source, &destination, depth + 1, entries)?;
        }
    }
    Ok(())
}

pub fn run(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 2 {
        return Err("Usage: efi-agent pack <ESP-directory> <new-disk.img>".into());
    }
    let root = Path::new(&args[0]).canonicalize()?;
    let output = Path::new(&args[1]);
    let filename = output
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Image output needs a UTF-8 filename")?;
    let stem = filename
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if filename.ends_with([' ', '.'])
        || filename
            .chars()
            .any(|c| c.is_control() || matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit()
    {
        return Err("Image output needs a regular file name".into());
    }
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if parent.canonicalize()?.starts_with(&root) {
        return Err("Image output must be outside the source boot tree".into());
    }
    if !root.join("EFI/BOOT/BOOTX64.EFI").is_file() {
        return Err("Boot tree must contain EFI/BOOT/BOOTX64.EFI".into());
    }
    let mut entries = Vec::new();
    collect(&root, "", 0, &mut entries)?;
    let source_bytes: u64 = entries
        .iter()
        .filter(|entry| !entry.directory)
        .map(|entry| entry.length)
        .try_fold(0u64, |sum, length| sum.checked_add(length))
        .ok_or("Boot tree size overflow")?;
    if source_bytes > MAX_SOURCE_BYTES {
        return Err("Boot tree exceeds 48 MiB".into());
    }
    // Never open a disk device or overwrite an existing artifact.
    let mut file = File::options()
        .read(true)
        .write(true)
        .create_new(true)
        .open(output)?;
    let result = build(&mut file, &entries);
    drop(file);
    if let Err(error) = result {
        fs::remove_file(output)?;
        return Err(error);
    }
    println!(
        "GPT/FAT32 image: {} ({} bytes, {} entries)",
        output.display(),
        DISK_BYTES,
        entries.len()
    );
    Ok(())
}

fn build(file: &mut File, entries: &[Entry]) -> Result<(), Box<dyn std::error::Error>> {
    file.set_len(DISK_BYTES)?;
    gpt::mbr::ProtectiveMBR::with_lb_size((DISK_BYTES / 512 - 1) as u32).overwrite_lba0(file)?;
    let mut disk = gpt::GptConfig::new()
        .writable(true)
        .logical_block_size(gpt::disk::LogicalBlockSize::Lb512)
        .create_from_device(&mut *file, None)?;
    let id = disk.add_partition(
        "EFI Agent",
        ESP_BYTES,
        gpt::partition_types::EFI,
        0,
        Some(2048),
    )?;
    let partition = &disk.partitions()[&id];
    let start = partition.bytes_start(gpt::disk::LogicalBlockSize::Lb512)?;
    let end = start + partition.bytes_len(gpt::disk::LogicalBlockSize::Lb512)?;
    disk.write()?;
    let mut volume = fatfs::StdIoWrapper::new(fscommon::StreamSlice::new(&mut *file, start, end)?);
    fatfs::format_volume(
        &mut volume,
        fatfs::FormatVolumeOptions::new()
            .fat_type(fatfs::FatType::Fat32)
            .bytes_per_cluster(512)
            .volume_label(*b"EFI AGENT  "),
    )?;
    let filesystem = fatfs::FileSystem::new(volume, fatfs::FsOptions::new())?;
    {
        let root = filesystem.root_dir();
        for entry in entries {
            if entry.directory {
                root.create_dir(&entry.destination)?;
            } else {
                let mut source = File::open(&entry.source)?;
                let mut destination =
                    fatfs::StdIoWrapper::new(root.create_file(&entry.destination)?);
                let mut copied = 0u64;
                let mut buffer = [0; 64 * 1024];
                loop {
                    let count = source.read(&mut buffer)?;
                    if count == 0 {
                        break;
                    }
                    destination.write_all(&buffer[..count])?;
                    copied += count as u64;
                }
                if copied != entry.length {
                    return Err(format!(
                        "Source changed during packaging: {}",
                        entry.source.display()
                    )
                    .into());
                }
                destination.flush()?;
            }
        }
    }
    filesystem.stats()?;
    filesystem.unmount()?;
    file.sync_all()?;
    Ok(())
}
