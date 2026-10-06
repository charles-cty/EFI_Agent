//! Build a private writable VM disk and persist guest workspace changes.
use efi_agent_core::config::AgentConfig;
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
type Error = Box<dyn std::error::Error>;

const SHELL_STARTUP: &[u8] = b"@echo -off\r\n# EDK II sets homefilesystem to the volume that loaded this Shell.\r\necho Connecting installed firmware drivers...\r\nconnect -r\r\n# Use the mandatory common text mode for display and serial console sinks.\r\nmode 80 25\r\necho Starting EFI Agent.\r\n\"%homefilesystem%\\EFI\\AGENT\\AGENT.EFI\"\r\necho EFI Agent returned. Shell commands are now available.\r\n";

fn shell_assets() -> Result<(Vec<u8>, Vec<u8>, Vec<u8>), Error> {
    let shell = std::env::var_os("EFI_AGENT_SHELL")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("vendor/uefi-shell/shellx64.efi"));
    let source = Path::new("vendor/uefi-shell");
    Ok((
        fs::read(&shell)?,
        fs::read(source.join("License.txt"))?,
        fs::read(source.join("README.md"))?,
    ))
}

pub fn configuration() -> Result<AgentConfig, Error> {
    let config = AgentConfig {
        api_base: std::env::var("EFI_AGENT_API_BASE").map_err(|_| "Set EFI_AGENT_API_BASE")?,
        api_key: std::env::var("EFI_AGENT_API_KEY").map_err(|_| "Set EFI_AGENT_API_KEY")?,
        model: std::env::var("EFI_AGENT_MODEL").map_err(|_| "Set EFI_AGENT_MODEL")?,
        api_format: std::env::var("EFI_AGENT_API_FORMAT")
            .unwrap_or_else(|_| "chat_completions".into()),
        reasoning_effort: std::env::var("EFI_AGENT_REASONING_EFFORT")
            .unwrap_or_else(|_| "medium".into()),
        dns_address: std::env::var("EFI_AGENT_DNS_ADDRESS")
            .unwrap_or_else(|_| "1.1.1.1".into())
            .parse::<std::net::Ipv4Addr>()?
            .octets(),
        dns_port: std::env::var("EFI_AGENT_DNS_PORT")
            .unwrap_or_else(|_| "53".into())
            .parse()?,
        ipv4: static_ipv4()?,
        workspace: "\\work".into(),
        ca_certificate: None,
    };
    config.validate()?;
    Ok(config)
}

fn static_ipv4() -> Result<Option<efi_agent_core::config::StaticIpv4>, Error> {
    let values = [
        std::env::var("EFI_AGENT_IPV4_ADDRESS").ok(),
        std::env::var("EFI_AGENT_IPV4_NETMASK").ok(),
        std::env::var("EFI_AGENT_IPV4_GATEWAY").ok(),
    ];
    if values.iter().all(Option::is_none) {
        return Ok(None);
    }
    if values.iter().any(Option::is_none) {
        return Err("Set EFI_AGENT_IPV4_ADDRESS, EFI_AGENT_IPV4_NETMASK, and EFI_AGENT_IPV4_GATEWAY together".into());
    }
    let parse = |name: &str, value: &str| -> Result<[u8; 4], Error> {
        Ok(value
            .parse::<std::net::Ipv4Addr>()
            .map_err(|_| format!("Invalid {name}"))?
            .octets())
    };
    Ok(Some(efi_agent_core::config::StaticIpv4 {
        address: parse("EFI_AGENT_IPV4_ADDRESS", values[0].as_deref().unwrap())?,
        subnet_mask: parse("EFI_AGENT_IPV4_NETMASK", values[1].as_deref().unwrap())?,
        gateway: parse("EFI_AGENT_IPV4_GATEWAY", values[2].as_deref().unwrap())?,
    }))
}

/// Generate the firmware config from the same host settings for both modes.
fn write_configuration(tree: &Path) -> Result<(), Error> {
    let mut config = configuration()?;
    let ca = std::env::var("EFI_AGENT_CA_CERTIFICATE")
        .ok()
        .map(fs::read)
        .transpose()?;
    let directory = tree.join("EFI/AGENT");
    fs::create_dir_all(&directory)?;
    let ca_path = directory.join("CA.DER");
    if let Some(bytes) = ca {
        fs::write(&ca_path, bytes)?;
        config.ca_certificate = Some("\\EFI\\AGENT\\CA.DER".into());
    } else if ca_path.exists() {
        fs::remove_file(&ca_path)?;
    }
    fs::write(
        directory.join("CONFIG.JSON"),
        serde_json::to_vec_pretty(&config)?,
    )?;
    Ok(())
}

