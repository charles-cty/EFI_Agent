use crate::{EntryType, FileError, MAX_DIRECTORY_ENTRIES};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::{String, ToString},
};
use serde::{Deserialize, Serialize};

pub const ROOT: u64 = 1;
pub const MAX_NODES: usize = MAX_DIRECTORY_ENTRIES * 2 + 1;
pub const MAX_PATH_BYTES: usize = 4096;
pub const MAX_NAME_BYTES: usize = 1024;
pub const PRIVATE: &str = ".efi-agent-fs";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub parent: Option<u64>,
    pub name: String,
    pub kind: EntryType,
    pub backing: Option<String>,
    pub target: Option<String>,
    pub mode: u16,
    pub accessed: u64,
    pub modified: u64,
    pub created: u64,
}

impl Node {
    pub fn new(parent: Option<u64>, name: String, kind: EntryType, now: u64) -> Self {
        Self {
            parent,
            name,
            kind,
            backing: None,
            target: None,
            mode: if kind == EntryType::Directory {
                0o755
            } else {
                0o644
            },
            accessed: now,
            modified: now,
            created: now,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub next_inode: u64,
    pub nodes: BTreeMap<u64, Node>,
}

pub fn valid_name(name: &str) -> Result<(), FileError> {
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\', '\0']) {
        return Err(FileError::Invalid);
    }
    if name.len() > MAX_NAME_BYTES {
        return Err(FileError::ResourceLimit);
    }
    Ok(())
}

pub fn valid_backing(path: &str) -> bool {
    if path.is_empty() || path.len() > MAX_PATH_BYTES || path.starts_with('/') {
        return false;
    }
    if path.split('/').any(|part| valid_name(part).is_err()) {
        return false;
    }
    if let Some(suffix) = path.strip_prefix(".efi-agent-fs/") {
        return suffix
            .strip_prefix("objects/")
            .and_then(|name| name.parse::<u64>().ok())
            .is_some();
    }
    !path
        .split('/')
        .next()
        .is_some_and(|part| part.eq_ignore_ascii_case(PRIVATE))
}

impl State {
    pub fn new(now: u64) -> Self {
        Self {
            next_inode: ROOT + 1,
            nodes: BTreeMap::from([(
                ROOT,
                Node::new(None, String::new(), EntryType::Directory, now),
            )]),
        }
    }

    pub fn validate(&self) -> Result<(), FileError> {
        let root = self.nodes.get(&ROOT).ok_or(FileError::Corrupt)?;
        if self.nodes.len() > MAX_NODES
            || self
                .nodes
                .keys()
                .next_back()
                .is_none_or(|id| *id >= self.next_inode)
            || root.parent.is_some()
            || !root.name.is_empty()
            || root.kind != EntryType::Directory
        {
            return Err(FileError::Corrupt);
        }
        let mut names = BTreeSet::new();
        let mut backings = BTreeSet::new();
        for (&id, node) in &self.nodes {
            if id == 0 || node.mode & !0o7777 != 0 {
                return Err(FileError::Corrupt);
            }
            match node.kind {
                EntryType::File
                    if node.target.is_none()
                        && node.backing.as_deref().is_some_and(valid_backing) =>
                {
                    if !backings.insert(node.backing.as_ref().unwrap()) {
                        return Err(FileError::Corrupt);
                    }
                }
                EntryType::Directory if node.target.is_none() && node.backing.is_none() => (),
                EntryType::Symlink
                    if node.backing.is_none()
                        && node.target.as_ref().is_some_and(|target| {
                            !target.is_empty()
                                && target.len() <= MAX_PATH_BYTES
                                && !target.contains(['\\', '\0'])
                        }) => {}
                _ => return Err(FileError::Corrupt),
            }
            if let Some(parent) = node.parent {
                valid_name(&node.name).map_err(|_| FileError::Corrupt)?;
                if parent == ROOT
                    && (node.name.eq_ignore_ascii_case(PRIVATE) || node.name == ".efi-process")
                {
                    return Err(FileError::Corrupt);
                }
                if !names.insert((parent, &node.name)) {
                    return Err(FileError::Corrupt);
                }
                let mut ancestor = parent;
                let mut depth = 0;
                loop {
                    let parent = self.nodes.get(&ancestor).ok_or(FileError::Corrupt)?;
                    if parent.kind != EntryType::Directory || ancestor == id || depth > 32 {
                        return Err(FileError::Corrupt);
                    }
                    if ancestor == ROOT {
                        break;
                    }
                    ancestor = parent.parent.ok_or(FileError::Corrupt)?;
                    depth += 1;
                }
            } else if !node.name.is_empty() {
                return Err(FileError::Corrupt);
            }
        }
        Ok(())
    }

    pub fn child(&self, directory: u64, name: &str) -> Option<u64> {
        self.nodes.iter().find_map(|(&id, node)| {
            (node.parent == Some(directory) && node.name == name).then_some(id)
        })
    }

    pub fn allocate(&mut self, node: Node) -> Result<u64, FileError> {
        if self.nodes.len() >= MAX_NODES {
            return Err(FileError::ResourceLimit);
        }
        let id = self.next_inode;
        self.next_inode = id.checked_add(1).ok_or(FileError::Overflow)?;
        self.nodes.insert(id, node);
        Ok(id)
    }

    pub fn path(&self, id: u64) -> Result<String, FileError> {
        let mut parts = alloc::vec::Vec::new();
        let mut current = id;
        while current != ROOT {
            let node = self.nodes.get(&current).ok_or(FileError::NotFound)?;
            current = node.parent.ok_or(FileError::NotFound)?;
            parts.push(node.name.as_str());
        }
        parts.reverse();
        Ok(parts.join("/").to_string())
    }
}
