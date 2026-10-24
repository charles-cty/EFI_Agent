//! Process information and descriptor links over live dispatcher state.
use super::{DirectoryEntries, EntryType, FileError, Metadata, Scope, VirtualFileSystem};
use crate::{
    Pid, Status, World,
    clock::{Clock, ClockId},
    io::{Descriptor, TableView},
};
use alloc::{
    collections::BTreeMap,
    format,
    rc::Rc,
    string::{String, ToString},
    vec::Vec,
};
use core::cell::RefCell;

pub(crate) struct Record {
    parent: Pid,
    state: char,
    command: String,
    executable: String,
    arguments: Vec<String>,
    environment: Vec<String>,
    cwd: Option<String>,
    descriptors: TableView,
}

struct Timing {
    clock: Rc<dyn Clock>,
    start: Option<u64>,
    last: Option<u64>,
    idle_ns: u64,
    idle: bool,
    failure: Option<FileError>,
}

impl Timing {
    fn sample(&mut self) -> Result<(u64, u64), FileError> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        let now = self
            .clock
            .now(ClockId::Monotonic)
            .map_err(|error| match error {
                crate::clock::ClockError::Unavailable => FileError::Unsupported,
                crate::clock::ClockError::Overflow => FileError::Overflow,
                _ => FileError::Io,
            })?;
        let start = self.start.ok_or(FileError::Unsupported)?;
        let elapsed = now.checked_sub(start).ok_or(FileError::Io)?;
        let interval = now
            .checked_sub(self.last.unwrap_or(now))
            .ok_or(FileError::Io)?;
        let idle = if self.idle {
            self.idle_ns
                .checked_add(interval)
                .ok_or(FileError::Overflow)?
        } else {
            self.idle_ns
        };
        self.last = Some(now);
        self.idle_ns = idle;
        Ok((elapsed, idle))
    }
}

#[derive(Default)]
pub(crate) struct Registry {
    records: RefCell<BTreeMap<Pid, Record>>,
    timing: RefCell<Option<Timing>>,
}

impl Registry {
    pub(super) fn statistics(&self) -> super::FileSystemStatistics {
        let records = self.records.borrow();
        let mut statistics = super::special_statistics(4);
        statistics.files = 4 + records
            .values()
            .map(|record| {
                let descriptors = record
                    .descriptors
                    .upgrade()
                    .map(|table| {
                        table
                            .borrow()
                            .values()
                            .filter_map(alloc::rc::Weak::upgrade)
                            .filter(|descriptor| {
                                !matches!(descriptor.as_ref(), Descriptor::Control(_))
                            })
                            .count()
                    })
                    .unwrap_or(0);
                8 + descriptors as u64
            })
            .sum::<u64>();
        statistics
    }
    pub(super) fn set_clock(&self, clock: Rc<dyn Clock>) {
        let start = clock.now(ClockId::Monotonic).ok();
        *self.timing.borrow_mut() = Some(Timing {
            clock,
            start,
            last: start,
            idle_ns: 0,
            idle: true,
            failure: None,
        });
    }

    pub(crate) fn activity(&self, idle: bool) {
        if let Some(timing) = self.timing.borrow_mut().as_mut() {
            // A missed state transition makes idle accounting unavailable.
            match timing.sample() {
                Ok(_) => timing.idle = idle,
                Err(error) => timing.failure = Some(error),
            }
        }
    }

    fn uptime(&self) -> Result<Vec<u8>, FileError> {
        let (elapsed, idle) = self
            .timing
            .borrow_mut()
            .as_mut()
            .ok_or(FileError::Unsupported)?
            .sample()?;
        Ok(format!(
            "{}.{:02} {}.{:02}\n",
            elapsed / 1_000_000_000,
            elapsed / 10_000_000 % 100,
            idle / 1_000_000_000,
            idle / 10_000_000 % 100
        )
        .into_bytes())
    }

