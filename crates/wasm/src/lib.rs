#![no_std]

extern crate alloc;

mod abi;
pub mod clock;
mod dns;
pub mod filesystem;
mod filesystem_abi;
mod io;
pub mod network;
mod process_control;
pub mod random;
mod socket;
pub use io::{MAX_DESCRIPTORS, PIPE_CAPACITY};

use alloc::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    rc::Rc,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use core::{cell::RefCell, fmt};
use wasmi::{
    CompilationMode, Config, Engine, Module, Store, StoreLimits, StoreLimitsBuilder, TypedFunc,
    TypedResumableCall, Val,
};

pub type Pid = u32;
pub const MAX_PROCESSES: usize = 32;
pub const MAX_MEMORY_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 512 * 1024;
const QUANTUM: u64 = 50_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Ready,
    Waiting(Pid),
    Connecting,
    Resolving,
    Closing(u32),
    Reading(u32),
    Writing(u32),
    Polling,
    Exited(i32),
    Trapped(String),
}

impl Status {
    pub fn is_live(&self) -> bool {
        matches!(
            self,
            Self::Ready
                | Self::Waiting(_)
                | Self::Connecting
                | Self::Resolving
                | Self::Closing(_)
                | Self::Reading(_)
                | Self::Writing(_)
                | Self::Polling
        )
    }
}

#[derive(Debug)]
pub enum Error {
    UnknownProgram(String),
    ProcessLimit,
    InvalidArguments,
    ArgumentsTooLarge,
    BadDescriptor,
    InvalidModule(wasmi::Error),
    ProgramFile(filesystem::FileError),
    UnknownProcess(Pid),
    NotExited(Pid),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownProgram(name) => write!(f, "Unknown WASM program: {name}"),
            Self::ProcessLimit => f.write_str("WASM process limit reached"),
            Self::InvalidArguments => f.write_str("Invalid WASM arguments"),
            Self::ArgumentsTooLarge => f.write_str("WASM arguments and environment exceed 64 KiB"),
            Self::BadDescriptor => f.write_str("Invalid WASM descriptor"),
            Self::InvalidModule(error) => write!(f, "WASM module: {error}"),
            Self::ProgramFile(error) => write!(f, "WASM program file: {error:?}"),
            Self::UnknownProcess(pid) => write!(f, "Unknown WASM process: {pid}"),
            Self::NotExited(pid) => write!(f, "WASM process {pid} has not exited"),
        }
    }
}
impl core::error::Error for Error {}
impl From<wasmi::Error> for Error {
    fn from(error: wasmi::Error) -> Self {
        Self::InvalidModule(error)
    }
}

impl Error {
    fn spawn_errno(&self) -> i32 {
        use wasmi::errors::{ErrorKind, InstantiationError, MemoryError, TableError};
        match self {
            Self::UnknownProgram(_) => -2,
            Self::ProcessLimit => -11,
            Self::BadDescriptor => -9,
            Self::InvalidArguments => -22,
            Self::ArgumentsTooLarge => -7,
            Self::InvalidModule(error)
                if matches!(
                    error.kind(),
                    ErrorKind::Memory(
                        MemoryError::OutOfSystemMemory
                            | MemoryError::ResourceLimiterDeniedAllocation
                    ) | ErrorKind::Table(TableError::OutOfSystemMemory)
                        | ErrorKind::Instantiation(InstantiationError::FailedToInstantiateMemory(
                            MemoryError::OutOfSystemMemory
                                | MemoryError::ResourceLimiterDeniedAllocation
                        ))
                        | ErrorKind::Instantiation(InstantiationError::FailedToInstantiateTable(
                            TableError::OutOfSystemMemory
                        ))
                ) =>
            {
                -12
            }
            _ => -8,
        }
    }
}

struct Context {
    pid: Pid,
    parent: Pid,
    args: Vec<String>,
    environment: Vec<String>,
    stdin: Rc<RefCell<VecDeque<u8>>>,
    stdout: Rc<RefCell<Vec<u8>>>,
    stderr: Rc<RefCell<Vec<u8>>>,
    descriptors: io::Table,
    close_on_exec: BTreeSet<u32>,
    umask: u16,
    creation_mode: Option<u16>,
    cwd: String,
    clock: Option<Rc<dyn clock::Clock>>,
    limits: StoreLimits,
    random: Option<Rc<dyn random::Random>>,
    network: Option<Rc<dyn network::Network>>,
    dns_server: Option<([u8; 4], u16)>,
    dns_query: Option<dns::Query>,
}

