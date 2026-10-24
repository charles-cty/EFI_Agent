//! Persistent inode namespace above filesystems such as EFI FAT.
//!
//! Path mutations commit metadata rather than moving open backing files. Two
//! checksummed generations bound recovery after an interrupted metadata write.
mod journal;
mod state;
mod temporary;

use crate::{
    Capacity, DirectoryEntries, EntryType, File, FileError, FileSystem, MAX_RESIZE_BYTES, Metadata,
    OpenOptions, TimeUpdate,
};
use alloc::{
    boxed::Box,
    collections::{BTreeMap, VecDeque},
    rc::Rc,
    string::String,
    vec::Vec,
};
use core::cell::{Cell, RefCell};
use state::{MAX_PATH_BYTES, Node, PRIVATE, ROOT, State, valid_name};

pub type Clock = Rc<dyn Fn() -> Result<u64, FileError>>;

enum Storage {
    Journal,
    Temporary { slots: usize, device: u32 },
}

struct Inner {
    raw: Rc<dyn FileSystem>,
    state: RefCell<State>,
    generation: Cell<u64>,
    dirty: Cell<bool>,
    uncertain: Cell<bool>,
    open: RefCell<BTreeMap<u64, usize>>,
    errors: RefCell<Vec<FileError>>,
    clock: RefCell<Option<Clock>>,
    storage: Storage,
}

struct Anchor {
    inner: Rc<Inner>,
    inode: u64,
}

/// Directory views share one inode graph and journal. A view retains its
/// directory inode when a different descriptor renames or removes that name.
#[derive(Clone)]
pub struct Namespace {
    anchor: Rc<Anchor>,
}

enum Resolution {
    Found(u64),
    Missing {
        parent: u64,
        name: String,
        directory: bool,
    },
}

/// Host-only workspace export data. Guest lookup cannot access backing paths.
pub struct SnapshotEntry {
    pub path: String,
    pub metadata: Metadata,
    pub backing: Option<String>,
    pub target: Option<String>,
}

