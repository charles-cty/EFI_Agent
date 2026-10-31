#![no_std]

extern crate alloc;

mod abi;
pub mod clock;
mod dns;
mod executable;
pub mod filesystem;
mod filesystem_abi;
pub mod installation;
mod io;
pub mod network;
mod process_control;
pub mod random;
pub mod session;
mod socket;
pub use io::{MAX_DESCRIPTORS, PIPE_CAPACITY};

use alloc::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    format,
    rc::{Rc, Weak},
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
use efi_agent_filesystem::installation::ASH_APPLETS;

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
    InvalidExecutable,
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
            Self::InvalidExecutable => f.write_str("Invalid executable format"),
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
            Self::ProgramFile(
                filesystem::FileError::Access | filesystem::FileError::IsDirectory,
            ) => -13,
            Self::ProgramFile(filesystem::FileError::NotFound) => -2,
            Self::ProgramFile(filesystem::FileError::NotDirectory) => -20,
            Self::ProgramFile(filesystem::FileError::Loop) => -40,
            Self::ProgramFile(filesystem::FileError::ResourceLimit) => -36,
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
    stdin: Rc<RefCell<io::Input>>,
    terminal: Rc<io::Terminal>,
    stdout: Rc<RefCell<Vec<u8>>>,
    stderr: Rc<RefCell<Vec<u8>>>,
    descriptors: io::Table,
    close_on_exec: BTreeSet<u32>,
    umask: u16,
    creation_mode: Option<u16>,
    cwd: Option<Rc<dyn filesystem::FileSystem>>,
    clock: Option<Rc<dyn clock::Clock>>,
    limits: StoreLimits,
    random: Option<Rc<dyn random::Random>>,
    network: Option<Rc<dyn network::Network>>,
    dns_server: Option<([u8; 4], u16)>,
    dns_query: Option<dns::Query>,
    session: Option<Rc<RefCell<session::Channel>>>,
}

#[derive(Default)]
pub(crate) struct SpawnSettings {
    pub(crate) cwd: Option<String>,
    pub(crate) umask: Option<u16>,
    pub(crate) session: Option<Rc<RefCell<session::Channel>>>,
}

struct Process {
    // Wait/reap metadata and inherited output must outlive the Wasm Store.
    parent: Pid,
    root: Pid,
    stdout: Rc<RefCell<Vec<u8>>>,
    stderr: Rc<RefCell<Vec<u8>>>,
    status: Status,
    execution: Option<Execution>,
    command: String,
    executable: String,
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
    // Keep one shared image alive across concurrent instances and aliases.
    // Its Engine releases translated code when the last execution finishes.
    _program: Rc<Program>,
}

impl Process {
    fn execution(&self) -> &Execution {
        self.execution
            .as_ref()
            .expect("Live process has execution resources")
    }
}

struct CompiledFile {
    revision: (u64, u64, u64, u64),
    program: Weak<Program>,
}

struct Program {
    module: Module,
    linker: wasmi::Linker<Context>,
}

impl Program {
    fn new(wasm: &[u8]) -> Result<Rc<Self>, Error> {
        let mut config = Config::default();
        // Compile before scheduling: lazy translation must also preserve fuel
        // continuations when resolving an untranslated callee.
        config
            .consume_fuel(true)
            .allow_start_fn(false)
            .ignore_custom_sections(true)
            .set_max_recursion_depth(4096)
            .set_max_stack_height(8 * 1024 * 1024)
            // Exited processes must not leave grown execution stacks cached.
            .set_max_cached_stacks(0)
            .compilation_mode(CompilationMode::Eager);
        let engine = Engine::new(&config);
        let module = Module::new(&engine, wasm)?;
        let linker = abi::linker(&engine);
        for import in module.imports() {
            let signature = linker
                .signatures
                .get(&(import.module().into(), import.name().into()));
            if !matches!(import.ty(), wasmi::ExternType::Func(ty) if signature == Some(ty)) {
                return Err(wasmi::Error::new(format!(
                    "Unsupported import {}.{}",
                    import.module(),
                    import.name()
                ))
                .into());
            }
        }
        if !matches!(module.get_export("_start"), Some(wasmi::ExternType::Func(ty)) if ty.params().is_empty() && ty.results().is_empty())
            || !matches!(
                module.get_export("memory"),
                Some(wasmi::ExternType::Memory(_))
            )
        {
            return Err(wasmi::Error::new("Expected memory and _start command exports").into());
        }
        Ok(Rc::new(Self {
            module,
            linker: linker.linker,
        }))
    }
}