struct Process {
    // Wait/reap metadata and inherited output must outlive the Wasm Store.
    parent: Pid,
    root: Pid,
    stdout: Rc<RefCell<Vec<u8>>>,
    stderr: Rc<RefCell<Vec<u8>>>,
    status: Status,
    execution: Option<Execution>,
}

struct Execution {
    store: Store<Context>,
    memory: wasmi::Memory,
    entry: TypedFunc<(), ()>,
    continuation: Option<TypedResumableCall<()>>,
    reply: i32,
    wait_status: u32,
    control_wait: Option<(u32, u32, u32)>,
    pending_io: Option<abi::IoRequest>,
}

impl Process {
    fn execution(&self) -> &Execution {
        self.execution
            .as_ref()
            .expect("Live process has execution resources")
    }
}

/// A single-threaded process world. Host code calls `step` between firmware polls.
/// Each live process owns its store, memory and retained Wasm call stack.
/// Terminal records keep status and output but release execution resources.
pub struct World {
    engine: Engine,
    linker: wasmi::Linker<Context>,
    programs: BTreeMap<String, Module>,
    program_files: BTreeMap<String, String>,
    compiled_files: BTreeMap<String, Module>,
    processes: BTreeMap<Pid, Process>,
    ready: VecDeque<Pid>,
    next_pid: Pid,
    clock: Option<Rc<dyn clock::Clock>>,
    filesystem: Option<Rc<dyn filesystem::FileSystem>>,
    random: Option<Rc<dyn random::Random>>,
    network: Option<Rc<dyn network::Network>>,
    dns_server: Option<([u8; 4], u16)>,
}

impl World {
    /// Names implemented by the BusyBox multicall guest. They resolve to the
    /// ash image while retaining argv[0], so the guest can select the applet.
    fn ash_applet(name: &str) -> bool {
        let name = name.strip_prefix("/bin/").unwrap_or(name);
        matches!(
            name,
            "ash"
                | "cat"
                | "cp"
                | "date"
                | "env"
                | "grep"
                | "head"
                | "ls"
                | "mkdir"
                | "mv"
                | "rm"
                | "rmdir"
                | "touch"
                | "sort"
                | "uniq"
                | "cut"
                | "tr"
                | "realpath"
                | "od"
                | "base64"
                | "sha256sum"
                | "ln"
                | "sed"
                | "sleep"
                | "yes"
                | "awk"
                | "chmod"
                | "tail"
                | "wc"
                | "tee"
                | "basename"
                | "dirname"
                | "pwd"
                | "printenv"
                | "seq"
                | "true"
                | "false"
        )
    }

    pub fn new() -> Self {
        let mut config = Config::default();
        // Module start sections run during instantiation and cannot join the dispatcher.
        // WASI command modules use the separately scheduled `_start` export.
        // Compile before scheduling: lazy translation can exhaust fuel while
        // resolving a callee without returning a resumable Wasm continuation.
        config
            .consume_fuel(true)
            .allow_start_fn(false)
            // Asyncify adds helper frames around the guest's bounded shell
            // tree traversal. Keep its 256-node depth check below this limit.
            .set_max_recursion_depth(4096)
            .set_max_stack_height(8 * 1024 * 1024)
            .compilation_mode(CompilationMode::Eager);
        let engine = Engine::new(&config);
        let linker = abi::linker(&engine);
        Self {
            engine,
            linker,
            programs: BTreeMap::new(),
            program_files: BTreeMap::new(),
            compiled_files: BTreeMap::new(),
            processes: BTreeMap::new(),
            ready: VecDeque::new(),
            next_pid: 1,
            clock: None,
            filesystem: None,
            random: None,
            network: None,
            dns_server: None,
        }
    }