/// Read the committed logical tree without recovery writes or inode reclamation.
/// Exporters must use backing paths only to read regular-file contents.
pub fn read_snapshot(raw: &dyn FileSystem) -> Result<Option<Vec<SnapshotEntry>>, FileError> {
    let Some((_, state)) = journal::load(raw)? else {
        return Ok(None);
    };
    core::iter::once((ROOT, ROOT, ""))
        .chain(state.entries())
        .map(|(id, parent, name)| {
            let node = &state.nodes[&id];
            let (size, allocated) = match node.kind {
                EntryType::File => {
                    let info = raw.metadata(node.backing.as_ref().unwrap())?;
                    (info.size, info.allocated)
                }
                EntryType::Symlink => (node.target.as_ref().unwrap().len() as u64, Some(0)),
                _ => (0, Some(0)),
            };
            Ok(SnapshotEntry {
                path: if id == ROOT {
                    String::new()
                } else {
                    state.entry_path(parent, name)?
                },
                metadata: node_metadata(&state, id, size, allocated),
                backing: node.backing.clone(),
                target: node.target.clone(),
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

fn permission(node: &Node, requested: u16) -> Result<(), FileError> {
    if node.mode & requested == requested {
        Ok(())
    } else {
        Err(FileError::Access)
    }
}

fn components(path: &str) -> Result<VecDeque<String>, FileError> {
    if path.len() > MAX_PATH_BYTES {
        return Err(FileError::ResourceLimit);
    }
    if path.contains(['\\', '\0']) {
        return Err(FileError::Invalid);
    }
    Ok(path
        .split('/')
        .filter(|part| !part.is_empty())
        .map(String::from)
        .collect())
}

impl Inner {
    fn slots(&self) -> usize {
        match self.storage {
            Storage::Journal => state::MAX_NODES,
            Storage::Temporary { slots, .. } => slots,
        }
    }

    fn reserve_entry(&self, candidate: &State) -> Result<(), FileError> {
        if candidate.nodes.len() + candidate.links.len() >= self.slots() {
            return Err(match self.storage {
                Storage::Journal => FileError::ResourceLimit,
                Storage::Temporary { .. } => FileError::NoSpace,
            });
        }
        Ok(())
    }

    fn device(&self) -> u32 {
        match self.storage {
            Storage::Journal => 0,
            Storage::Temporary { device, .. } => device,
        }
    }

    fn stamp(&self) -> Result<Option<u64>, FileError> {
        self.clock
            .borrow()
            .as_ref()
            .map(|clock| clock())
            .transpose()
    }

    fn filesystem_statistics(&self) -> Result<crate::FileSystemStatistics, FileError> {
        self.check()?;
        let state = self.state.borrow();
        Ok(crate::FileSystemStatistics {
            capacity: self.raw.capacity()?,
            files: self.slots() as u64,
            files_free: (self.slots() - state.nodes.len() - state.links.len()) as u64,
            id: self.device(),
            kind: match self.storage {
                Storage::Journal => 0x45464941,
                Storage::Temporary { .. } => 0x01021994,
            },
            name_max: state::MAX_NAME_BYTES as u32,
            read_only: false,
        })
    }
    fn check(&self) -> Result<(), FileError> {
        if self.uncertain.get() {
            Err(FileError::Io)
        } else {
            Ok(())
        }
    }

    fn now(&self) -> Result<u64, FileError> {
        self.stamp()?.ok_or(FileError::Unsupported)
    }

    fn commit(&self, candidate: State) -> Result<(), FileError> {
        self.check()?;
        if matches!(self.storage, Storage::Temporary { .. }) {
            *self.state.borrow_mut() = candidate;
            self.dirty.set(false);
            return Ok(());
        }
        let generation = self
            .generation
            .get()
            .checked_add(1)
            .ok_or(FileError::Overflow)?;
        let bytes = journal::encode(generation, &candidate)?;
        if let Err(error) = journal::commit(self.raw.as_ref(), generation, &bytes) {
            // A failed flush can leave a complete newer generation on disk.
            // Further mutations must wait for recovery to select the winner.
            self.uncertain.set(true);
            return Err(error);
        }
        *self.state.borrow_mut() = candidate;
        self.generation.set(generation);
        self.dirty.set(false);
        Ok(())
    }

    fn flush(&self) -> Result<(), FileError> {
        self.check()?;
        if self.dirty.get() {
            let candidate = self.state.borrow().clone();
            self.commit(candidate)?;
        }
        Ok(())
    }

    fn retain(&self, inode: u64) {
        *self.open.borrow_mut().entry(inode).or_default() += 1;
    }

    fn release(&self, inode: u64) {
        let mut open = self.open.borrow_mut();
        let count = open.get_mut(&inode).expect("Retained inode exists");
        *count -= 1;
        if *count == 0 {
            open.remove(&inode);
        }
    }

    fn record(&self, error: FileError) {
        let mut errors = self.errors.borrow_mut();
        if errors.len() < 16 {
            errors.push(error);
        }
    }

    fn collect(&self) -> Result<(), FileError> {
        self.check()?;
        let unused: Vec<_> = self
            .state
            .borrow()
            .nodes
            .iter()
            .filter_map(|(&id, node)| {
                (id != ROOT && node.parent.is_none() && !self.open.borrow().contains_key(&id))
                    .then_some(id)
            })
            .collect();
        if unused.is_empty() && self.state.borrow().import_aliases.is_empty() {
            return Ok(());
        }
        let mut candidate = self.state.borrow().clone();
        for path in &candidate.import_aliases {
            match self.raw.unlink(path) {
                Ok(()) | Err(FileError::NotFound) => (),
                Err(error) => return Err(error),
            }
        }
        candidate.import_aliases.clear();
        for &id in &unused {
            if let Some(path) = &candidate.nodes[&id].backing {
                match self.raw.unlink(path) {
                    Ok(()) | Err(FileError::NotFound) => (),
                    Err(error) => return Err(error),
                }
            }
            candidate.nodes.remove(&id);
        }
        self.commit(candidate)?;
        Ok(())
    }

    fn metadata(&self, inode: u64) -> Result<Metadata, FileError> {
        self.check()?;
        let node = self
            .state
            .borrow()
            .nodes
            .get(&inode)
            .ok_or(FileError::NotFound)?
            .clone();
        let (size, allocated) = match node.kind {
            EntryType::File => {
                let info = self.raw.metadata(node.backing.as_ref().unwrap())?;
                (info.size, info.allocated)
            }
            EntryType::Symlink => (node.target.as_ref().unwrap().len() as u64, Some(0)),
            _ => (0, Some(0)),
        };
        let mut metadata = node_metadata(&self.state.borrow(), inode, size, allocated);
        metadata.device = u64::from(self.device());
        Ok(metadata)
    }
}

fn node_metadata(state: &State, inode: u64, size: u64, allocated: Option<u64>) -> Metadata {
    let node = &state.nodes[&inode];
    let links = if inode != ROOT && node.parent.is_none() {
        0
    } else if node.kind == EntryType::Directory {
        2 + state
            .nodes
            .values()
            .filter(|child| child.parent == Some(inode) && child.kind == EntryType::Directory)
            .count() as u64
    } else {
        1 + state
            .links
            .iter()
            .filter(|link| link.inode == inode)
            .count() as u64
    };
    Metadata {
        device: 0,
        inode,
        kind: node.kind,
        size,
        allocated,
        mode: node.mode,
        links,
        accessed: node.accessed,
        modified: node.modified,
        created: node.created,
    }
}

impl Namespace {
    pub fn open(raw: Rc<dyn FileSystem>, clock: Option<Clock>) -> Result<Self, FileError> {
        let loaded = journal::load(raw.as_ref())?;
        let (generation, state) = if let Some(loaded) = loaded {
            loaded
        } else {
            // load has already checked the format marker for an existing area.
            let resume = raw.metadata(PRIVATE).is_ok();
            let now = clock
                .as_ref()
                .map(|clock| clock())
                .transpose()?
                .unwrap_or(0);
            let mut state = State::new(now);
            import(
                raw.as_ref(),
                &mut state,
                ROOT,
                "",
                0,
                resume,
                &mut BTreeMap::new(),
            )?;
            let bytes = journal::encode(1, &state)?;
            journal::initialize(raw.as_ref(), resume)?;
            journal::commit(raw.as_ref(), 1, &bytes)?;
            (1, state)
        };
        let inner = Rc::new(Inner {
            raw,
            state: RefCell::new(state),
            generation: Cell::new(generation),
            dirty: Cell::new(false),
            uncertain: Cell::new(false),
            open: RefCell::new(BTreeMap::new()),
            errors: RefCell::new(Vec::new()),
            clock: RefCell::new(clock),
            storage: Storage::Journal,
        });
        let namespace = Self::view(inner, ROOT);
        namespace.collect_garbage()?;
        namespace.remove_uncommitted_objects()?;
        Ok(namespace)
    }

    /// Create an ephemeral inode tree with bounded data and directory slots.
    /// Slots include the root, additional hard-link names and detached inodes
    /// retained by open descriptors. No journal or host files are created.
    pub fn temporary(bytes: u64, slots: usize, device: u32, clock: Option<Clock>) -> Self {
        assert!((1..=state::MAX_NODES).contains(&slots));
        Self::view(
            Rc::new(Inner {
                raw: Rc::new(temporary::MemoryStore::new(bytes)),
                state: RefCell::new(State::new(0)),
                generation: Cell::new(0),
                dirty: Cell::new(false),
                uncertain: Cell::new(false),
                open: RefCell::new(BTreeMap::new()),
                errors: RefCell::new(Vec::new()),
                clock: RefCell::new(clock),
                storage: Storage::Temporary { slots, device },
            }),
            ROOT,
        )
    }

    /// Replace the host time provider before guest execution starts.
    pub fn set_clock(&self, clock: Option<Clock>) {
        *self.inner().clock.borrow_mut() = clock;
    }

    fn view(inner: Rc<Inner>, inode: u64) -> Self {
        inner.retain(inode);
        Self {
            anchor: Rc::new(Anchor { inner, inode }),
        }
    }

    fn inner(&self) -> &Rc<Inner> {
        &self.anchor.inner
    }

    pub fn flush(&self) -> Result<(), FileError> {
        self.inner().flush()
    }
    pub fn collect_garbage(&self) -> Result<(), FileError> {
        self.inner().collect()
    }
    pub fn take_errors(&self) -> Vec<FileError> {
        core::mem::take(&mut *self.inner().errors.borrow_mut())
    }

    /// Restore trusted packaging metadata in one committed generation.
    /// Resolve every path without following its final symlink before changing
    /// permissions. Identity, contents and link targets stay in this namespace.
    pub fn restore_metadata(&self, entries: &[(String, Metadata)]) -> Result<(), FileError> {
        let mut candidate = self.inner().state.borrow().clone();
        for (path, metadata) in entries {
            if metadata.mode & !0o7777 != 0 {
                return Err(FileError::Invalid);
            }
            let id = self.walk(path, false)?;
            let node = candidate.nodes.get_mut(&id).ok_or(FileError::NotFound)?;
            if node.kind != metadata.kind {
                return Err(FileError::Invalid);
            }
            node.mode = metadata.mode;
            node.accessed = metadata.accessed;
            node.modified = metadata.modified;
            node.created = metadata.created;
        }
        self.inner().commit(candidate)
    }

    pub fn snapshot(&self) -> Result<Vec<SnapshotEntry>, FileError> {
        self.inner().check()?;
        let state = self.inner().state.borrow();
        core::iter::once((ROOT, ROOT, ""))
            .chain(state.entries())
            .map(|(id, parent, name)| {
                let node = &state.nodes[&id];
                Ok(SnapshotEntry {
                    path: if id == ROOT {
                        String::new()
                    } else {
                        state.entry_path(parent, name)?
                    },
                    metadata: self.inner().metadata(id)?,
                    backing: node.backing.clone(),
                    target: node.target.clone(),
                })
            })
            .collect()
    }

    fn remove_uncommitted_objects(&self) -> Result<(), FileError> {
        for (name, kind) in self
            .inner()
            .raw
            .read_directory(".efi-agent-fs/objects")?
            .iter()
        {
            if name == "." || name == ".." {
                continue;
            }
            if kind != EntryType::File || name.parse::<u64>().is_err() {
                return Err(FileError::Corrupt);
            }
            let path = alloc::format!(".efi-agent-fs/objects/{name}");
            if !self
                .inner()
                .state
                .borrow()
                .nodes
                .values()
                .any(|node| node.backing.as_ref() == Some(&path))
            {
                self.inner().raw.unlink(&path)?;
            }
        }
        Ok(())
    }

    fn walk(&self, path: &str, follow: bool) -> Result<u64, FileError> {
        match self.lookup(path, follow, false)? {
            Resolution::Found(id) => Ok(id),
            Resolution::Missing { .. } => unreachable!("Missing entries were rejected"),
        }
    }

    fn lookup(
        &self,
        path: &str,
        follow: bool,
        allow_missing: bool,
    ) -> Result<Resolution, FileError> {
        self.inner().check()?;
        if path.starts_with('/') {
            return Err(FileError::NotCapable);
        }
        let mut parts = components(path)?;
        let state = self.inner().state.borrow();
        let mut stack = alloc::vec![self.anchor.inode];
        let mut links = 0;
        let mut require_directory = path.ends_with('/');
        while let Some(part) = parts.pop_front() {
            let current = *stack.last().unwrap();
            let directory = state.nodes.get(&current).ok_or(FileError::NotFound)?;
            if directory.kind != EntryType::Directory {
                return Err(FileError::NotDirectory);
            }
            permission(directory, 0o100)?;
            if part == "." {
                continue;
            }
            if part == ".." {
                if stack.len() == 1 {
                    return Err(FileError::NotCapable);
                }
                stack.pop();
                continue;
            }
            valid_name(&part)?;
            let Some(child) = state.child(current, &part) else {
                if allow_missing && parts.is_empty() {
                    return Ok(Resolution::Missing {
                        parent: current,
                        name: part,
                        directory: require_directory,
                    });
                }
                return Err(FileError::NotFound);
            };
            let node = &state.nodes[&child];
            if node.kind == EntryType::Symlink && (follow || !parts.is_empty() || require_directory)
            {
                links += 1;
                if links > 40 {
                    return Err(FileError::Loop);
                }
                let target = node.target.as_ref().unwrap();
                if parts.is_empty() && target.ends_with('/') {
                    require_directory = true;
                }
                if target.starts_with('/') {
                    if self.anchor.inode != ROOT {
                        return Err(FileError::NotCapable);
                    }
                    stack.truncate(1);
                }
                let mut replacement = components(target)?;
                replacement.append(&mut parts);
                parts = replacement;
            } else {
                stack.push(child);
            }
        }
        let id = *stack.last().unwrap();
        if require_directory && state.nodes[&id].kind != EntryType::Directory {
            return Err(FileError::NotDirectory);
        }
        Ok(Resolution::Found(id))
    }

    fn parent(&self, path: &str) -> Result<(u64, String), FileError> {
        let path = path.trim_end_matches('/');
        if path.is_empty() {
            return Err(FileError::Access);
        }
        let (parent, name) = path.rsplit_once('/').unwrap_or(("", path));
        valid_name(name)?;
        let parent = self.walk(parent, true)?;
        self.check_parent(parent, name)?;
        Ok((parent, name.into()))
    }

    fn check_parent(&self, parent: u64, name: &str) -> Result<(), FileError> {
        let state = self.inner().state.borrow();
        let node = &state.nodes[&parent];
        if node.kind != EntryType::Directory {
            return Err(FileError::NotDirectory);
        }
        if parent != ROOT && node.parent.is_none() {
            return Err(FileError::NotFound);
        }
        permission(node, 0o300)?;
        if matches!(self.inner().storage, Storage::Journal)
            && parent == ROOT
            && (name.eq_ignore_ascii_case(PRIVATE) || name == ".efi-process")
        {
            return Err(FileError::Access);
        }
        Ok(())
    }

    fn stamp(&self) -> Result<u64, FileError> {
        self.inner().stamp().map(|now| now.unwrap_or(0))
    }

    fn detach(&self, path: &str, directory: bool) -> Result<(), FileError> {
        self.inner().check()?;
        if path.ends_with('/')
            && self.inner().state.borrow().nodes[&self.walk(path.trim_end_matches('/'), false)?]
                .kind
                != EntryType::Directory
        {
            return Err(FileError::NotDirectory);
        }
        let (parent, name) = self.parent(path)?;
        let mut candidate = self.inner().state.borrow().clone();
        let id = candidate.child(parent, &name).ok_or(FileError::NotFound)?;
        if directory != (candidate.nodes[&id].kind == EntryType::Directory) {
            return Err(if directory {
                FileError::NotDirectory
            } else {
                FileError::IsDirectory
            });
        }
        if directory && candidate.entries().any(|(_, parent, _)| parent == id) {
            return Err(FileError::NotEmpty);
        }
        candidate.detach_entry(parent, &name)?;
        candidate.nodes.get_mut(&parent).unwrap().modified = self.stamp()?;
        self.inner().commit(candidate)?;
        if let Err(error) = self.collect_garbage() {
            self.inner().record(error);
        }
        Ok(())
    }

    fn move_to(
        &self,
        source: &str,
        destination_view: &Self,
        destination: &str,
    ) -> Result<(), FileError> {
        if !Rc::ptr_eq(self.inner(), destination_view.inner()) {
            return Err(FileError::CrossDevice);
        }
        let (old_parent, old_name) = self.parent(source)?;
        let (new_parent, new_name) = destination_view.parent(destination)?;
        let mut candidate = self.inner().state.borrow().clone();
        let id = candidate
            .child(old_parent, &old_name)
            .ok_or(FileError::NotFound)?;
        let source_directory = candidate.nodes[&id].kind == EntryType::Directory;
        if (source.ends_with('/') || destination.ends_with('/')) && !source_directory {
            return Err(FileError::NotDirectory);
        }
        if old_parent == new_parent && old_name == new_name {
            return Ok(());
        }
        if source_directory {
            let mut ancestor = new_parent;
            loop {
                if ancestor == id {
                    return Err(FileError::Invalid);
                }
                let Some(parent) = candidate.nodes[&ancestor].parent else {
                    break;
                };
                ancestor = parent;
            }
        }
        if let Some(target) = candidate.child(new_parent, &new_name) {
            if target == id {
                return Ok(());
            }
            let target_directory = candidate.nodes[&target].kind == EntryType::Directory;
            if destination.ends_with('/') && !target_directory {
                return Err(FileError::NotDirectory);
            }
            if source_directory != target_directory {
                return Err(if source_directory {
                    FileError::NotDirectory
                } else {
                    FileError::IsDirectory
                });
            }
            if target_directory && candidate.entries().any(|(_, parent, _)| parent == target) {
                return Err(FileError::NotEmpty);
            }
            candidate.detach_entry(new_parent, &new_name)?;
        }
        candidate.move_entry(old_parent, &old_name, new_parent, new_name)?;
        let now = self.stamp()?;
        candidate.nodes.get_mut(&old_parent).unwrap().modified = now;
        candidate.nodes.get_mut(&new_parent).unwrap().modified = now;
        self.inner().commit(candidate)?;
        if let Err(error) = self.collect_garbage() {
            self.inner().record(error);
        }
        Ok(())
    }

    fn link_to(
        &self,
        source: &str,
        destination_view: &Self,
        destination: &str,
        follow: bool,
    ) -> Result<(), FileError> {
        if !Rc::ptr_eq(self.inner(), destination_view.inner()) {
            return Err(FileError::CrossDevice);
        }
        let inode = self.walk(source, follow)?;
        if self.inner().state.borrow().nodes[&inode].kind == EntryType::Directory {
            return Err(FileError::NotPermitted);
        }
        if destination.ends_with('/') {
            return Err(FileError::NotDirectory);
        }
        let (parent, name) = destination_view.parent(destination)?;
        let mut candidate = self.inner().state.borrow().clone();
        self.inner().reserve_entry(&candidate)?;
        candidate.add_link(inode, parent, name)?;
        candidate.nodes.get_mut(&parent).unwrap().modified = self.stamp()?;
        self.inner().commit(candidate)
    }
}

fn import(
    raw: &dyn FileSystem,
    state: &mut State,
    parent: u64,
    path: &str,
    depth: usize,
    resume: bool,
    identities: &mut BTreeMap<(u64, u64), u64>,
) -> Result<(), FileError> {
    if depth > 32 {
        return Err(FileError::ResourceLimit);
    }
    for (name, kind) in raw.read_directory(path)?.iter() {
        if name == "." || name == ".." {
            continue;
        }
        if parent == ROOT && name == PRIVATE && resume {
            continue;
        }
        if parent == ROOT && (name.eq_ignore_ascii_case(PRIVATE) || name == ".efi-process") {
            return Err(FileError::Exists);
        }
        valid_name(name)?;
        let path = if path.is_empty() {
            name.into()
        } else {
            alloc::format!("{path}/{name}")
        };
        if path.len() > MAX_PATH_BYTES {
            return Err(FileError::ResourceLimit);
        }
        if !matches!(kind, EntryType::File | EntryType::Directory) {
            return Err(FileError::Unsupported);
        }
        let info = raw.metadata(&path)?;
        if kind == EntryType::File
            && info.inode != 0
            && let Some(&inode) = identities.get(&(info.device, info.inode))
        {
            state.add_link(inode, parent, name.into())?;
            state.import_aliases.push(path);
            continue;
        }
        let mut node = Node::new(Some(parent), name.into(), kind, info.modified);
        node.mode = info.mode;
        node.accessed = info.accessed;
        node.created = info.created;
        if kind == EntryType::File {
            node.backing = Some(path.clone());
        }
        let id = state.allocate(node)?;
        if kind == EntryType::File && info.inode != 0 {
            identities.insert((info.device, info.inode), id);
        }
        if kind == EntryType::Directory {
            import(raw, state, id, &path, depth + 1, resume, identities)?;
        }
    }
    Ok(())
}

impl FileSystem for Namespace {
    fn open(&self, path: &str, options: OpenOptions) -> Result<Box<dyn File>, FileError> {
        self.inner().check()?;
        if (options.truncate || options.append) && !options.write
            || options.exclusive && !options.create
        {
            return Err(FileError::Invalid);
        }
        if options.create && path.ends_with('/') {
            return Err(FileError::IsDirectory);
        }
        if options.create && options.exclusive {
            match self.walk(path, false) {
                Ok(_) => return Err(FileError::Exists),
                Err(FileError::NotFound) => (),
                Err(error) => return Err(error),
            }
        }
        let id = match self.lookup(path, true, options.create)? {
            Resolution::Found(id) => id,
            Resolution::Missing {
                parent,
                name,
                directory,
            } => {
                if directory {
                    return Err(FileError::IsDirectory);
                }
                self.check_parent(parent, &name)?;
                let mut candidate = self.inner().state.borrow().clone();
                if candidate.child(parent, &name).is_some() {
                    return Err(FileError::Exists);
                }
                let now = self.stamp()?;
                self.inner().reserve_entry(&candidate)?;
                let id = candidate.allocate(Node::new(Some(parent), name, EntryType::File, now))?;
                let backing = alloc::format!(".efi-agent-fs/objects/{id}");
                let mut raw_options = options;
                raw_options.exclusive = true;
                raw_options.append = false;
                let mut file = self.inner().raw.open(&backing, raw_options)?;
                if let Err(error) = file.flush() {
                    self.inner().uncertain.set(true);
                    return Err(error);
                }
                candidate.nodes.get_mut(&id).unwrap().backing = Some(backing);
                candidate.nodes.get_mut(&parent).unwrap().modified = now;
                self.inner().commit(candidate)?;
                self.inner().retain(id);
                return Ok(Box::new(OpenFile {
                    inner: self.inner().clone(),
                    inode: id,
                    handle: Some(file),
                    append: options.append,
                    read: options.read,
                    write: options.write,
                }));
            }
        };
        let node = self.inner().state.borrow().nodes[&id].clone();
        if node.kind == EntryType::Directory {
            return Err(FileError::IsDirectory);
        }
        permission(
            &node,
            if options.read { 0o400 } else { 0 } | if options.write { 0o200 } else { 0 },
        )?;
        let mut raw_options = options;
        raw_options.create = false;
        raw_options.exclusive = false;
        raw_options.append = false;
        let modified = if options.truncate {
            self.inner().stamp()?
        } else {
            None
        };
        let file = self.inner().raw.open(
            node.backing.as_ref().ok_or(FileError::Invalid)?,
            raw_options,
        )?;
        self.inner().retain(id);
        if let Some(time) = modified {
            self.inner()
                .state
                .borrow_mut()
                .nodes
                .get_mut(&id)
                .unwrap()
                .modified = time;
            self.inner().dirty.set(true);
        }
        Ok(Box::new(OpenFile {
            inner: self.inner().clone(),
            inode: id,
            handle: Some(file),
            append: options.append,
            read: options.read,
            write: options.write,
        }))
    }

    fn metadata(&self, path: &str) -> Result<Metadata, FileError> {
        self.inner().metadata(self.walk(path, true)?)
    }
    fn symlink_metadata(&self, path: &str) -> Result<Metadata, FileError> {
        self.inner().metadata(self.walk(path, false)?)
    }
    fn read_directory(&self, path: &str) -> Result<DirectoryEntries, FileError> {
        let id = self.walk(path, true)?;
        let state = self.inner().state.borrow();
        let node = &state.nodes[&id];
        if node.kind != EntryType::Directory {
            return Err(FileError::NotDirectory);
        }
        permission(node, 0o400)?;
        let mut entries = DirectoryEntries::new();
        entries.push(".".into(), EntryType::Directory)?;
        entries.push("..".into(), EntryType::Directory)?;
        for (inode, _, name) in state.entries().filter(|(_, parent, _)| *parent == id) {
            entries.push(name.into(), state.nodes[&inode].kind)?;
        }
        Ok(entries)
    }
    fn create_directory(&self, path: &str) -> Result<(), FileError> {
        match self.walk(path, false) {
            Ok(_) => return Err(FileError::Exists),
            Err(FileError::NotFound) => (),
            Err(error) => return Err(error),
        }
        let (parent, name) = self.parent(path)?;
        let mut candidate = self.inner().state.borrow().clone();
        if candidate.child(parent, &name).is_some() {
            return Err(FileError::Exists);
        }
        let now = self.stamp()?;
        self.inner().reserve_entry(&candidate)?;
        candidate.allocate(Node::new(Some(parent), name, EntryType::Directory, now))?;
        candidate.nodes.get_mut(&parent).unwrap().modified = now;
        self.inner().commit(candidate)
    }
    fn unlink(&self, path: &str) -> Result<(), FileError> {
        self.detach(path, false)
    }
    fn remove_directory(&self, path: &str) -> Result<(), FileError> {
        self.detach(path, true)
    }
    fn rename(&self, source: &str, destination: &str) -> Result<(), FileError> {
        self.move_to(source, self, destination)
    }
    fn hard_link(&self, source: &str, destination: &str, follow: bool) -> Result<(), FileError> {
        self.link_to(source, self, destination, follow)
    }
    fn hard_link_to(
        &self,
        source: &str,
        other: &dyn FileSystem,
        destination: &str,
        follow: bool,
    ) -> Result<(), FileError> {
        let other = (other as &dyn core::any::Any)
            .downcast_ref::<Self>()
            .ok_or(FileError::CrossDevice)?;
        self.link_to(source, other, destination, follow)
    }
    fn rename_to(
        &self,
        source: &str,
        other: &dyn FileSystem,
        destination: &str,
    ) -> Result<(), FileError> {
        let other = (other as &dyn core::any::Any)
            .downcast_ref::<Self>()
            .ok_or(FileError::CrossDevice)?;
        self.move_to(source, other, destination)
    }
    fn identity(&self) -> usize {
        Rc::as_ptr(self.inner()) as usize
    }
    fn directory_view(&self, path: &str) -> Result<Option<Rc<dyn FileSystem>>, FileError> {
        let id = self.walk(path, true)?;
        if self.inner().state.borrow().nodes[&id].kind != EntryType::Directory {
            return Err(FileError::NotDirectory);
        }
        Ok(Some(Rc::new(Self::view(self.inner().clone(), id))))
    }
    fn resolve_path(&self, base: &str, path: &str) -> Result<String, FileError> {
        self.inner().check()?;
        if path.is_empty() {
            return Err(FileError::NotFound);
        }
        if path.starts_with('/') || path.contains(['\\', '\0']) {
            return Err(FileError::NotCapable);
        }
        if base.is_empty() {
            Ok(path.into())
        } else {
            Ok(alloc::format!("{base}/{path}"))
        }
    }
    fn symlink(&self, target: &str, path: &str) -> Result<(), FileError> {
        if target.is_empty() {
            return Err(FileError::NotFound);
        }
        components(target)?;
        let (parent, name) = self.parent(path)?;
        let mut candidate = self.inner().state.borrow().clone();
        if candidate.child(parent, &name).is_some() {
            return Err(FileError::Exists);
        }
        let now = self.stamp()?;
        let mut node = Node::new(Some(parent), name, EntryType::Symlink, now);
        node.mode = 0o777;
        node.target = Some(target.into());
        self.inner().reserve_entry(&candidate)?;
        candidate.allocate(node)?;
        candidate.nodes.get_mut(&parent).unwrap().modified = now;
        self.inner().commit(candidate)
    }
    fn read_link(&self, path: &str) -> Result<String, FileError> {
        let id = self.walk(path, false)?;
        self.inner().state.borrow().nodes[&id]
            .target
            .clone()
            .ok_or(FileError::Invalid)
    }
    fn set_times(
        &self,
        path: &str,
        follow: bool,
        access: TimeUpdate,
        modify: TimeUpdate,
    ) -> Result<(), FileError> {
        set_times(self.inner(), self.walk(path, follow)?, access, modify)
    }
    fn set_mode(&self, path: &str, mode: u16) -> Result<(), FileError> {
        set_mode(self.inner(), self.walk(path, true)?, mode)
    }
    fn capacity(&self) -> Result<Capacity, FileError> {
        self.inner().raw.capacity()
    }
    fn filesystem_statistics(&self, path: &str) -> Result<crate::FileSystemStatistics, FileError> {
        self.walk(path, true)?;
        self.inner().filesystem_statistics()
    }
    fn mounts(&self) -> Result<Vec<crate::Mount>, FileError> {
        Ok(alloc::vec![crate::Mount {
            source: "workspace".into(),
            path: "/".into(),
            kind: "efi-agent".into(),
            read_only: false,
        }])
    }
}

fn set_times(
    inner: &Inner,
    id: u64,
    access: TimeUpdate,
    modify: TimeUpdate,
) -> Result<(), FileError> {
    let mut candidate = inner.state.borrow().clone();
    let now = if matches!(access, TimeUpdate::Now) || matches!(modify, TimeUpdate::Now) {
        Some(inner.now()?)
    } else {
        None
    };
    let node = candidate.nodes.get_mut(&id).ok_or(FileError::NotFound)?;
    let update = |value: &mut u64, change| match change {
        TimeUpdate::Keep => (),
        TimeUpdate::Set(time) => *value = time,
        TimeUpdate::Now => *value = now.unwrap(),
    };
    update(&mut node.accessed, access);
    update(&mut node.modified, modify);
    inner.commit(candidate)
}

fn set_mode(inner: &Inner, id: u64, mode: u16) -> Result<(), FileError> {
    if mode & !0o7777 != 0 {
        return Err(FileError::Invalid);
    }
    let mut candidate = inner.state.borrow().clone();
    candidate
        .nodes
        .get_mut(&id)
        .ok_or(FileError::NotFound)?
        .mode = mode;
    inner.commit(candidate)
}

struct OpenFile {
    inner: Rc<Inner>,
    inode: u64,
    handle: Option<Box<dyn File>>,
    append: bool,
    read: bool,
    write: bool,
}

impl File for OpenFile {
    fn filesystem_statistics(&self) -> Result<crate::FileSystemStatistics, FileError> {
        self.inner.filesystem_statistics()
    }
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, FileError> {
        self.inner.check()?;
        if !self.read {
            return Err(FileError::Access);
        }
        let now = self.inner.stamp()?;
        let count = self.handle.as_mut().unwrap().read(buffer)?;
        if count != 0
            && let Some(now) = now
        {
            self.inner
                .state
                .borrow_mut()
                .nodes
                .get_mut(&self.inode)
                .unwrap()
                .accessed = now;
            self.inner.dirty.set(true);
        }
        Ok(count)
    }
    fn write(&mut self, buffer: &[u8]) -> Result<usize, FileError> {
        self.inner.check()?;
        if !self.write {
            return Err(FileError::Access);
        }
        if buffer.is_empty() {
            return Ok(0);
        }
        let handle = self.handle.as_mut().unwrap();
        let position = handle.seek(0, if self.append { 2 } else { 1 })?;
        if position >= MAX_RESIZE_BYTES {
            return Err(FileError::FileTooLarge);
        }
        let count = buffer.len().min((MAX_RESIZE_BYTES - position) as usize);
        let now = self.inner.stamp()?;
        let count = handle.write(&buffer[..count])?;
        if count != 0
            && let Some(now) = now
        {
            self.inner
                .state
                .borrow_mut()
                .nodes
                .get_mut(&self.inode)
                .unwrap()
                .modified = now;
            self.inner.dirty.set(true);
        }
        Ok(count)
    }
    fn seek(&mut self, offset: i64, whence: u32) -> Result<u64, FileError> {
        self.inner.check()?;
        self.handle.as_mut().unwrap().seek(offset, whence)
    }
    fn metadata(&mut self) -> Result<Metadata, FileError> {
        self.inner.check()?;
        let info = self.handle.as_mut().unwrap().metadata()?;
        let state = self.inner.state.borrow();
        let mut metadata = node_metadata(&state, self.inode, info.size, info.allocated);
        metadata.device = u64::from(self.inner.device());
        Ok(metadata)
    }
    fn flush(&mut self) -> Result<(), FileError> {
        if self.write {
            self.handle.as_mut().unwrap().flush()?;
        }
        self.inner.flush()
    }
    fn resize(&mut self, size: u64) -> Result<(), FileError> {
        self.inner.check()?;
        if !self.write {
            return Err(FileError::Access);
        }
        if size > MAX_RESIZE_BYTES {
            return Err(FileError::FileTooLarge);
        }
        let now = self.inner.stamp()?;
        self.handle.as_mut().unwrap().resize(size)?;
        if let Some(now) = now {
            self.inner
                .state
                .borrow_mut()
                .nodes
                .get_mut(&self.inode)
                .unwrap()
                .modified = now;
            self.inner.dirty.set(true);
        }
        Ok(())
    }
    fn set_times(&mut self, access: TimeUpdate, modify: TimeUpdate) -> Result<(), FileError> {
        set_times(&self.inner, self.inode, access, modify)
    }
    fn set_mode(&mut self, mode: u16) -> Result<(), FileError> {
        set_mode(&self.inner, self.inode, mode)
    }
}

impl Drop for OpenFile {
    fn drop(&mut self) {
        // Read-only closes defer access-time persistence to an explicit fsync
        // or workspace flush, instead of rewriting the journal on every read.
        if self.write
            && let Err(error) = self.flush()
        {
            self.inner.record(error);
        }
        self.handle.take();
        self.inner.release(self.inode);
        if let Err(error) = self.inner.collect() {
            self.inner.record(error);
        }
    }
}

impl Drop for Anchor {
    fn drop(&mut self) {
        self.inner.release(self.inode);
        if let Err(error) = self.inner.collect() {
            self.inner.record(error);
        }
    }
}