/// A single-threaded process world. Host code calls `step` between firmware polls.
/// Each live process owns its store, memory and retained Wasm call stack.
/// Terminal records keep status and output but release execution resources.
pub struct World {
    programs: BTreeMap<String, Rc<Program>>,
    program_files: BTreeMap<String, String>,
    installed_names: BTreeSet<String>,
    compiled_files: BTreeMap<String, CompiledFile>,
    bin: Rc<RefCell<BTreeMap<String, Option<String>>>>,
    processes: BTreeMap<Pid, Process>,
    ready: VecDeque<Pid>,
    next_pid: Pid,
    clock: Option<Rc<dyn clock::Clock>>,
    filesystem: Option<Rc<dyn filesystem::FileSystem>>,
    process_files: Rc<filesystem::process::Registry>,
    random: Option<Rc<dyn random::Random>>,
    network: Option<Rc<dyn network::Network>>,
    dns_server: Option<([u8; 4], u16)>,
}

impl World {
    /// Names implemented by the BusyBox multicall guest. They resolve to the
    /// ash image while retaining argv[0], so the guest can select the applet.
    fn ash_applet(name: &str) -> bool {
        let name = name.strip_prefix("/bin/").unwrap_or(name);
        ASH_APPLETS.contains(&name)
    }

    pub fn new() -> Self {
        Self {
            programs: BTreeMap::new(),
            program_files: BTreeMap::new(),
            installed_names: BTreeSet::new(),
            compiled_files: BTreeMap::new(),
            bin: Rc::new(RefCell::new(BTreeMap::new())),
            processes: BTreeMap::new(),
            ready: VecDeque::new(),
            next_pid: 1,
            clock: None,
            filesystem: None,
            process_files: Rc::new(filesystem::process::Registry::default()),
            random: None,
            network: None,
            dns_server: None,
        }
    }

    pub fn register(&mut self, name: &str, wasm: &[u8]) -> Result<(), Error> {
        if name.is_empty() || name.contains('\0') {
            return Err(Error::InvalidArguments);
        }
        let program = Program::new(wasm)?;
        let previous_path = self.program_files.remove(name);
        let previous = self.programs.insert(name.into(), program);
        if let Err(error) = self.refresh_bin() {
            if let Some(path) = previous_path {
                self.program_files.insert(name.into(), path);
            }
            if let Some(previous) = previous {
                self.programs.insert(name.into(), previous);
            } else {
                self.programs.remove(name);
            }
            return Err(Error::ProgramFile(error));
        }
        Ok(())
    }

    pub fn has_program(&self, name: &str) -> bool {
        let name = name.strip_prefix("/bin/").unwrap_or(name);
        self.programs.contains_key(name)
            || self.program_files.contains_key(name)
            || (Self::ash_applet(name)
                && (self.programs.contains_key("ash") || self.program_files.contains_key("ash")))
    }