    pub fn register(&mut self, name: &str, wasm: &[u8]) -> Result<(), Error> {
        if name.is_empty() || name.contains('\0') {
            return Err(Error::InvalidArguments);
        }
        let module = Module::new(&self.engine, wasm)?;
        self.programs.insert(name.into(), module);
        Ok(())
    }

    pub fn has_program(&self, name: &str) -> bool {
        let name = name.strip_prefix("/bin/").unwrap_or(name);
        self.programs.contains_key(name)
            || self.program_files.contains_key(name)
            || (Self::ash_applet(name)
                && (self.programs.contains_key("ash") || self.program_files.contains_key("ash")))
    }

    /// Discover workspace modules without compiling them. Existing installations
    /// retain their loaded image when a workspace file changes.
    pub fn discover_programs(&mut self) -> Result<(), filesystem::FileError> {
        use filesystem::{EntryType, FileError, OpenOptions};
        let fs = self.filesystem.as_ref().ok_or(FileError::Unsupported)?;
        let entries = fs.read_directory("")?;
        let mut pending = BTreeMap::new();
        for (path, kind) in entries.iter() {
            if kind != EntryType::File {
                continue;
            }
            let Some(stem) = path.get(..path.len().saturating_sub(5)) else {
                continue;
            };
            if !path
                .get(stem.len()..)
                .is_some_and(|suffix| suffix.eq_ignore_ascii_case(".wasm"))
            {
                continue;
            }
            if stem.is_empty() {
                return Err(FileError::Invalid);
            }
            if self.has_program(stem) {
                continue;
            }
            if fs.metadata(path)?.size > filesystem::MAX_RESIZE_BYTES {
                return Err(FileError::FileTooLarge);
            }
            let mut file = fs.open(
                path,
                OpenOptions {
                    read: true,
                    write: false,
                    create: false,
                    exclusive: false,
                    truncate: false,
                    append: false,
                },
            )?;
            let mut header = [0; 8];
            let mut used = 0;
            while used < header.len() {
                let count = file.read(&mut header[used..])?;
                if count == 0 {
                    return Err(FileError::Invalid);
                }
                used += count;
            }
            if header != *b"\0asm\x01\0\0\0" {
                return Err(FileError::Invalid);
            }
            pending.insert(stem.into(), path.into());
        }
        self.program_files.extend(pending);
        if !self.has_program("qjs")
            && let Some(path) = self.program_files.get("quickjs").cloned()
        {
            self.program_files.insert("qjs".into(), path);
        }
        Ok(())
    }

    fn load_program(&mut self, name: &str) -> Result<(), Error> {
        use filesystem::{FileError, OpenOptions};
        let name = name.strip_prefix("/bin/").unwrap_or(name);
        if self.programs.contains_key(name) {
            return Ok(());
        }
        let path = self
            .program_files
            .get(name)
            .ok_or_else(|| Error::UnknownProgram(name.into()))?
            .clone();
        let module = if let Some(module) = self.compiled_files.get(&path) {
            module.clone()
        } else {
            let fs = self
                .filesystem
                .as_ref()
                .expect("Program files require a filesystem");
            let load = || {
                let mut file = fs.open(
                    &path,
                    OpenOptions {
                        read: true,
                        write: false,
                        create: false,
                        exclusive: false,
                        truncate: false,
                        append: false,
                    },
                )?;
                let size = file.metadata()?.size;
                if size > filesystem::MAX_RESIZE_BYTES {
                    return Err(FileError::FileTooLarge);
                }
                let mut bytes = vec![0; size as usize];
                let mut used = 0;
                while used < bytes.len() {
                    let end = (used + io::PIPE_CAPACITY).min(bytes.len());
                    let count = file.read(&mut bytes[used..end])?;
                    if count == 0 {
                        return Err(FileError::Io);
                    }
                    used += count;
                }
                Ok(bytes)
            };
            let module = Module::new(&self.engine, &load().map_err(Error::ProgramFile)?)?;
            self.compiled_files.insert(path, module.clone());
            module
        };
        self.programs.insert(name.into(), module);
        Ok(())
    }

    /// Install the host clock before spawning. A live world's timer epochs must
    /// not change because the host replaced its provider.
    pub fn set_clock(&mut self, clock: Rc<dyn clock::Clock>) {
        assert!(
            self.processes.is_empty(),
            "Cannot replace a live world's clock"
        );
        self.clock = Some(clock);
    }