pub struct Disk {
    pub image: PathBuf,
    directory: PathBuf,
    workspace: PathBuf,
    baseline: BTreeMap<PathBuf, Vec<u8>>,
    preserve: bool,
}
impl Disk {
    pub fn prepare(source: &Path, workspace: PathBuf) -> Result<Self, Error> {
        let directory = std::env::temp_dir().join(format!(
            "efi-agent-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        fs::create_dir(&directory)?;
        // Boot configuration contains credentials; restrict the temporary tree.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
        }
        let result = (|| {
            let tree = directory.join("esp");
            if source.is_dir() {
                copy_tree(source, &tree)?;
            } else if source
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("efi"))
            {
                fs::create_dir_all(tree.join("EFI/BOOT"))?;
                fs::copy(source, tree.join("EFI/BOOT/BOOTX64.EFI"))?;
            } else {
                extract(source, &tree)?;
            }
            // Native packages start in Shell. The VM uses the serial Agent
            // entry point even when its input is a generated native package.
            let application = tree.join("EFI/AGENT/AGENT.EFI");
            if application.is_file() {
                fs::copy(application, tree.join("EFI/BOOT/BOOTX64.EFI"))?;
            }
            fs::create_dir_all(tree.join("EFI/AGENT"))?;
            let work = tree.join("work");
            if work.exists() {
                fs::remove_dir_all(&work)?;
            }
            copy_tree(&workspace, &work)?;
            write_configuration(&tree)?;
            fs::write(tree.join("EFI/AGENT/VM.TXT"), b"serial\n")?;
            let image = directory.join("vm.img");
            crate::pack::run(vec![
                tree.to_string_lossy().into(),
                image.to_string_lossy().into(),
            ])?;
            let baseline = snapshot(&work)?;
            Ok(Self {
                image,
                directory: directory.clone(),
                workspace,
                baseline,
                preserve: false,
            })
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&directory);
        }
        result
    }
    pub fn save(&self) -> Result<(), Error> {
        let extracted = self.directory.join("saved");
        extract(&self.image, &extracted)?;
        let changed = snapshot(&extracted.join("work"))?;
        let current = snapshot(&self.workspace)?;
        // Validate all conflicts before overwriting any host file. Guest file
        // tools have no delete operation, so absent files do not delete host data.
        for (path, bytes) in &changed {
            if self.baseline.get(path) != Some(bytes)
                && current.get(path) != self.baseline.get(path)
            {
                return Err(
                    format!("Host workspace changed during VM run: {}", path.display()).into(),
                );
            }
        }
        for (path, bytes) in changed {
            if self.baseline.get(&path) == Some(&bytes) {
                continue;
            }
            let destination = self.workspace.join(&path);
            let parent = destination.parent().ok_or("Missing workspace parent")?;
            fs::create_dir_all(parent)?;
            let temporary = parent.join(format!(".efi-agent-{}.tmp", std::process::id()));
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            let result = (|| {
                output.write_all(&bytes)?;
                output.sync_all()?;
                drop(output);
                fs::rename(&temporary, &destination)
            })(); // Windows rename replaces a regular existing file.
            if result.is_err() {
                let _ = fs::remove_file(&temporary);
            }
            result?;
        }
        Ok(())
    }
    pub fn preserve(&mut self) {
        self.preserve = true;
    }
    pub fn cleanup(&self) -> Result<(), Error> {
        fs::remove_dir_all(&self.directory)?;
        Ok(())
    }
}
impl Drop for Disk {
    fn drop(&mut self) {
        if !self.preserve {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }
}
const MAX_TREE_BYTES: u64 = 48 * 1024 * 1024;
const MAX_TREE_ENTRIES: usize = 4096;
#[derive(Default)]
struct Budget {
    bytes: u64,
    entries: usize,
}
impl Budget {
    fn entry(&mut self, bytes: u64, depth: usize) -> Result<(), Error> {
        self.bytes = self.bytes.checked_add(bytes).ok_or("Tree size overflow")?;
        self.entries += 1;
        if self.bytes > MAX_TREE_BYTES || self.entries > MAX_TREE_ENTRIES || depth > 32 {
            return Err("Tree exceeds 48 MiB, 4096 entries, or 32 directory levels".into());
        }
        Ok(())
    }
}
fn checked_metadata(path: &Path) -> Result<fs::Metadata, Error> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() && !metadata.is_dir() {
        return Err("Tree contains a link or special file".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("Tree contains a Windows reparse point".into());
        }
    }
    Ok(metadata)
}
fn copy_tree(source: &Path, destination: &Path) -> Result<(), Error> {
    fn copy(
        source: &Path,
        destination: &Path,
        depth: usize,
        budget: &mut Budget,
    ) -> Result<(), Error> {
        checked_metadata(source)?;
        fs::create_dir_all(destination)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let metadata = checked_metadata(&entry.path())?;
            budget.entry(
                if metadata.is_file() {
                    metadata.len()
                } else {
                    0
                },
                depth,
            )?;
            let target = destination.join(entry.file_name());
            if metadata.is_dir() {
                copy(&entry.path(), &target, depth + 1, budget)?;
            } else {
                fs::copy(entry.path(), target)?;
            }
        }
        Ok(())
    }
    copy(source, destination, 0, &mut Budget::default())
}
fn snapshot(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>, Error> {
    fn collect(
        root: &Path,
        path: &Path,
        depth: usize,
        result: &mut BTreeMap<PathBuf, Vec<u8>>,
        budget: &mut Budget,
    ) -> Result<(), Error> {
        checked_metadata(path)?;
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let metadata = checked_metadata(&entry.path())?;
            budget.entry(
                if metadata.is_file() {
                    metadata.len()
                } else {
                    0
                },
                depth,
            )?;
            if metadata.is_dir() {
                collect(root, &entry.path(), depth + 1, result, budget)?;
            } else {
                result.insert(
                    entry.path().strip_prefix(root)?.into(),
                    fs::read(entry.path())?,
                );
            }
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    collect(root, root, 0, &mut files, &mut Budget::default())?;
    Ok(files)
}

fn reserved_name(name: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}

pub fn extract(image: &Path, destination: &Path) -> Result<(), Error> {
    let disk = gpt::GptConfig::new().open(image)?;
    let partition = disk
        .partitions()
        .values()
        .find(|p| p.part_type_guid == gpt::partition_types::EFI)
        .ok_or("Disk has no EFI partition")?;
    let start = partition.bytes_start(gpt::disk::LogicalBlockSize::Lb512)?;
    let end = start + partition.bytes_len(gpt::disk::LogicalBlockSize::Lb512)?;
    let file = fs::File::open(image)?;
    let volume = fatfs::StdIoWrapper::new(fscommon::StreamSlice::new(file, start, end)?);
    let filesystem = fatfs::FileSystem::new(volume, fatfs::FsOptions::new())?;
    let mut budget = Budget::default();
    fn directory<T: fatfs::ReadWriteSeek<Error = std::io::Error>>(
        dir: fatfs::Dir<'_, T, fatfs::DefaultTimeProvider, fatfs::LossyOemCpConverter>,
        path: &Path,
        depth: usize,
        budget: &mut Budget,
    ) -> Result<(), Error> {
        if depth > 32 {
            return Err("Boot tree is too deep".into());
        }
        fs::create_dir_all(path)?;
        for entry in dir.iter() {
            let entry = entry?;
            let name = entry.file_name();
            if name == "." || name == ".." {
                continue;
            }
            if name.is_empty()
                || name.ends_with([' ', '.'])
                || name.chars().any(|c| {
                    c.is_control()
                        || matches!(c, '/' | '\\' | ':' | '*' | '?' | '\"' | '<' | '>' | '|')
                })
                || reserved_name(&name)
            {
                return Err("Invalid FAT filename".into());
            }
            let target = path.join(name);
            if entry.is_dir() {
                budget.entry(0, depth)?;
                directory(entry.to_dir(), &target, depth + 1, budget)?;
            } else {
                let bytes = entry.len() as usize;
                budget.entry(bytes as u64, depth)?;
                let mut input = entry.to_file();
                let mut output = fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(target)?;
                let mut buffer = [0; 65536];
                loop {
                    let count = fatfs::Read::read(&mut input, &mut buffer)?;
                    if count == 0 {
                        break;
                    }
                    output.write_all(&buffer[..count])?;
                }
            }
        }
        Ok(())
    }
    directory(filesystem.root_dir(), destination, 0, &mut budget)?;
    filesystem.unmount()?;
    Ok(())
}

/// Assemble distinct VM and native packages from one compiled EFI application.
pub fn package(args: Vec<String>) -> Result<(), Error> {
    if args.len() != 2 {
        return Err("Usage: efi-agent package <application.efi> <output-directory>".into());
    }
    let efi = Path::new(&args[0]);
    configuration()?;
    let (shell, license, source) = shell_assets()?;
    let output = Path::new(&args[1]);
    for (mode, name) in [("vm", "esp"), ("native", "native-esp")] {
        let tree = output.join(name);
        fs::create_dir_all(tree.join("EFI/BOOT"))?;
        fs::create_dir_all(tree.join("EFI/AGENT"))?;
        fs::create_dir_all(tree.join("EFI/AGENT/DRIVERS"))?;
        fs::create_dir_all(tree.join("EFI/TOOLS"))?;
        fs::copy(efi, tree.join("EFI/AGENT/AGENT.EFI"))?;
        fs::write(tree.join("EFI/TOOLS/SHELLX64.EFI"), &shell)?;
        fs::write(tree.join("EFI/TOOLS/SHELL-LICENSE.TXT"), &license)?;
        fs::write(tree.join("EFI/TOOLS/SHELL-SOURCE.TXT"), &source)?;
        if mode == "vm" {
            fs::copy(efi, tree.join("EFI/BOOT/BOOTX64.EFI"))?;
            fs::write(tree.join("EFI/AGENT/VM.TXT"), b"serial\n")?;
            // VM configuration is injected into a private disk at launch.
            let config_path = tree.join("EFI/AGENT/CONFIG.JSON");
            if config_path.exists() {
                fs::remove_file(config_path)?;
            }
        } else {
            fs::write(tree.join("EFI/BOOT/BOOTX64.EFI"), &shell)?;
            fs::write(tree.join("EFI/BOOT/startup.nsh"), SHELL_STARTUP)?;
            let marker = tree.join("EFI/AGENT/VM.TXT");
            if marker.exists() {
                fs::remove_file(marker)?;
            }
            write_configuration(&tree)?;
            fs::create_dir_all(tree.join("work"))?;
        }
        let image = output.join(format!("efi-agent-{mode}.img"));
        let temporary = image.with_extension(format!("img.{}.tmp", std::process::id()));
        let result = (|| {
            crate::pack::run(vec![
                tree.to_string_lossy().into(),
                temporary.to_string_lossy().into(),
            ])?;
            fs::rename(&temporary, &image)?;
            Ok::<_, Error>(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_fat_save_preserves_host_changes_and_refuses_conflicting_edits() {
        for conflict in [false, true] {
            let directory = std::env::temp_dir().join(format!(
                "efi-save-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let workspace = directory.join("host");
            let tree = directory.join("tree");
            fs::create_dir_all(&workspace).unwrap();
            fs::create_dir_all(tree.join("EFI/BOOT")).unwrap();
            fs::create_dir_all(tree.join("work")).unwrap();
            fs::write(tree.join("EFI/BOOT/BOOTX64.EFI"), b"test EFI").unwrap();
            fs::write(workspace.join("edited.txt"), b"original").unwrap();
            fs::write(workspace.join("unchanged.txt"), b"original other").unwrap();
            let baseline = snapshot(&workspace).unwrap();
            fs::write(tree.join("work/edited.txt"), "guest 中\n").unwrap();
            fs::write(tree.join("work/new.txt"), b"new").unwrap();
            fs::write(tree.join("work/unchanged.txt"), b"original other").unwrap();
            // An unrelated host edit must survive even though the guest still
            // contains the old copy. A conflicting host edit rejects the batch.
            fs::write(workspace.join("unchanged.txt"), b"host newer").unwrap();
            if conflict {
                fs::write(workspace.join("edited.txt"), b"host conflict").unwrap();
            }
            let image = directory.join("disk.img");
            crate::pack::run(vec![
                tree.to_string_lossy().into(),
                image.to_string_lossy().into(),
            ])
            .unwrap();
            let disk = Disk {
                image,
                directory,
                workspace: workspace.clone(),
                baseline,
                preserve: false,
            };
            let result = disk.save();
            assert_eq!(result.is_err(), conflict);
            assert_eq!(
                fs::read(workspace.join("unchanged.txt")).unwrap(),
                b"host newer"
            );
            if conflict {
                assert_eq!(
                    fs::read(workspace.join("edited.txt")).unwrap(),
                    b"host conflict"
                );
                assert!(!workspace.join("new.txt").exists());
            } else {
                assert_eq!(
                    fs::read(workspace.join("edited.txt")).unwrap(),
                    "guest 中\n".as_bytes()
                );
                assert_eq!(fs::read(workspace.join("new.txt")).unwrap(), b"new");
            }
        }
    }
}
