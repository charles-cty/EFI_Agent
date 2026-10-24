use crate::{EntryType, FileError, MAX_DIRECTORY_ENTRIES};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::{String, ToString},
    vec::Vec,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<DirectoryLink>,
    /// Imported physical aliases are removed only after their logical entries commit.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub import_aliases: Vec<String>,
}

/// Additional directory names reference one inode and its one backing object.
/// Directories retain a unique parent; directory hard links are prohibited.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectoryLink {
    pub inode: u64,
    pub parent: u64,
    pub name: String,
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
            links: Vec::new(),
            import_aliases: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<(), FileError> {
        let root = self.nodes.get(&ROOT).ok_or(FileError::Corrupt)?;
        if self.nodes.len() + self.links.len() > MAX_NODES
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
                if !names.insert((parent, node.name.as_str())) {
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
        for link in &self.links {
            let inode = self.nodes.get(&link.inode).ok_or(FileError::Corrupt)?;
            if inode.kind == EntryType::Directory || inode.parent.is_none() {
                return Err(FileError::Corrupt);
            }
            valid_name(&link.name).map_err(|_| FileError::Corrupt)?;
            let parent = self.nodes.get(&link.parent).ok_or(FileError::Corrupt)?;
            if parent.kind != EntryType::Directory
                || (link.parent != ROOT && parent.parent.is_none())
                || (link.parent == ROOT
                    && (link.name.eq_ignore_ascii_case(PRIVATE) || link.name == ".efi-process"))
                || !names.insert((link.parent, link.name.as_str()))
            {
                return Err(FileError::Corrupt);
            }
            self.path(link.parent).map_err(|_| FileError::Corrupt)?;
        }
        let mut aliases = BTreeSet::new();
        if self.import_aliases.len() > MAX_NODES {
            return Err(FileError::Corrupt);
        }
        for path in &self.import_aliases {
            if !valid_backing(path) || backings.contains(path) || !aliases.insert(path) {
                return Err(FileError::Corrupt);
            }
        }
        Ok(())
    }

    pub fn child(&self, directory: u64, name: &str) -> Option<u64> {
        self.entries()
            .find_map(|(id, parent, child)| (parent == directory && child == name).then_some(id))
    }

    pub fn entries(&self) -> impl Iterator<Item = (u64, u64, &str)> {
        self.nodes
            .iter()
            .filter_map(|(&id, node)| node.parent.map(|parent| (id, parent, node.name.as_str())))
            .chain(
                self.links
                    .iter()
                    .map(|link| (link.inode, link.parent, link.name.as_str())),
            )
    }

    pub fn entry_path(&self, parent: u64, name: &str) -> Result<String, FileError> {
        let parent = self.path(parent)?;
        Ok(if parent.is_empty() {
            name.into()
        } else {
            alloc::format!("{parent}/{name}")
        })
    }

    pub fn detach_entry(&mut self, parent: u64, name: &str) -> Result<u64, FileError> {
        let id = self.child(parent, name).ok_or(FileError::NotFound)?;
        let node = &self.nodes[&id];
        if node.parent == Some(parent) && node.name == name {
            let replacement = self
                .links
                .iter()
                .position(|link| link.inode == id)
                .map(|index| self.links.swap_remove(index));
            let node = self.nodes.get_mut(&id).unwrap();
            if let Some(link) = replacement {
                node.parent = Some(link.parent);
                node.name = link.name;
            } else {
                node.parent = None;
                node.name.clear();
            }
        } else {
            let index = self
                .links
                .iter()
                .position(|link| link.parent == parent && link.name == name)
                .unwrap();
            self.links.swap_remove(index);
        }
        Ok(id)
    }

    pub fn move_entry(
        &mut self,
        parent: u64,
        name: &str,
        new_parent: u64,
        new_name: String,
    ) -> Result<(), FileError> {
        let id = self.child(parent, name).ok_or(FileError::NotFound)?;
        let node = self.nodes.get_mut(&id).unwrap();
        if node.parent == Some(parent) && node.name == name {
            node.parent = Some(new_parent);
            node.name = new_name;
        } else {
            let link = self
                .links
                .iter_mut()
                .find(|link| link.parent == parent && link.name == name)
                .unwrap();
            link.parent = new_parent;
            link.name = new_name;
        }
        Ok(())
    }

    pub fn add_link(&mut self, inode: u64, parent: u64, name: String) -> Result<(), FileError> {
        if self.nodes.len() + self.links.len() >= MAX_NODES {
            return Err(FileError::ResourceLimit);
        }
        if self.child(parent, &name).is_some() {
            return Err(FileError::Exists);
        }
        self.links.push(DirectoryLink {
            inode,
            parent,
            name,
        });
        Ok(())
    }

    pub fn allocate(&mut self, node: Node) -> Result<u64, FileError> {
        if self.nodes.len() + self.links.len() >= MAX_NODES {
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