    /// Install the random source before spawning any process.
    pub fn set_random(&mut self, random: Rc<dyn random::Random>) {
        assert!(
            self.processes.is_empty(),
            "Cannot replace a live world's random source"
        );
        self.random = Some(random);
    }

    /// Install the network provider before spawning any process.
    pub fn set_network(&mut self, network: Rc<dyn network::Network>) {
        assert!(
            self.processes.is_empty(),
            "Cannot replace a live world's network"
        );
        self.network = Some(network);
    }

    pub fn set_dns_server(&mut self, address: [u8; 4], port: u16) {
        assert!(
            self.processes.is_empty(),
            "Cannot replace a live world's DNS server"
        );
        assert!(port != 0, "DNS port must be nonzero");
        self.dns_server = Some((address, port));
    }

    /// Install the root filesystem before spawning any process.
    pub fn set_filesystem(&mut self, filesystem: Rc<dyn filesystem::FileSystem>) {
        assert!(
            self.processes.is_empty(),
            "Cannot replace a live world's filesystem"
        );
        self.filesystem = Some(filesystem::VirtualFileSystem::new(filesystem));
    }

    pub fn spawn(&mut self, args: Vec<String>, stdin: Vec<u8>) -> Result<Pid, Error> {
        self.spawn_with_environment(args, Vec::new(), stdin)
    }

    /// Start a root with an explicit startup environment. No host variables are
    /// copied implicitly. Arguments and NAME=value entries share a 64 KiB limit.
    pub fn spawn_with_environment(
        &mut self,
        args: Vec<String>,
        environment: Vec<String>,
        stdin: Vec<u8>,
    ) -> Result<Pid, Error> {
        self.spawn_child(0, args, Some(environment), stdin, None)
    }