    pub(super) fn descriptor(&self, pid: Pid, fd: u32) -> Result<Descriptor, FileError> {
        let records = self.records.borrow();
        let table = records
            .get(&pid)
            .ok_or(FileError::NotFound)?
            .descriptors
            .upgrade()
            .ok_or(FileError::NotFound)?;
        let descriptor = table
            .borrow()
            .get(&fd)
            .and_then(alloc::rc::Weak::upgrade)
            .ok_or(FileError::NotFound)?;
        if matches!(descriptor.as_ref(), Descriptor::Control(_)) {
            return Err(FileError::NotFound);
        }
        Ok(descriptor.duplicate())
    }
}

impl World {
    pub(crate) fn sync_process_files(&self) {
        let mut records = self.process_files.records.borrow_mut();
        records.retain(|pid, _| self.processes.contains_key(pid));
        for (&pid, process) in &self.processes {
            let context = process
                .execution
                .as_ref()
                .map(|execution| execution.store.data());
            let state = match process.status {
                Status::Ready => 'R',
                Status::Exited(_) | Status::Trapped(_) => 'Z',
                _ => 'S',
            };
            let record = records.entry(pid).or_insert_with(|| Record {
                parent: process.parent,
                state,
                command: process.command.clone(),
                executable: process.executable.clone(),
                arguments: context
                    .map(|context| context.args.clone())
                    .unwrap_or_default(),
                environment: context
                    .map(|context| context.environment.clone())
                    .unwrap_or_default(),
                cwd: context.map(|context| context.cwd.clone()),
                descriptors: context
                    .map(|context| context.descriptors.view())
                    .unwrap_or_default(),
            });
            record.parent = process.parent;
            record.state = state;
            record.cwd = context.map(|context| context.cwd.clone());
            record.descriptors = context
                .map(|context| context.descriptors.view())
                .unwrap_or_default();
            if context.is_none() {
                record.arguments = Vec::new();
                record.environment = Vec::new();
            }
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Node {
    Root,
    SelfLink(Pid),
    Uptime,
    Mounts,
    Process(Pid),
    Status(Pid),
    Arguments(Pid),
    Environment(Pid),
    Cwd(Pid),
    Executable(Pid),
    Fds(Pid),
    Fd(Pid, u32),
    DeviceLink(&'static str),
}

impl Node {
    fn identity(self) -> (u64, u64) {
        match self {
            Self::DeviceLink(target) => (
                2,
                100 + match target {
                    "/proc/self/fd" => 0,
                    "/proc/self/fd/0" => 1,
                    "/proc/self/fd/1" => 2,
                    _ => 3,
                },
            ),
            Self::Root => (4, 1),
            Self::SelfLink(_) => (4, 2),
            Self::Uptime => (4, 3),
            Self::Mounts => (4, 4),
            Self::Process(pid) => (4, 512 + u64::from(pid) * 256),
            Self::Status(pid) => (4, 513 + u64::from(pid) * 256),
            Self::Arguments(pid) => (4, 514 + u64::from(pid) * 256),
            Self::Environment(pid) => (4, 515 + u64::from(pid) * 256),
            Self::Cwd(pid) => (4, 516 + u64::from(pid) * 256),
            Self::Executable(pid) => (4, 517 + u64::from(pid) * 256),
            Self::Fds(pid) => (4, 518 + u64::from(pid) * 256),
            Self::Fd(pid, fd) => (4, 520 + u64::from(pid) * 256 + u64::from(fd)),
        }
    }

    pub(super) fn kind(self) -> EntryType {
        match self {
            Self::Root | Self::Process(_) | Self::Fds(_) => EntryType::Directory,
            Self::SelfLink(_)
            | Self::Cwd(_)
            | Self::Executable(_)
            | Self::Fd(_, _)
            | Self::DeviceLink(_) => EntryType::Symlink,
            _ => EntryType::File,
        }
    }
}

impl VirtualFileSystem {
    pub(super) fn process_open(
        &self,
        node: Node,
        options: super::OpenOptions,
    ) -> Result<alloc::boxed::Box<dyn super::File>, FileError> {
        if options.create && options.exclusive {
            return Err(FileError::Exists);
        }
        if let Node::Fd(pid, fd) = node {
            return match self.process_files.descriptor(pid, fd)? {
                Descriptor::File {
                    handle,
                    read,
                    write,
                    flags,
                    ..
                } => {
                    if (options.read && !read) || (options.write && !write) {
                        return Err(FileError::Access);
                    }
                    Ok(alloc::boxed::Box::new(SharedFile {
                        handle,
                        read: options.read,
                        write: options.write,
                        flags,
                    }))
                }
                Descriptor::Directory(_) => Err(FileError::IsDirectory),
                _ => Err(FileError::Unsupported),
            };
        }
        if options.write || options.truncate || options.append {
            return Err(FileError::ReadOnly);
        }
        Ok(alloc::boxed::Box::new(super::VirtualFile {
            data: self.process_data(node)?,
            offset: 0,
            read: options.read,
            metadata: self.process_metadata(node, true)?,
            process_files: self.process_files.clone(),
        }))
    }

    pub(super) fn process_descriptor(&self, node: Node) -> Result<Option<Descriptor>, FileError> {
        match node {
            Node::Fd(pid, fd) => self.process_files.descriptor(pid, fd).map(Some),
            _ => Ok(None),
        }
    }
    pub(crate) fn for_process(&self, pid: Pid) -> Rc<Self> {
        let mut view = self.clone();
        view.process = Some(pid);
        Rc::new(view)
    }

    pub(super) fn process_node(&self, path: &str) -> Result<Option<Node>, FileError> {
        if self.scope == Scope::Workspace {
            return Ok(None);
        }
        let node = match path {
            "dev/fd" => Node::DeviceLink("/proc/self/fd"),
            "dev/stdin" => Node::DeviceLink("/proc/self/fd/0"),
            "dev/stdout" => Node::DeviceLink("/proc/self/fd/1"),
            "dev/stderr" => Node::DeviceLink("/proc/self/fd/2"),
            "proc" => Node::Root,
            "proc/self" => Node::SelfLink(self.process.ok_or(FileError::NotFound)?),
            "proc/uptime" => Node::Uptime,
            "proc/mounts" => Node::Mounts,
            _ => {
                let Some(rest) = path.strip_prefix("proc/") else {
                    return Ok(None);
                };
                let (number, rest) = rest.split_once('/').unwrap_or((rest, ""));
                let pid: Pid = number.parse().map_err(|_| FileError::NotFound)?;
                if number != pid.to_string()
                    || !self.process_files.records.borrow().contains_key(&pid)
                {
                    return Err(FileError::NotFound);
                }
                match rest {
                    "" => Node::Process(pid),
                    "status" => Node::Status(pid),
                    "cmdline" => Node::Arguments(pid),
                    "environ" => Node::Environment(pid),
                    "cwd" => Node::Cwd(pid),
                    "exe" => Node::Executable(pid),
                    "fd" => Node::Fds(pid),
                    "mounts" => Node::Mounts,
                    _ => {
                        let number = rest.strip_prefix("fd/").ok_or(FileError::NotFound)?;
                        let fd: u32 = number.parse().map_err(|_| FileError::NotFound)?;
                        if number != fd.to_string() {
                            return Err(FileError::NotFound);
                        }
                        self.process_files.descriptor(pid, fd)?;
                        Node::Fd(pid, fd)
                    }
                }
            }
        };
        Ok(Some(node))
    }

    pub(super) fn process_metadata(&self, node: Node, follow: bool) -> Result<Metadata, FileError> {
        if let Node::Fd(pid, fd) = node
            && follow
        {
            return self.process_files.descriptor(pid, fd)?.metadata();
        }
        let kind = node.kind();
        let terminal_link = match node {
            Node::Cwd(pid) | Node::Executable(pid) => self
                .process_files
                .records
                .borrow()
                .get(&pid)
                .is_some_and(|record| record.state == 'Z'),
            _ => false,
        };
        let size = if kind == EntryType::Symlink && !terminal_link {
            self.process_link(node)?.len() as u64
        } else {
            0
        };
        let mut info = super::virtual_metadata(kind, size);
        (info.device, info.inode) = node.identity();
        info.mode = match kind {
            EntryType::Directory => 0o555,
            EntryType::Symlink => 0o777,
            _ => 0o444,
        };
        Ok(info)
    }

    pub(super) fn process_link(&self, node: Node) -> Result<String, FileError> {
        match node {
            Node::SelfLink(pid) => Ok(pid.to_string()),
            Node::DeviceLink(target) => Ok(target.into()),
            Node::Cwd(pid) => self
                .process_files
                .records
                .borrow()
                .get(&pid)
                .and_then(|record| record.cwd.clone())
                .ok_or(FileError::NotFound),
            Node::Executable(pid) => self
                .process_files
                .records
                .borrow()
                .get(&pid)
                .filter(|record| record.state != 'Z')
                .map(|record| record.executable.clone())
                .ok_or(FileError::NotFound),
            Node::Fd(pid, fd) => self.process_files.descriptor(pid, fd)?.description(),
            _ => Err(FileError::Invalid),
        }
    }

    pub(super) fn process_data(&self, node: Node) -> Result<Vec<u8>, FileError> {
        if matches!(node, Node::Uptime) {
            return self.process_files.uptime();
        }
        if matches!(node, Node::Mounts) {
            return self.mount_table();
        }
        let records = self.process_files.records.borrow();
        match node {
            Node::Status(pid) => {
                let record = records.get(&pid).ok_or(FileError::NotFound)?;
                let name = record
                    .command
                    .replace('\\', "\\\\")
                    .replace('\n', "\\n")
                    .replace('\t', "\\t");
                let state = match record.state {
                    'R' => "R (running)",
                    'Z' => "Z (zombie)",
                    _ => "S (sleeping)",
                };
                Ok(format!("Name:\t{name}\nState:\t{state}\nPid:\t{pid}\nPPid:\t{}\nUid:\t1\t1\t1\t1\nGid:\t1\t1\t1\t1\n", record.parent).into_bytes())
            }
            Node::Arguments(pid) | Node::Environment(pid) => {
                let record = records.get(&pid).ok_or(FileError::NotFound)?;
                let entries = if matches!(node, Node::Arguments(_)) {
                    &record.arguments
                } else {
                    &record.environment
                };
                let mut data = Vec::new();
                for entry in entries {
                    data.extend_from_slice(entry.as_bytes());
                    data.push(0);
                }
                Ok(data)
            }
            _ => Err(if node.kind() == EntryType::Directory {
                FileError::IsDirectory
            } else {
                FileError::Unsupported
            }),
        }
    }

    pub(super) fn process_directory(&self, node: Node) -> Result<DirectoryEntries, FileError> {
        let mut entries = DirectoryEntries::new();
        entries.push(".".into(), EntryType::Directory)?;
        entries.push("..".into(), EntryType::Directory)?;
        match node {
            Node::Root => {
                for (name, kind) in [
                    ("self", EntryType::Symlink),
                    ("uptime", EntryType::File),
                    ("mounts", EntryType::File),
                ] {
                    entries.push(name.into(), kind)?;
                }
                for pid in self.process_files.records.borrow().keys() {
                    entries.push(pid.to_string(), EntryType::Directory)?;
                }
            }
            Node::Process(_) => {
                for (name, kind) in [
                    ("status", EntryType::File),
                    ("cmdline", EntryType::File),
                    ("environ", EntryType::File),
                    ("mounts", EntryType::File),
                    ("cwd", EntryType::Symlink),
                    ("exe", EntryType::Symlink),
                    ("fd", EntryType::Directory),
                ] {
                    entries.push(name.into(), kind)?;
                }
            }
            Node::Fds(pid) => {
                let records = self.process_files.records.borrow();
                let record = records.get(&pid).ok_or(FileError::NotFound)?;
                if let Some(table) = record.descriptors.upgrade() {
                    for (fd, descriptor) in table.borrow().iter() {
                        if let Some(descriptor) = descriptor.upgrade()
                            && !matches!(descriptor.as_ref(), Descriptor::Control(_))
                        {
                            entries.push(fd.to_string(), EntryType::Symlink)?;
                        }
                    }
                }
            }
            _ => return Err(FileError::NotDirectory),
        }
        Ok(entries)
    }

    pub(crate) fn descriptor_path(&self, path: &str) -> Result<Option<Descriptor>, FileError> {
        let path = self.walk(path, super::walk::Follow::Final, true)?;
        let owner = path.owner(self);
        match owner.process_node(&owner.mounted(&path)) {
            Ok(Some(Node::Fd(pid, fd))) => owner.process_files.descriptor(pid, fd).map(Some),
            Ok(_) | Err(FileError::NotFound) => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub(crate) fn mount_table(&self) -> Result<Vec<u8>, FileError> {
        let mut result = String::new();
        for mount in self.mounts_raw()? {
            for field in [&mount.source, &mount.path, &mount.kind] {
                if field.is_empty() || field.contains('\0') {
                    return Err(FileError::Invalid);
                }
                for ch in field.chars() {
                    match ch {
                        ' ' => result.push_str("\\040"),
                        '\t' => result.push_str("\\011"),
                        '\n' => result.push_str("\\012"),
                        '\\' => result.push_str("\\134"),
                        _ => result.push(ch),
                    }
                }
                result.push(' ');
            }
            result.push_str(if mount.read_only {
                "ro 0 0\n"
            } else {
                "rw 0 0\n"
            });
            if result.len() > 65536 {
                return Err(FileError::ResourceLimit);
            }
        }
        Ok(result.into_bytes())
    }
}

struct SharedFile {
    handle: Rc<RefCell<alloc::boxed::Box<dyn super::File>>>,
    read: bool,
    write: bool,
    flags: Rc<core::cell::Cell<u16>>,
}

impl super::File for SharedFile {
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, FileError> {
        if !self.read {
            return Err(FileError::Access);
        }
        self.handle.borrow_mut().read(buffer)
    }
    fn write(&mut self, buffer: &[u8]) -> Result<usize, FileError> {
        if !self.write {
            return Err(FileError::Access);
        }
        let mut handle = self.handle.borrow_mut();
        if self.flags.get() & 1 != 0 {
            handle.seek(0, 2)?;
        }
        handle.write(buffer)
    }
    fn seek(&mut self, offset: i64, whence: u32) -> Result<u64, FileError> {
        self.handle.borrow_mut().seek(offset, whence)
    }
    fn metadata(&mut self) -> Result<Metadata, FileError> {
        self.handle.borrow_mut().metadata()
    }
    fn flush(&mut self) -> Result<(), FileError> {
        self.handle.borrow_mut().flush()
    }
    fn sync(&mut self, data_only: bool) -> Result<(), FileError> {
        self.handle.borrow_mut().sync(data_only)
    }
    fn resize(&mut self, size: u64) -> Result<(), FileError> {
        if !self.write {
            return Err(FileError::Access);
        }
        self.handle.borrow_mut().resize(size)
    }
    fn set_mode(&mut self, mode: u16) -> Result<(), FileError> {
        self.handle.borrow_mut().set_mode(mode)
    }
    fn set_times(
        &mut self,
        access: super::TimeUpdate,
        modify: super::TimeUpdate,
    ) -> Result<(), FileError> {
        self.handle.borrow_mut().set_times(access, modify)
    }
    fn filesystem_statistics(&self) -> Result<super::FileSystemStatistics, FileError> {
        self.handle.borrow().filesystem_statistics()
    }
}