    fn load_program(&mut self, name: &str) -> Result<Rc<Program>, Error> {
        use filesystem::{FileError, OpenOptions};
        let name = name.strip_prefix("/bin/").unwrap_or(name);
        if let Some(program) = self.programs.get(name) {
            return Ok(program.clone());
        }
        self.compiled_files
            .retain(|_, cached| cached.program.strong_count() != 0);
        let path = self
            .program_files
            .get(name)
            .ok_or_else(|| Error::UnknownProgram(name.into()))?
            .clone();
        let fs = self
            .filesystem
            .as_ref()
            .expect("Program files require a filesystem");
        let mut file = fs
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
            .map_err(Error::ProgramFile)?;
        let info = file.metadata().map_err(Error::ProgramFile)?;
        if info.size > filesystem::MAX_RESIZE_BYTES {
            return Err(Error::ProgramFile(FileError::FileTooLarge));
        }
        let revision = file
            .content_revision()
            .map_err(Error::ProgramFile)?
            .map(|revision| (info.device, info.inode, revision, info.size));
        if let Some(revision) = revision
            && let Some(cached) = self.compiled_files.get(&path)
            && cached.revision == revision
            && let Some(program) = cached.program.upgrade()
        {
            return Ok(program);
        }
        let mut bytes = vec![0; info.size as usize];
        let mut used = 0;
        while used < bytes.len() {
            let end = (used + io::PIPE_CAPACITY).min(bytes.len());
            let count = file
                .read(&mut bytes[used..end])
                .map_err(Error::ProgramFile)?;
            if count == 0 {
                return Err(Error::ProgramFile(FileError::Io));
            }
            used += count;
        }
        let program = Program::new(&bytes)?;
        if let Some(revision) = revision {
            self.compiled_files.insert(
                path,
                CompiledFile {
                    revision,
                    program: Rc::downgrade(&program),
                },
            );
        }
        Ok(program)
    }

    /// Install the host clock before spawning. A live world's timer epochs must
    /// not change because the host replaced its provider.
    pub fn set_clock(&mut self, clock: Rc<dyn clock::Clock>) {
        assert!(
            self.processes.is_empty(),
            "Cannot replace a live world's clock"
        );
        if let Some(filesystem) = &self.filesystem {
            (filesystem.as_ref() as &dyn core::any::Any)
                .downcast_ref::<filesystem::VirtualFileSystem>()
                .expect("World installs the virtual root filesystem")
                .set_clock(clock.clone());
        }
        self.clock = Some(clock);
    }