    fn spawn_child(
        &mut self,
        parent: Pid,
        args: Vec<String>,
        environment: Option<Vec<String>>,
        stdin: Vec<u8>,
        descriptor_map: Option<Vec<(u32, u32)>>,
    ) -> Result<Pid, Error> {
        let name = args.first().ok_or(Error::InvalidArguments)?;
        let environment = environment.unwrap_or_else(|| {
            self.processes[&parent]
                .execution()
                .store
                .data()
                .environment
                .clone()
        });
        if args.iter().any(|arg| arg.contains('\0')) || !valid_environment(&environment) {
            return Err(Error::InvalidArguments);
        }
        if args
            .iter()
            .chain(&environment)
            .map(|entry| entry.len() + 1)
            .sum::<usize>()
            > 64 * 1024
        {
            return Err(Error::ArgumentsTooLarge);
        }
        if self.processes.len() >= MAX_PROCESSES {
            return Err(Error::ProcessLimit);
        }
        let module_name = if Self::ash_applet(name) {
            "ash"
        } else {
            name.strip_prefix("/bin/").unwrap_or(name)
        };
        self.load_program(module_name)?;
        let module = self
            .programs
            .get(module_name)
            .ok_or_else(|| Error::UnknownProgram(name.clone()))?;
        let pid = self.next_pid;
        let next_pid = pid
            .checked_add(1)
            .filter(|next| *next <= i32::MAX as u32)
            .ok_or(Error::ProcessLimit)?;
        let (stdin, stdout, stderr) = if parent == 0 {
            (
                Rc::new(RefCell::new(stdin.into())),
                Rc::new(RefCell::new(Vec::new())),
                Rc::new(RefCell::new(Vec::new())),
            )
        } else {
            let context = self.processes[&parent].execution().store.data();
            (
                context.stdin.clone(),
                context.stdout.clone(),
                context.stderr.clone(),
            )
        };
        let mut descriptors = if parent == 0 {
            BTreeMap::from([
                (0, io::Descriptor::Input(stdin.clone())),
                (1, io::Descriptor::Output(stdout.clone())),
                (2, io::Descriptor::Output(stderr.clone())),
            ])
        } else {
            // Explicit mappings read the unchanged parent table, so cycles and
            // repeated sources work without temporary guest descriptors.
            let table = &self.processes[&parent].execution().store.data().descriptors;
            let mut inherited = BTreeMap::new();
            let explicit = descriptor_map.is_some();
            for (target, source) in descriptor_map.unwrap_or_else(|| vec![(0, 0), (1, 1), (2, 2)]) {
                if let Some(descriptor) = table.get(&source) {
                    if matches!(descriptor, io::Descriptor::Control(_)) {
                        if explicit {
                            return Err(Error::BadDescriptor);
                        }
                        continue;
                    }
                    if explicit {
                        inherited.insert(target, descriptor.duplicate());
                    } else if !self.processes[&parent]
                        .execution()
                        .store
                        .data()
                        .close_on_exec
                        .contains(&source)
                    {
                        inherited.insert(target, descriptor.clone());
                    }
                } else if explicit {
                    return Err(Error::BadDescriptor);
                }
            }
            inherited
        };
        if let Some(filesystem) = &self.filesystem {
            descriptors.insert(
                3,
                io::Descriptor::Directory(filesystem::Directory {
                    filesystem: filesystem.clone(),
                    path: String::new(),
                    preopen: true,
                    rights: io::DIRECTORY_RIGHTS,
                    inheriting: io::DIRECTORY_RIGHTS | io::FILE_RIGHTS,
                    entries: Rc::new(RefCell::new(None)),
                    flags: 0,
                }),
            );
        }
        let context = Context {
            pid,
            parent,
            args,
            environment,
            stdin,
            stdout: stdout.clone(),
            stderr: stderr.clone(),
            descriptors,
            close_on_exec: BTreeSet::new(),
            creation_mode: None,
            umask: if parent == 0 {
                0o022
            } else {
                self.processes[&parent].execution().store.data().umask
            },
            cwd: if parent == 0 {
                String::from("/")
            } else {
                self.processes[&parent].execution().store.data().cwd.clone()
            },
            clock: self.clock.clone(),
            random: self.random.clone(),
            network: self.network.clone(),
            dns_server: self.dns_server,
            dns_query: None,
            limits: StoreLimitsBuilder::new()
                .memory_size(MAX_MEMORY_BYTES)
                .memories(1)
                .tables(1)
                .table_elements(65536)
                .instances(1)
                .build(),
        };
        let mut store = Store::new(&self.engine, context);
        store.limiter(|context| &mut context.limits);
        // Instantiation copies data segments but cannot execute guest code:
        // start sections are rejected by the engine configuration.
        store.set_fuel(u64::MAX)?;
        let instance = self.linker.instantiate_and_start(&mut store, module)?;
        let memory = instance
            .get_memory(&store, "memory")
            .ok_or_else(|| wasmi::Error::new("Missing memory export"))?;
        let entry = instance.get_typed_func::<(), ()>(&store, "_start")?;
        self.next_pid = next_pid;
        let root = if parent == 0 {
            pid
        } else {
            self.processes[&parent].root
        };
        self.processes.insert(
            pid,
            Process {
                parent,
                root,
                stdout,
                stderr,
                status: Status::Ready,
                execution: Some(Execution {
                    store,
                    memory,
                    entry,
                    continuation: None,
                    reply: 0,
                    wait_status: 0,
                    control_wait: None,
                    pending_io: None,
                }),
            },
        );
        self.ready.push_back(pid);
        Ok(pid)
    }

    pub fn status(&self, pid: Pid) -> Result<&Status, Error> {
        Ok(&self
            .processes
            .get(&pid)
            .ok_or(Error::UnknownProcess(pid))?
            .status)
    }

