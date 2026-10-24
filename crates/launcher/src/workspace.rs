//! Logical workspace snapshots and conflict-checked host persistence.
use efi_agent_filesystem::{
    EntryType, FileSystem, Metadata,
    namespace::{Namespace, read_snapshot},
    native::NativeFileSystem,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::Path,
    rc::Rc,
    time::{SystemTime, UNIX_EPOCH},
};

type Error = Box<dyn std::error::Error>;
const HOST_METADATA: &str = ".efi-agent-workspace.json";
const MAX_BYTES: u64 = 48 * 1024 * 1024;
const MAX_ENTRIES: usize = 4096;
const MAX_METADATA: u64 = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Contents {
    File(Rc<[u8]>),
    Directory,
    Link(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attributes {
    pub(crate) mode: u16,
    pub(crate) access: u64,
    pub(crate) modify: u64,
    pub(crate) create: u64,
}

impl From<Metadata> for Attributes {
    fn from(info: Metadata) -> Self {
        Self {
            mode: info.mode,
            access: info.accessed,
            modify: info.modified,
            create: info.created,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub contents: Contents,
    attributes: Attributes,
    hard_link: Option<String>,
}

impl Entry {
    // Reading a host file can update atime. It must not create a save conflict.
    fn host_matches(&self, other: &Self) -> bool {
        self.contents == other.contents
            && self.hard_link == other.hard_link
            && self.attributes.mode == other.attributes.mode
            && self.attributes.modify == other.attributes.modify
    }
}

pub type Snapshot = BTreeMap<String, Entry>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Observed {
    directory: bool,
    size: u64,
    modified: u64,
    readonly: bool,
    digest: [u8; 32],
}

fn nanos(time: Result<SystemTime, std::io::Error>) -> u64 {
    time.ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .and_then(|value| value.as_nanos().try_into().ok())
        .unwrap_or(0)
}

fn observed(info: &fs::Metadata, contents: &Contents) -> Observed {
    let digest = match contents {
        Contents::Directory => [0; 32],
        Contents::File(bytes) => Sha256::digest(bytes.as_ref()).into(),
        Contents::Link(target) => Sha256::digest(target.as_bytes()).into(),
    };
    Observed {
        directory: info.is_dir(),
        size: if info.is_dir() { 0 } else { info.len() },
        modified: nanos(info.modified()),
        readonly: info.permissions().readonly(),
        digest,
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cached {
    observed: Observed,
    attributes: Attributes,
    link: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HostMetadata {
    format: String,
    version: u32,
    entries: BTreeMap<String, Cached>,
}

fn valid_path(path: &str) -> Result<(), Error> {
    if path.is_empty() || path.len() > 4096 || path.split('/').count() > 32 {
        return Err(format!("Invalid workspace path: {path:?}").into());
    }
    for name in path.split('/') {
        let stem = name
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        if name.is_empty()
            || matches!(name, "." | "..")
            || name.ends_with([' ', '.'])
            || name.encode_utf16().count() > 255
            || name.chars().any(|c| {
                c.is_control()
                    || u32::from(c) > 0xffff
                    || matches!(c, '\\' | ':' | '"' | '*' | '?' | '<' | '>' | '|')
            })
            || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || ((stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.len() == 4
                && matches!(stem.as_bytes()[3], b'1'..=b'9'))
        {
            return Err(format!("Unrepresentable workspace path: {path:?}").into());
        }
    }
    let first = path.split('/').next().unwrap();
    if [HOST_METADATA, ".efi-agent-fs", ".efi-process"]
        .iter()
        .any(|reserved| first.eq_ignore_ascii_case(reserved))
    {
        return Err(format!("Reserved workspace path: {path}").into());
    }
    Ok(())
}

fn validate(tree: &Snapshot) -> Result<(), Error> {
    if tree.len() > MAX_ENTRIES {
        return Err("Workspace exceeds 4096 entries".into());
    }
    let mut bytes = 0u64;
    let mut names = BTreeSet::new();
    for (path, entry) in tree {
        valid_path(path)?;
        if !names.insert(path.to_uppercase()) {
            return Err(format!("Case-insensitive workspace collision: {path}").into());
        }
        if let Some((parent, _)) = path.rsplit_once('/')
            && !matches!(
                tree.get(parent).map(|entry| &entry.contents),
                Some(Contents::Directory)
            )
        {
            return Err(format!("Workspace parent is absent or replaced: {parent}").into());
        }
        let size = match &entry.contents {
            Contents::File(bytes) => bytes.len(),
            Contents::Directory => 0,
            Contents::Link(target) => {
                if target.is_empty() || target.len() > 4096 || target.contains(['\0', '\\']) {
                    return Err(format!("Invalid workspace link: {path}").into());
                }
                target.len()
            }
        };
        if let Some(target) = &entry.hard_link {
            let canonical = tree.get(target).ok_or("Missing hard-link target")?;
            if target >= path
                || canonical.hard_link.is_some()
                || matches!(entry.contents, Contents::Directory)
                || entry.contents != canonical.contents
                || entry.attributes != canonical.attributes
            {
                return Err(format!("Invalid workspace hard-link group: {path}").into());
            }
        }
        bytes = bytes
            .checked_add(if entry.hard_link.is_some() {
                0
            } else {
                size as u64
            })
            .ok_or("Workspace size overflow")?;
        if bytes > MAX_BYTES || entry.attributes.mode & !0o7777 != 0 {
            return Err("Workspace limits or mode are invalid".into());
        }
    }
    Ok(())
}

type Identity = (u64, u64);

#[derive(Default)]
struct HostObjects {
    bytes: u64,
    groups: BTreeMap<Identity, Vec<String>>,
    contents: BTreeMap<Identity, Rc<[u8]>>,
}

// Canonical names depend on path order, never on host iteration or inode IDs.
fn group_links(tree: &mut Snapshot, groups: BTreeMap<Identity, Vec<String>>) -> Result<(), Error> {
    for mut paths in groups.into_values() {
        paths.sort();
        let canonical = &paths[0];
        let original = tree[canonical].clone();
        for path in &paths[1..] {
            let entry = tree.get_mut(path).unwrap();
            if entry.contents != original.contents
                || entry.attributes.mode != original.attributes.mode
                || entry.attributes.modify != original.attributes.modify
            {
                return Err(format!("Inconsistent hard-link metadata: {path}").into());
            }
            entry.attributes = original.attributes.clone();
            entry.hard_link = Some(canonical.clone());
        }
    }
    Ok(())
}

fn regular_metadata(path: &Path) -> Result<fs::Metadata, Error> {
    let info = fs::symlink_metadata(path)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if info.file_attributes() & 0x400 != 0 {
            return Err(format!("Workspace reparse point: {}", path.display()).into());
        }
    }
    if !info.is_file() && !info.is_dir() {
        return Err(format!("Workspace special entry: {}", path.display()).into());
    }
    Ok(info)
}

fn load_host_metadata(root: &Path) -> Result<BTreeMap<String, Cached>, Error> {
    let path = root.join(HOST_METADATA);
    let info = match fs::symlink_metadata(&path) {
        Ok(_) => regular_metadata(&path)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(error) => return Err(error.into()),
    };
    if !info.is_file() || info.len() > MAX_METADATA {
        return Err("Invalid host workspace metadata".into());
    }
    let metadata: HostMetadata = serde_json::from_slice(&fs::read(path)?)?;
    if metadata.format != "EFI Agent workspace"
        || metadata.version != 1
        || metadata.entries.len() > MAX_ENTRIES
    {
        return Err("Unrecognized host workspace metadata".into());
    }
    for (path, entry) in &metadata.entries {
        valid_path(path)?;
        if entry.attributes.mode & !0o7777 != 0 {
            return Err("Invalid cached workspace mode".into());
        }
    }
    Ok(metadata.entries)
}

pub fn host(root: &Path) -> Result<Snapshot, Error> {
    regular_metadata(root)?;
    let cached = load_host_metadata(root)?;
    let provider = NativeFileSystem(root.into());
    let mut result = Snapshot::new();
    let mut objects = HostObjects::default();
    fn walk(
        root: &Path,
        prefix: &str,
        provider: &NativeFileSystem,
        cached: &BTreeMap<String, Cached>,
        result: &mut Snapshot,
        objects: &mut HostObjects,
    ) -> Result<(), Error> {
        let directory = root.join(prefix);
        for child in fs::read_dir(&directory)? {
            let child = child?;
            let name = child
                .file_name()
                .into_string()
                .map_err(|_| "Workspace names must be UTF-8")?;
            if prefix.is_empty() && name == HOST_METADATA {
                continue;
            }
            let path = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            valid_path(&path)?;
            let info = regular_metadata(&child.path())?;
            let raw = provider
                .metadata(&path)
                .map_err(|error| format!("Workspace stat {path}: {error:?}"))?;
            let identity = (raw.device, raw.inode);
            let shared = info.is_file() && raw.inode != 0;
            objects.bytes = objects
                .bytes
                .checked_add(
                    if info.is_file() && (!shared || !objects.contents.contains_key(&identity)) {
                        info.len()
                    } else {
                        0
                    },
                )
                .ok_or("Workspace size overflow")?;
            if objects.bytes > MAX_BYTES || result.len() >= MAX_ENTRIES {
                return Err("Workspace exceeds size or entry limits".into());
            }
            let mut contents = if info.is_dir() {
                Contents::Directory
            } else {
                let bytes = match objects.contents.get(&identity).filter(|_| shared) {
                    Some(bytes) => bytes.clone(),
                    None => Rc::from(fs::read(child.path())?),
                };
                if shared {
                    objects
                        .groups
                        .entry(identity)
                        .or_default()
                        .push(path.clone());
                    objects.contents.insert(identity, bytes.clone());
                }
                Contents::File(bytes)
            };
            let mut attributes = Attributes::from(raw);
            if let Some(cache) = cached
                .get(&path)
                .filter(|cache| cache.observed == observed(&info, &contents))
            {
                attributes = cache.attributes.clone();
                if let Some(target) = &cache.link {
                    if contents != Contents::File(Rc::from(target.as_bytes())) {
                        return Err(format!("Workspace link placeholder changed: {path}").into());
                    }
                    contents = Contents::Link(target.clone());
                }
            }
            result.insert(
                path.clone(),
                Entry {
                    contents,
                    attributes,
                    hard_link: None,
                },
            );
            if info.is_dir() {
                walk(root, &path, provider, cached, result, objects)?;
            }
        }
        Ok(())
    }
    walk(root, "", &provider, &cached, &mut result, &mut objects)?;
    group_links(&mut result, objects.groups)?;
    validate(&result)?;
    Ok(result)
}

pub fn guest(root: &Path, baseline: &Snapshot) -> Result<Snapshot, Error> {
    let provider = NativeFileSystem(root.into());
    let private = fs::symlink_metadata(root.join(".efi-agent-fs"));
    if matches!(&private, Err(error) if error.kind() == std::io::ErrorKind::NotFound) {
        let mut tree = host(root)?;
        // Plain FAT staging does not carry logical attributes. Preserve the
        // baseline attributes for unchanged entries until namespace mounting.
        for (path, entry) in &mut tree {
            if let Some(old) = baseline
                .get(path)
                .filter(|old| old.contents == entry.contents)
            {
                entry.attributes = old.attributes.clone();
            }
        }
        return Ok(tree);
    }
    private?;
    let entries = read_snapshot(&provider)
        .map_err(|error| format!("Read guest namespace: {error:?}"))?
        .ok_or("Guest namespace has no committed generation")?;
    let mut result = Snapshot::new();
    let mut bytes = 0u64;
    let mut groups: BTreeMap<Identity, Vec<String>> = BTreeMap::new();
    let mut objects: BTreeMap<Identity, Contents> = BTreeMap::new();
    for entry in entries {
        if entry.path.is_empty() {
            continue;
        }
        valid_path(&entry.path)?;
        let identity = (entry.metadata.device, entry.metadata.inode);
        let shared = entry.metadata.kind != EntryType::Directory;
        bytes = bytes
            .checked_add(if objects.contains_key(&identity) {
                0
            } else {
                entry.metadata.size
            })
            .ok_or("Guest workspace size overflow")?;
        if bytes > MAX_BYTES || result.len() >= MAX_ENTRIES {
            return Err("Guest workspace exceeds limits".into());
        }
        let contents = if let Some(contents) = objects.get(&identity).filter(|_| shared) {
            contents.clone()
        } else {
            match entry.metadata.kind {
                EntryType::File => Contents::File(Rc::from(fs::read(
                    root.join(entry.backing.ok_or("Missing guest backing")?),
                )?)),
                EntryType::Directory => Contents::Directory,
                EntryType::Symlink => {
                    Contents::Link(entry.target.ok_or("Missing guest link target")?)
                }
                _ => return Err("Unsupported guest entry kind".into()),
            }
        };
        if shared {
            groups.entry(identity).or_default().push(entry.path.clone());
            objects.insert(identity, contents.clone());
        }
        result.insert(
            entry.path,
            Entry {
                contents,
                attributes: entry.metadata.into(),
                hard_link: None,
            },
        );
    }
    group_links(&mut result, groups)?;
    validate(&result)?;
    Ok(result)
}

/// Prepare FAT backing objects and committed metadata from a logical host tree.
/// The destination is a new private staging directory.
pub fn stage(tree: &Snapshot, destination: &Path) -> Result<(), Error> {
    validate(tree)?;
    fs::create_dir(destination)?;
    // This private directory is not observable by guests. Import ordinary
    // files once instead of committing the full journal for each library file.
    for (path, entry) in tree {
        if entry.hard_link.is_some() {
            continue;
        }
        match &entry.contents {
            Contents::Directory => fs::create_dir(destination.join(path))?,
            Contents::File(bytes) => {
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(destination.join(path))?;
                for chunk in bytes.chunks(65536) {
                    file.write_all(chunk)?;
                }
                file.sync_all()?;
            }
            Contents::Link(_) => (),
        }
    }
    let namespace = Namespace::open(Rc::new(NativeFileSystem(destination.into())), None)
        .map_err(|error| format!("Initialize staged workspace: {error:?}"))?;
    for (path, entry) in tree {
        if entry.hard_link.is_none()
            && let Contents::Link(target) = &entry.contents
        {
            namespace
                .symlink(target, path)
                .map_err(|error| format!("Stage link {path}: {error:?}"))?;
        }
    }
    for (path, entry) in tree {
        if let Some(target) = &entry.hard_link {
            namespace
                .hard_link(target, path, false)
                .map_err(|error| format!("Stage hard link {path}: {error:?}"))?;
        }
    }
    // Restore permissions after creation so restrictive parents remain usable
    // during staging. Final symlinks receive their own metadata.
    let metadata: Vec<_> = tree
        .iter()
        .map(|(path, entry)| {
            let kind = match entry.contents {
                Contents::Directory => EntryType::Directory,
                Contents::Link(_) => EntryType::Symlink,
                Contents::File(_) => EntryType::File,
            };
            let mut metadata = Metadata::new(kind, 0);
            metadata.mode = entry.attributes.mode;
            metadata.accessed = entry.attributes.access;
            metadata.modified = entry.attributes.modify;
            metadata.created = entry.attributes.create;
            (path.clone(), metadata)
        })
        .collect();
    namespace
        .restore_metadata(&metadata)
        .map_err(|error| format!("Restore staged metadata: {error:?}"))?;
    namespace
        .flush()
        .map_err(|error| format!("Flush staged workspace: {error:?}"))?;
    if !namespace.take_errors().is_empty() {
        return Err("Staged workspace close failed".into());
    }
    Ok(())
}

pub fn merge(
    baseline: &Snapshot,
    current: &Snapshot,
    changed: &Snapshot,
) -> Result<Snapshot, Error> {
    validate(baseline)?;
    validate(current)?;
    validate(changed)?;
    let mut result = current.clone();
    let paths: BTreeSet<_> = baseline.keys().chain(changed.keys()).collect();
    for path in paths {
        let before = baseline.get(path);
        let after = changed.get(path);
        if before == after {
            continue;
        }
        let now = current.get(path);
        let same = |a: Option<&Entry>, b: Option<&Entry>| match (a, b) {
            (Some(a), Some(b)) => a.host_matches(b),
            (None, None) => true,
            _ => false,
        };
        if !same(now, before) && !same(now, after) {
            return Err(format!("Host workspace changed during VM run: {path}").into());
        }
        match after {
            Some(entry) => {
                result.insert(path.clone(), entry.clone());
            }
            None => {
                result.remove(path);
            }
        }
    }
    validate(&result)?;
    Ok(result)
}

fn replace_file(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let parent = path.parent().ok_or("Missing workspace parent")?;
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let temporary = parent.join(format!(".efi-agent-{}-{nonce}.tmp", std::process::id()));
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let result = (|| {
        output.write_all(bytes)?;
        output.sync_all()?;
        drop(output);
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn save(root: &Path, baseline: &Snapshot, changed: &Snapshot) -> Result<(), Error> {
    let current = host(root)?;
    let desired = merge(baseline, &current, changed)?;
    // Check the whole merge before changes. Remove children before parents;
    // never recurse through a host directory that gained an unknown child.
    let mut removals: Vec<_> = current
        .iter()
        .filter(|(path, entry)| {
            desired.get(*path).is_none_or(|next| {
                matches!(entry.contents, Contents::Directory)
                    != matches!(next.contents, Contents::Directory)
            })
        })
        .collect();
    removals.sort_by_key(|(path, _)| std::cmp::Reverse(path.split('/').count()));
    for (path, entry) in removals {
        regular_metadata(&root.join(path))?;
        if matches!(entry.contents, Contents::Directory) {
            fs::remove_dir(root.join(path))?;
        } else {
            fs::remove_file(root.join(path))?;
        }
    }
    for (path, entry) in &desired {
        if entry.hard_link.is_some() {
            continue;
        }
        if current
            .get(path)
            .is_some_and(|old| old.contents == entry.contents && old.hard_link.is_none())
        {
            continue;
        }
        let destination = root.join(path);
        if let Some((parent, _)) = path.rsplit_once('/')
            && !regular_metadata(&root.join(parent))?.is_dir()
        {
            return Err(format!("Workspace parent changed: {parent}").into());
        }
        match &entry.contents {
            Contents::Directory => {
                fs::create_dir(&destination)?;
            }
            Contents::File(bytes) => replace_file(&destination, bytes)?,
            Contents::Link(target) => replace_file(&destination, target.as_bytes())?,
        }
    }
    // Rebuild aliases after every canonical replacement. Atomic file replacement
    // creates a new host inode even when its former aliases remain open.
    for (path, entry) in &desired {
        if let Some(target) = &entry.hard_link {
            let destination = root.join(path);
            if let Some((parent, _)) = path.rsplit_once('/')
                && !regular_metadata(&root.join(parent))?.is_dir()
            {
                return Err(format!("Workspace parent changed: {parent}").into());
            }
            match fs::symlink_metadata(&destination) {
                Ok(_) => {
                    regular_metadata(&destination)?;
                    fs::remove_file(&destination)?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(error.into()),
            }
            regular_metadata(&root.join(target))?;
            fs::hard_link(root.join(target), destination)?;
        }
    }
    // POSIX attributes and links have one representation on Windows and Unix.
    // Link placeholders are never followed by host copies or snapshot readers.
    let mut entries = BTreeMap::new();
    for (path, entry) in desired {
        let info = regular_metadata(&root.join(&path))?;
        entries.insert(
            path,
            Cached {
                observed: observed(&info, &entry.contents),
                attributes: entry.attributes,
                link: match entry.contents {
                    Contents::Link(target) => Some(target),
                    _ => None,
                },
            },
        );
    }
    let bytes = serde_json::to_vec_pretty(&HostMetadata {
        format: "EFI Agent workspace".into(),
        version: 1,
        entries,
    })?;
    if bytes.len() as u64 > MAX_METADATA {
        return Err("Host workspace metadata exceeds limit".into());
    }
    replace_file(&root.join(HOST_METADATA), &bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use efi_agent_filesystem::{OpenOptions, TimeUpdate, namespace::Namespace};
    use std::{path::PathBuf, rc::Rc};

    struct Work(PathBuf);
    impl Work {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path =
                std::env::temp_dir().join(format!("efi-workspace-{}-{nonce}", std::process::id()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Work {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    fn entry(contents: Contents) -> Entry {
        Entry {
            contents,
            hard_link: None,
            attributes: Attributes {
                mode: 0o751,
                access: 193,
                modify: 853,
                create: 731,
            },
        }
    }
    fn file(bytes: &[u8]) -> Entry {
        entry(Contents::File(Rc::from(bytes)))
    }

    #[test]
    fn staged_namespace_survives_fat_save_reload_and_restage_with_all_attributes() {
        let work = Work::new();
        let tree = work.0.join("tree");
        fs::create_dir_all(tree.join("EFI/BOOT")).unwrap();
        fs::write(tree.join("EFI/BOOT/BOOTX64.EFI"), b"test EFI").unwrap();
        let mut expected = Snapshot::from([
            ("dir".into(), entry(Contents::Directory)),
            ("dir/empty".into(), entry(Contents::Directory)),
            ("dir/data".into(), file(b"binary\0\xff\n")),
            ("link".into(), entry(Contents::Link("dir/data".into()))),
            ("dangling".into(), entry(Contents::Link("absent".into()))),
        ]);
        // Restrictive parents must not prevent creation of descendants, and
        // symlink metadata must not be applied to its target.
        expected.get_mut("dir").unwrap().attributes.mode = 0o400;
        expected.get_mut("link").unwrap().attributes.mode = 0o123;
        let mut alias = expected["dir/data"].clone();
        alias.hard_link = Some("dir/data".into());
        expected.insert("hard-file".into(), alias);
        let mut alias = expected["link"].clone();
        alias.hard_link = Some("link".into());
        expected.insert("linked-symlink".into(), alias);
        stage(&expected, &tree.join("work")).unwrap();
        let image = work.0.join("disk.img");
        crate::pack::run(vec![
            tree.to_string_lossy().into(),
            image.to_string_lossy().into(),
        ])
        .unwrap();
        let extracted = work.0.join("extracted");
        crate::boot::extract(&image, &extracted).unwrap();
        let exported = guest(&extracted.join("work"), &Snapshot::new()).unwrap();
        assert_eq!(exported, expected);
        let host_root = work.0.join("host");
        fs::create_dir(&host_root).unwrap();
        save(&host_root, &Snapshot::new(), &exported).unwrap();
        let reloaded = host(&host_root).unwrap();
        assert_eq!(reloaded, expected);
        let restaged = work.0.join("restaged");
        stage(&reloaded, &restaged).unwrap();
        assert_eq!(guest(&restaged, &Snapshot::new()).unwrap(), expected);
        let provider = NativeFileSystem(host_root);
        assert_eq!(
            provider.metadata("hard-file").unwrap().inode,
            provider.metadata("dir/data").unwrap().inode
        );
        assert_eq!(
            provider.metadata("link").unwrap().inode,
            provider.metadata("linked-symlink").unwrap().inode
        );
    }

    #[test]
    fn host_hard_links_preserve_identity_and_save_can_split_and_rejoin_groups() {
        let work = Work::new();
        fs::create_dir(work.0.join("dir")).unwrap();
        fs::write(work.0.join("dir/original"), b"shared").unwrap();
        fs::hard_link(work.0.join("dir/original"), work.0.join("alias")).unwrap();
        let baseline = host(&work.0).unwrap();
        assert_eq!(baseline["dir/original"].hard_link.as_deref(), Some("alias"));
        let mut changed = baseline.clone();
        for path in ["alias", "dir/original"] {
            changed.get_mut(path).unwrap().contents =
                Contents::File(Rc::from(b"updated".as_slice()));
        }
        save(&work.0, &baseline, &changed).unwrap();
        let shared = host(&work.0).unwrap();
        let provider = NativeFileSystem(work.0.clone());
        assert_eq!(
            provider.metadata("alias").unwrap().inode,
            provider.metadata("dir/original").unwrap().inode
        );
        assert_eq!(fs::read(work.0.join("dir/original")).unwrap(), b"updated");
        let mut split = shared.clone();
        split.get_mut("dir/original").unwrap().hard_link = None;
        save(&work.0, &shared, &split).unwrap();
        assert_ne!(
            provider.metadata("alias").unwrap().inode,
            provider.metadata("dir/original").unwrap().inode
        );
        let split = host(&work.0).unwrap();
        let mut rejoined = split.clone();
        rejoined.get_mut("dir/original").unwrap().hard_link = Some("alias".into());
        save(&work.0, &split, &rejoined).unwrap();
        assert_eq!(host(&work.0).unwrap(), rejoined);
        assert_eq!(
            provider.metadata("alias").unwrap().inode,
            provider.metadata("dir/original").unwrap().inode
        );
    }

    #[test]
    fn host_identity_changes_conflict_even_with_equal_bytes_and_timestamps() {
        let work = Work::new();
        fs::write(work.0.join("a"), b"original").unwrap();
        fs::hard_link(work.0.join("a"), work.0.join("b")).unwrap();
        let baseline = host(&work.0).unwrap();
        let mut changed = baseline.clone();
        for path in ["a", "b"] {
            changed.get_mut(path).unwrap().contents = Contents::File(Rc::from(b"guest".as_slice()));
        }
        let modified = fs::metadata(work.0.join("b")).unwrap().modified().unwrap();
        replace_file(&work.0.join("b"), b"original").unwrap();
        fs::File::options()
            .write(true)
            .open(work.0.join("b"))
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(modified))
            .unwrap();
        assert!(save(&work.0, &baseline, &changed).is_err());
        assert_eq!(fs::read(work.0.join("a")).unwrap(), b"original");
        assert_eq!(fs::read(work.0.join("b")).unwrap(), b"original");
        assert!(!work.0.join(HOST_METADATA).exists());
    }

    #[test]
    fn hard_link_groups_count_bytes_once_and_reject_inconsistent_snapshots_before_staging() {
        let work = Work::new();
        let original = entry(Contents::File(Rc::from(vec![193; 25 * 1024 * 1024])));
        let mut alias = original.clone();
        alias.hard_link = Some("a".into());
        let valid = Snapshot::from([("a".into(), original), ("b".into(), alias)]);
        validate(&valid).unwrap();
        for case in 0..5 {
            let mut invalid = valid.clone();
            let alias = invalid.get_mut("b").unwrap();
            match case {
                0 => alias.hard_link = None, // Two independent 25 MiB files exceed the limit.
                1 => alias.hard_link = Some("missing".into()),
                2 => alias.hard_link = Some("b".into()),
                3 => alias.attributes.mode ^= 0o100,
                _ => alias.contents = Contents::File(Rc::from(b"different bytes".as_slice())),
            }
            let destination = work.0.join(format!("bad-{case}"));
            assert!(stage(&invalid, &destination).is_err());
            assert!(!destination.exists());
        }
    }

    #[test]
    fn complete_save_preserves_empty_directories_links_modes_times_and_deletions() {
        let work = Work::new();
        fs::create_dir(work.0.join("removed-dir")).unwrap();
        fs::write(work.0.join("removed-dir/file"), b"deleted").unwrap();
        fs::write(work.0.join("source"), b"rename bytes").unwrap();
        fs::write(work.0.join("unchanged"), b"original").unwrap();
        let baseline = host(&work.0).unwrap();
        let mut changed = baseline.clone();
        changed.remove("removed-dir");
        changed.remove("removed-dir/file");
        let original = changed.remove("source").unwrap();
        changed.insert("renamed".into(), original);
        changed.insert("empty".into(), entry(Contents::Directory));
        changed.insert("empty/nested".into(), entry(Contents::Directory));
        changed.insert("new".into(), file(b"binary\0bytes\xff"));
        changed.insert("link".into(), entry(Contents::Link("renamed".into())));
        fs::write(work.0.join("unchanged"), b"host edit").unwrap();
        save(&work.0, &baseline, &changed).unwrap();
        let saved = host(&work.0).unwrap();
        assert!(!saved.contains_key("source"));
        assert!(!saved.contains_key("removed-dir"));
        assert!(!work.0.join("removed-dir").exists());
        assert_eq!(saved["new"], changed["new"]);
        assert_eq!(saved["empty/nested"], changed["empty/nested"]);
        assert_eq!(saved["link"], changed["link"]);
        assert_eq!(saved["renamed"], changed["renamed"]);
        assert_eq!(
            saved["unchanged"].contents,
            Contents::File(Rc::from(b"host edit".as_slice()))
        );
        assert_eq!(fs::read(work.0.join("link")).unwrap(), b"renamed");
        assert!(!saved.contains_key(HOST_METADATA));
        // A second save with no guest changes preserves the same logical tree.
        save(&work.0, &saved, &saved).unwrap();
        assert_eq!(host(&work.0).unwrap(), saved);
    }

    #[test]
    fn conflicts_are_checked_before_effects_including_deleted_parents_and_new_host_children() {
        for host_added_child in [false, true] {
            let work = Work::new();
            fs::create_dir(work.0.join("dir")).unwrap();
            fs::write(work.0.join("dir/file"), b"original").unwrap();
            let baseline = host(&work.0).unwrap();
            let mut changed = Snapshot::new();
            changed.insert("first-new-file".into(), file(b"must not appear"));
            if host_added_child {
                fs::write(work.0.join("dir/host-new"), b"host bytes").unwrap();
            } else {
                fs::write(work.0.join("dir/file"), b"host edit").unwrap();
            }
            let before = host(&work.0).unwrap();
            assert!(save(&work.0, &baseline, &changed).is_err());
            let after = host(&work.0).unwrap();
            assert_eq!(
                before.keys().collect::<Vec<_>>(),
                after.keys().collect::<Vec<_>>()
            );
            for (path, entry) in before {
                assert!(entry.host_matches(&after[&path]), "{path}");
            }
            assert!(!work.0.join("first-new-file").exists());
            assert!(!work.0.join(HOST_METADATA).exists());
        }
    }

    #[test]
    fn replacing_files_and_directories_uses_dependency_order_and_never_removes_unknown_data() {
        let work = Work::new();
        fs::create_dir(work.0.join("dir")).unwrap();
        fs::write(work.0.join("dir/child"), b"child").unwrap();
        fs::write(work.0.join("file"), b"file").unwrap();
        let baseline = host(&work.0).unwrap();
        let changed = Snapshot::from([
            ("dir".into(), file(b"directory replaced")),
            ("file".into(), entry(Contents::Directory)),
            ("file/child".into(), file(b"file replaced")),
        ]);
        save(&work.0, &baseline, &changed).unwrap();
        assert_eq!(host(&work.0).unwrap(), changed);
        assert_eq!(fs::read(work.0.join("dir")).unwrap(), b"directory replaced");
        assert_eq!(
            fs::read(work.0.join("file/child")).unwrap(),
            b"file replaced"
        );
    }

    #[test]
    fn logical_guest_export_follows_metadata_without_exporting_backing_paths() {
        let work = Work::new();
        fs::write(work.0.join("source"), b"original bytes").unwrap();
        let namespace = Namespace::open(Rc::new(NativeFileSystem(work.0.clone())), None).unwrap();
        namespace.rename("source", "moved").unwrap();
        namespace.create_directory("empty").unwrap();
        namespace.symlink("moved", "link").unwrap();
        namespace.set_mode("moved", 0o640).unwrap();
        namespace
            .set_times("moved", true, TimeUpdate::Set(193), TimeUpdate::Set(853))
            .unwrap();
        let options = OpenOptions {
            read: true,
            write: true,
            create: true,
            exclusive: false,
            truncate: false,
            append: false,
        };
        let mut detached = namespace.open("deleted", options).unwrap();
        detached.write(b"never exported").unwrap();
        namespace.unlink("deleted").unwrap();
        let tree = guest(&work.0, &Snapshot::new()).unwrap();
        assert_eq!(
            tree.keys().map(String::as_str).collect::<Vec<_>>(),
            ["empty", "link", "moved"]
        );
        assert_eq!(
            tree["moved"].contents,
            Contents::File(Rc::from(b"original bytes".as_slice()))
        );
        assert_eq!(tree["moved"].attributes.mode, 0o640);
        assert_eq!(tree["moved"].attributes.modify, 853);
        assert_eq!(tree["link"].contents, Contents::Link("moved".into()));
        let destination = Work::new();
        save(&destination.0, &Snapshot::new(), &tree).unwrap();
        assert_eq!(host(&destination.0).unwrap(), tree);
    }

    #[test]
    fn malformed_host_metadata_and_unsafe_logical_paths_fail_without_changes() {
        let work = Work::new();
        fs::write(work.0.join("file"), b"preserved").unwrap();
        for path in [
            "../escape",
            "a/../escape",
            "a//b",
            "CON",
            "name:stream",
            ".EFI-AGENT-FS/file",
            HOST_METADATA,
        ] {
            let bad = Snapshot::from([(path.into(), file(b"bad"))]);
            assert!(
                merge(&Snapshot::new(), &Snapshot::new(), &bad).is_err(),
                "{path}"
            );
        }
        fs::write(work.0.join(HOST_METADATA), b"user collision").unwrap();
        assert!(host(&work.0).is_err());
        assert_eq!(fs::read(work.0.join("file")).unwrap(), b"preserved");
        assert_eq!(
            fs::read(work.0.join(HOST_METADATA)).unwrap(),
            b"user collision"
        );
    }

    #[test]
    fn changed_link_placeholder_becomes_an_ordinary_host_file() {
        let work = Work::new();
        let tree = Snapshot::from([(
            "link".into(),
            entry(Contents::Link("original-target".into())),
        )]);
        save(&work.0, &Snapshot::new(), &tree).unwrap();
        fs::write(work.0.join("link"), b"a host replacement with another size").unwrap();
        assert!(matches!(
            host(&work.0).unwrap()["link"].contents,
            Contents::File(_)
        ));
        assert!(save(&work.0, &tree, &Snapshot::new()).is_err());
    }

    #[test]
    fn same_size_and_mtime_replacement_invalidates_cached_link_and_mode() {
        let work = Work::new();
        let tree = Snapshot::from([("link".into(), entry(Contents::Link("original".into())))]);
        save(&work.0, &Snapshot::new(), &tree).unwrap();
        let path = work.0.join("link");
        let before = fs::metadata(&path).unwrap();
        fs::write(&path, b"replaced").unwrap();
        let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.set_times(fs::FileTimes::new().set_modified(before.modified().unwrap()))
            .unwrap();
        drop(file);
        let current = host(&work.0).unwrap();
        assert_eq!(
            current["link"].contents,
            Contents::File(Rc::from(b"replaced".as_slice()))
        );
        assert_ne!(
            current["link"].attributes.modify,
            tree["link"].attributes.modify
        );
        assert!(save(&work.0, &tree, &Snapshot::new()).is_err());
        assert_eq!(fs::read(path).unwrap(), b"replaced");
    }
}