    /// Install the random source before spawning any process.
    pub fn set_random(&mut self, random: Rc<dyn random::Random>) {
        assert!(
            self.processes.is_empty(),
            "Cannot replace a live world's random source"
        );
        if let Some(filesystem) = &self.filesystem {
            (filesystem.as_ref() as &dyn core::any::Any)
                .downcast_ref::<filesystem::VirtualFileSystem>()
                .expect("World installs the virtual root filesystem")
                .set_random(random.clone());
        }
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
    pub fn set_filesystem(
        &mut self,
        filesystem: Rc<dyn filesystem::FileSystem>,
    ) -> Result<(), filesystem::MountError> {
        assert!(
            self.processes.is_empty(),
            "Cannot replace a live world's filesystem"
        );
        let filesystem = filesystem::VirtualFileSystem::with_programs(
            filesystem,
            self.bin.clone(),
            self.process_files.clone(),
        )?;
        if let Some(clock) = &self.clock {
            filesystem.set_clock(clock.clone());
        }
        if let Some(random) = &self.random {
            filesystem.set_random(random.clone());
        }
        self.filesystem = Some(filesystem);
        self.compiled_files.clear();
        Ok(())
    }

    /// Share the mounted root with host file tools and module loaders.
    pub fn filesystem(&self) -> Option<Rc<dyn filesystem::FileSystem>> {
        self.filesystem.clone()
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
        self.spawn_child(
            0,
            args,
            Some(environment),
            stdin,
            None,
            SpawnSettings::default(),
        )
    }

    fn spawn_child(
        &mut self,
        parent: Pid,
        args: Vec<String>,
        environment: Option<Vec<String>>,
        stdin: Vec<u8>,
        descriptor_map: Option<Vec<(u32, u32)>>,
        settings: SpawnSettings,
    ) -> Result<Pid, Error> {
        args.first().ok_or(Error::InvalidArguments)?;
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
        let cwd = if let Some(path) = &settings.cwd {
            Some(self.cwd_view(parent, path).map_err(Error::ProgramFile)?)
        } else if parent == 0 {
            self.filesystem.clone()
        } else {
            self.processes[&parent].execution().store.data().cwd.clone()
        };
        let command_cwd = settings.cwd.clone().unwrap_or_else(|| {
            if parent == 0 {
                String::from("/")
            } else {
                format!("/proc/{parent}/cwd")
            }
        });
        let (module_name, args) = self.resolve_command(args, &environment, &command_cwd, parent)?;
        if args
            .iter()
            .chain(&environment)
            .map(|entry| entry.len() + 1)
            .sum::<usize>()
            > 64 * 1024
        {
            return Err(Error::ArgumentsTooLarge);
        }
        let program = self.load_program(&module_name)?;
        let pid = self.next_pid;
        let next_pid = pid
            .checked_add(1)
            .filter(|next| *next <= i32::MAX as u32)
            .ok_or(Error::ProcessLimit)?;
        let (stdin, stdout, stderr, terminal) = if parent == 0 {
            let terminal = Rc::new(io::Terminal::default());
            terminal.input.borrow_mut().bytes = stdin.into();
            (
                terminal.input.clone(),
                Rc::new(RefCell::new(Vec::new())),
                Rc::new(RefCell::new(Vec::new())),
                terminal,
            )
        } else {
            let context = self.processes[&parent].execution().store.data();
            (
                context.stdin.clone(),
                context.stdout.clone(),
                context.stderr.clone(),
                context.terminal.clone(),
            )
        };
        let mut descriptors = if parent == 0 {
            BTreeMap::from([
                (0, io::Descriptor::Input(stdin.clone())),
                (
                    1,
                    io::Descriptor::Output(io::Output {
                        bytes: stdout.clone(),
                        terminal: terminal.clone(),
                    }),
                ),
                (
                    2,
                    io::Descriptor::Output(io::Output {
                        bytes: stderr.clone(),
                        terminal: terminal.clone(),
                    }),
                ),
            ])
        } else {
            // Explicit mappings read the unchanged parent table, so cycles and
            // repeated sources work without temporary guest descriptors.
            let table = &self.processes[&parent].execution().store.data().descriptors;
            let mut inherited = BTreeMap::new();
            let explicit = descriptor_map.is_some();
            for (target, source) in descriptor_map.unwrap_or_else(|| vec![(0, 0), (1, 1), (2, 2)]) {
                if let Some(descriptor) = table.get(&source) {
                    if descriptor.is_private() {
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
        for descriptor in descriptors.values_mut() {
            if let io::Descriptor::Directory(directory) = descriptor
                && let Some(filesystem) = (directory.filesystem.as_ref() as &dyn core::any::Any)
                    .downcast_ref::<filesystem::VirtualFileSystem>()
            {
                directory.filesystem = filesystem.for_process(pid);
            }
        }
        if let Some(filesystem) = &self.filesystem {
            descriptors.insert(
                3,
                io::Descriptor::Directory(filesystem::Directory {
                    filesystem: (filesystem.as_ref() as &dyn core::any::Any)
                        .downcast_ref::<filesystem::VirtualFileSystem>()
                        .expect("World installs the virtual root filesystem")
                        .for_process(pid),
                    path: String::new(),
                    preopen: true,
                    rights: io::DIRECTORY_RIGHTS,
                    inheriting: io::DIRECTORY_RIGHTS | io::FILE_RIGHTS,
                    entries: Rc::new(RefCell::new(None)),
                    flags: 0,
                }),
            );
        }
        let command = args[0].rsplit('/').next().unwrap().to_string();
        let executable = self
            .program_files
            .get(&module_name)
            .map(|path| format!("/{}", path.trim_start_matches('/')))
            .unwrap_or_else(|| format!("/bin/{module_name}"));
        let context = Context {
            pid,
            parent,
            args,
            environment,
            stdin,
            terminal,
            stdout: stdout.clone(),
            stderr: stderr.clone(),
            descriptors: descriptors.into(),
            close_on_exec: BTreeSet::new(),
            creation_mode: None,
            umask: settings.umask.unwrap_or(if parent == 0 {
                0o022
            } else {
                self.processes[&parent].execution().store.data().umask
            }),
            cwd: cwd.map(|view| {
                (view.as_ref() as &dyn core::any::Any)
                    .downcast_ref::<filesystem::VirtualFileSystem>()
                    .expect("World cwd uses the virtual filesystem")
                    .for_process(pid) as Rc<dyn filesystem::FileSystem>
            }),
            clock: self.clock.clone(),
            random: self.random.clone(),
            network: self.network.clone(),
            dns_server: self.dns_server,
            dns_query: None,
            session: settings.session,
            limits: StoreLimitsBuilder::new()
                .memory_size(MAX_MEMORY_BYTES)
                .memories(1)
                .tables(1)
                .table_elements(65536)
                .instances(1)
                .build(),
        };
        let mut store = Store::new(program.module.engine(), context);
        store.limiter(|context| &mut context.limits);
        // Instantiation copies data segments but cannot execute guest code:
        // start sections are rejected by the engine configuration.
        store.set_fuel(u64::MAX)?;
        let instance = program
            .linker
            .instantiate_and_start(&mut store, &program.module)?;
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
                command,
                executable,
                execution: Some(Execution {
                    store,
                    memory,
                    entry,
                    continuation: None,
                    reply: 0,
                    wait_status: 0,
                    control_wait: None,
                    pending_io: None,
                    _program: program,
                }),
            },
        );
        self.ready.push_back(pid);
        self.sync_process_files();
        self.process_files.activity(false);
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
        self.sync_process_files();
        self.process_files.activity(self.ready.is_empty());
        Ok(())
    }

    pub fn reap(&mut self, pid: Pid) -> Result<Status, Error> {
        let status = self.status(pid)?.clone();
        if status.is_live() {
            return Err(Error::NotExited(pid));
        }
        self.processes.remove(&pid);
        self.sync_process_files();
        Ok(status)
    }

    /// Run one queue member until it yields, exhausts its fuel, exits, or traps.
    /// Fuel is a host responsiveness guard; explicit host calls provide cooperation.
    pub fn step(&mut self) -> Option<Pid> {
        self.wake_io();
        self.process_files.activity(self.ready.is_empty());
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
        self.sync_process_files();
        self.process_files.activity(self.ready.is_empty());
        Some(pid)
    }

    fn handle(&mut self, pid: Pid, action: abi::Action) {
        let reply =
            match action {
                abi::Action::Control { fd, bytes, written } => {
                    self.control(pid, fd, &bytes, written);
                    return;
                }
                abi::Action::Yield => 0,
                abi::Action::CancelChild(child) => {
                    if self
                        .processes
                        .get(&child)
                        .is_none_or(|process| process.parent != pid)
                    {
                        -10
                    } else {
                        // Collect before cancellation reparents descendants. Ownership
                        // is checked against the direct child, never a different root.
                        let mut tree = vec![child];
                        let mut index = 0;
                        while index < tree.len() {
                            let parent = tree[index];
                            tree.extend(self.processes.iter().filter_map(|(&id, process)| {
                                (process.parent == parent).then_some(id)
                            }));
                            index += 1;
                        }
                        // Stop parents first so a suspended wait cannot reap a
                        // descendant in the middle of subtree cancellation.
                        for &id in &tree {
                            self.cancel(id).expect("Collected process exists");
                        }
                        for &id in tree.iter().skip(1).rev() {
                            self.reap(id).expect("Cancelled descendant is terminal");
                        }
                        0
                    }
                }
                abi::Action::ProgramExists(name) => i32::from(self.has_program(&name)),
                abi::Action::Spawn {
                    args,
                    environment,
                    descriptors,
                } => {
                    match self.spawn_child(
                        pid,
                        args,
                        environment,
                        Vec::new(),
                        descriptors,
                        SpawnSettings::default(),
                    ) {
                        Ok(child) => child as i32,
                        Err(error) => error.spawn_errno(),
                    }
                }
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