    pub fn process_ids(&self) -> impl Iterator<Item = Pid> + '_ {
        self.processes.keys().copied()
    }

    pub fn parent(&self, pid: Pid) -> Result<Pid, Error> {
        self.processes
            .get(&pid)
            .map(|process| process.parent)
            .ok_or(Error::UnknownProcess(pid))
    }

    /// Original root identity survives parent exit, adoption and reaping.
    pub fn root(&self, pid: Pid) -> Result<Pid, Error> {
        self.processes
            .get(&pid)
            .map(|process| process.root)
            .ok_or(Error::UnknownProcess(pid))
    }

    pub fn take_output(&mut self, pid: Pid) -> Result<(Vec<u8>, Vec<u8>), Error> {
        let process = self
            .processes
            .get_mut(&pid)
            .ok_or(Error::UnknownProcess(pid))?;
        Ok((
            core::mem::take(&mut *process.stdout.borrow_mut()),
            core::mem::take(&mut *process.stderr.borrow_mut()),
        ))
    }

    pub fn cancel(&mut self, pid: Pid) -> Result<(), Error> {
        let process = self
            .processes
            .get_mut(&pid)
            .ok_or(Error::UnknownProcess(pid))?;
        if process.status.is_live() {
            // Dropping execution also closes descriptors and releases pending
            // I/O buffers and the retained Wasm continuation.
            process.execution = None;
            process.status = Status::Exited(130);
            self.ready.retain(|queued| *queued != pid);
            self.adopt_children(pid);
            self.wake_parent(pid);
            self.wake_io();
        }
        Ok(())
    }

    pub fn reap(&mut self, pid: Pid) -> Result<Status, Error> {
        let status = self.status(pid)?.clone();
        if status.is_live() {
            return Err(Error::NotExited(pid));
        }
        self.processes.remove(&pid);
        Ok(status)
    }

    /// Run one queue member until it yields, exhausts its fuel, exits, or traps.
    /// Fuel is a host responsiveness guard; explicit host calls provide cooperation.
    pub fn step(&mut self) -> Option<Pid> {
        self.wake_io();
        let pid = self.ready.pop_front()?;
        let process = self.processes.get_mut(&pid).expect("Ready process exists");
        assert_eq!(process.status, Status::Ready);
        let execution = process
            .execution
            .as_mut()
            .expect("Ready process has execution resources");
        let fuel = match &execution.continuation {
            Some(TypedResumableCall::OutOfFuel(call)) => QUANTUM.max(call.required_fuel()),
            _ => QUANTUM,
        };
        execution.store.set_fuel(fuel).expect("Fuel enabled");
        let result = match execution.continuation.take() {
            None => execution.entry.call_resumable(&mut execution.store, ()),
            Some(TypedResumableCall::HostTrap(call)) => {
                call.resume(&mut execution.store, &[Val::I32(execution.reply)])
            }
            Some(TypedResumableCall::OutOfFuel(call)) => call.resume(&mut execution.store),
            Some(TypedResumableCall::Finished(())) => unreachable!("Finished calls are not queued"),
        };
        match result {
            Ok(TypedResumableCall::Finished(())) => process.status = Status::Exited(0),
            Ok(TypedResumableCall::OutOfFuel(call)) => {
                execution.continuation = Some(TypedResumableCall::OutOfFuel(call));
                self.ready.push_back(pid);
            }
            Ok(TypedResumableCall::HostTrap(call)) => {
                if let Some(action) = call.host_error().downcast_ref::<abi::Action>().cloned() {
                    execution.continuation = Some(TypedResumableCall::HostTrap(call));
                    self.handle(pid, action);
                } else if let Some(code) = call.host_error().i32_exit_status() {
                    process.status = Status::Exited(code);
                } else {
                    process.status = Status::Trapped(call.host_error().to_string());
                }
            }
            Err(error) => {
                process.status = match error.i32_exit_status() {
                    Some(code) => Status::Exited(code),
                    None => Status::Trapped(error.to_string()),
                }
            }
        }
        if matches!(
            self.processes[&pid].status,
            Status::Exited(_) | Status::Trapped(_)
        ) {
            let process = self
                .processes
                .get_mut(&pid)
                .expect("Exiting process exists");
            process.execution = None;
            self.adopt_children(pid);
            self.wake_parent(pid);
        }
        self.wake_io();
        Some(pid)
    }

    fn handle(&mut self, pid: Pid, action: abi::Action) {
        let reply = match action {
            abi::Action::Control { fd, bytes, written } => {
                self.control(pid, fd, &bytes, written);
                return;
            }
            abi::Action::Yield => 0,
            abi::Action::ProgramExists(name) => i32::from(self.has_program(&name)),
            abi::Action::Spawn {
                args,
                environment,
                descriptors,
            } => match self.spawn_child(pid, args, environment, Vec::new(), descriptors) {
                Ok(child) => child as i32,
                Err(error) => error.spawn_errno(),
            },
            abi::Action::Io(request) => {
                let process = self.processes.get_mut(&pid).expect("Caller exists");
                process.status = match &request {
                    abi::IoRequest::Connect { .. } | abi::IoRequest::ConnectSocket { .. } => {
                        Status::Connecting
                    }
                    abi::IoRequest::Resolve { .. } => Status::Resolving,
                    abi::IoRequest::CloseSocket { fd } => Status::Closing(*fd),
                    abi::IoRequest::Read { fd, .. } => Status::Reading(*fd),
                    abi::IoRequest::Write { fd, .. } => Status::Writing(*fd),
                    abi::IoRequest::Poll { .. } => Status::Polling,
                };
                process
                    .execution
                    .as_mut()
                    .expect("Caller is live")
                    .pending_io = Some(request);
                return;
            }
            abi::Action::Wait {
                child,
                status_ptr,
                nohang,
            } => {
                if self
                    .processes
                    .get(&child)
                    .is_none_or(|process| process.parent != pid)
                {
                    -10
                } else if nohang && self.processes[&child].status.is_live() {
                    // A status buffer is written only when a child is reaped.
                    // Keep the caller runnable so it can drain other children.
                    0
                } else {
                    let parent = self.processes.get_mut(&pid).expect("Caller exists");
                    parent.status = Status::Waiting(child);
                    parent
                        .execution
                        .as_mut()
                        .expect("Caller is live")
                        .wait_status = status_ptr;
                    self.wake_parent(child);
                    return;
                }
            }
        };
        self.processes
            .get_mut(&pid)
            .expect("Caller exists")
            .execution
            .as_mut()
            .expect("Caller is live")
            .reply = reply;
        self.ready.push_back(pid);
    }

    fn wake_parent(&mut self, child: Pid) {
        let Some(process) = self.processes.get(&child) else {
            return;
        };
        let code = match process.status {
            Status::Exited(code) => code,
            Status::Trapped(_) => 128,
            _ => return,
        };
        let parent_pid = process.parent;
        let Some(parent) = self.processes.get_mut(&parent_pid) else {
            return;
        };
        if parent.status != Status::Waiting(child) {
            return;
        }
        let execution = parent.execution.as_mut().expect("Waiting parent is live");
        let control_wait = execution.control_wait.take();
        let result = if let Some((fd, written, count)) = control_wait {
            process_control::respond(execution, fd, written, count, child as i32, code)
        } else {
            execution.memory.write(
                &mut execution.store,
                execution.wait_status as usize,
                &code.to_le_bytes(),
            )
        };
        execution.reply = if result.is_ok() {
            if control_wait.is_some() {
                0
            } else {
                child as i32
            }
        } else {
            -14
        };
        parent.status = Status::Ready;
        self.ready.push_back(parent_pid);
        // A failed status write must allow the parent to retry waitpid.
        if result.is_ok() {
            self.processes.remove(&child);
        }
    }

    fn adopt_children(&mut self, parent: Pid) {
        for process in self.processes.values_mut() {
            if process.parent == parent {
                process.parent = 0;
                if let Some(execution) = &mut process.execution {
                    execution.store.data_mut().parent = 0;
                }
            }
        }
    }

    fn wake_io(&mut self) {
        for (pid, process) in &mut self.processes {
            let Some(execution) = &mut process.execution else {
                continue;
            };
            let Some(request) = &execution.pending_io else {
                continue;
            };
            let reply = match abi::perform_io(&mut execution.store, execution.memory, request) {
                Ok(None) => continue,
                Ok(Some(())) => 0,
                Err(errno) => errno,
            };
            execution.pending_io = None;
            execution.reply = reply;
            process.status = Status::Ready;
            self.ready.push_back(*pid);
        }
    }
}

fn valid_environment(environment: &[String]) -> bool {
    let mut names = alloc::collections::BTreeSet::new();
    environment.iter().all(|entry| {
        !entry.contains('\0')
            && entry
                .split_once('=')
                .is_some_and(|(name, _)| !name.is_empty() && names.insert(name))
    })
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}
