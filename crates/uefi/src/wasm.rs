use alloc::{
    collections::{BTreeMap, BTreeSet},
    format,
    rc::Rc,
    string::{String, ToString},
    vec::Vec,
};
use efi_agent_core::ui::App;
use efi_agent_core::wasm_output::WasmOutput;
use efi_agent_filesystem::{FileSystem, namespace::Namespace};
use efi_agent_wasm::{Status, World};

use crate::terminal;

struct ToolOutput {
    capture: Option<efi_agent_filesystem::capture::Capture>,
}

impl ToolOutput {
    fn new(filesystem: &dyn FileSystem, capture: bool) -> Result<Self, String> {
        Ok(Self {
            capture: if capture {
                Some(
                    efi_agent_filesystem::capture::Capture::new(filesystem)
                        .map_err(|error| format!("Create output logs: {error:?}"))?,
                )
            } else {
                None
            },
        })
    }

    fn append(&mut self, stdout: &[u8], stderr: &[u8]) -> Result<bool, String> {
        if let Some(capture) = &mut self.capture {
            capture.append(stdout, stderr).map_err(|error| {
                format!("Output log storage failed: {error:?}; output may be incomplete")
            })?;
        }
        Ok(!stdout.is_empty() || !stderr.is_empty())
    }

    fn result(&self, status: &Status) -> String {
        let status = match status {
            Status::Exited(code) => format!("Exit code: {code}"),
            Status::Trapped(error) => format!("Trap: {error}"),
            _ => unreachable!("Tool result requires a terminal process"),
        };
        self.contents(&status)
    }

    fn contents(&self, status: &str) -> String {
        self.capture
            .as_ref()
            .map_or_else(|| status.into(), |capture| capture.result(status))
    }

    fn flush(&mut self) -> Result<(), efi_agent_filesystem::FileError> {
        self.capture
            .as_mut()
            .map_or(Ok(()), |capture| capture.flush())
    }
}

struct Task {
    input: efi_agent_wasm::ProcessInput,
    output: ToolOutput,
    status: Option<Status>,
    error: Option<String>,
}

fn module_name(path: &str) -> Result<&str, String> {
    let filename = path
        .rsplit(['/', '\\'])
        .next()
        .expect("Path has a final part");
    let name = match filename.rsplit_once('.') {
        Some((stem, extension)) if extension.eq_ignore_ascii_case("wasm") => stem,
        _ => filename,
    };
    if name.is_empty() || matches!(name, "." | "..") {
        return Err("Module path must include a program name".into());
    }
    Ok(name)
}

struct FirmwareRandom;
impl efi_agent_wasm::random::Random for FirmwareRandom {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), efi_agent_wasm::random::RandomError> {
        crate::random::fill(bytes).map_err(|failure| match failure {
            efi_agent_core::random::Error::Unsupported
            | efi_agent_core::random::Error::Unavailable => {
                efi_agent_wasm::random::RandomError::Unavailable
            }
            efi_agent_core::random::Error::Busy => efi_agent_wasm::random::RandomError::Io,
        })
    }
}

pub struct Runtime {
    world: World,
    clock: Rc<crate::wasm_clock::FirmwareClock>,
    filesystem: Rc<Namespace>,
    outputs: BTreeMap<u32, WasmOutput>,
    roots: BTreeMap<u32, u32>,
    cancelled: BTreeSet<u32>,
    output_changed: bool,
    tool_outputs: BTreeMap<u32, ToolOutput>,
    sessions: [Option<efi_agent_wasm::session::Session>; 2],
    tasks: BTreeMap<u32, Task>,
    capture_error: Option<String>,
}

impl Runtime {
    pub fn new(
        workspace: &str,
        ipv4: Option<efi_agent_core::config::StaticIpv4>,
        dns: ([u8; 4], u16),
        environment: Vec<String>,
    ) -> Result<Self, String> {
        use efi_agent_wasm::clock::{Clock, ClockId};
        let clock = Rc::new(crate::wasm_clock::FirmwareClock::new());
        let file_clock = clock.clone();
        let filesystem = Rc::new(
            Namespace::open(
                Rc::new(crate::wasm_filesystem::FirmwareFileSystem(workspace.into())),
                Some(Rc::new(move || {
                    file_clock
                        .now(ClockId::Realtime)
                        .map_err(|_| efi_agent_filesystem::FileError::Io)
                })),
            )
            .map_err(|error| format!("Initialize workspace: {error:?}"))?,
        );
        let mut world = World::new();
        world
            .set_default_environment(environment)
            .map_err(|error| error.to_string())?;
        world.set_clock(clock.clone());
        world.set_random(Rc::new(FirmwareRandom));
        world.set_network(Rc::new(crate::tcp::cooperative::FirmwareNetwork(ipv4)));
        world.set_dns_server(dns.0, dns.1);
        world
            .set_filesystem(filesystem.clone())
            .map_err(|error| error.to_string())?;
        Ok(Self {
            world,
            clock,
            filesystem,
            outputs: BTreeMap::new(),
            roots: BTreeMap::new(),
            cancelled: BTreeSet::new(),
            output_changed: false,
            tool_outputs: BTreeMap::new(),
            sessions: [None, None],
            tasks: BTreeMap::new(),
            capture_error: None,
        })
    }

    pub fn filesystem(&self) -> Rc<dyn FileSystem> {
        self.world
            .filesystem()
            .expect("Runtime installs the mounted root")
    }

    pub fn clock_description(&self) -> String {
        self.clock.description()
    }

    /// Report deferred close errors as well as explicit metadata flush errors.
    pub fn flush_workspace(&self) -> Result<(), String> {
        let flush = self.filesystem.flush();
        let mut errors = self.filesystem.take_errors();
        if let Err(error) = flush {
            errors.push(error);
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(format!("Workspace flush/close failed: {errors:?}"))
        }
    }

    /// Dispatch one Wasm quantum at APPLICATION between firmware and UI polls.
    pub fn poll(&mut self, app: &mut App) -> bool {
        let progressed = self.world.step().is_some();
        self.output_changed = false;
        for (index, session) in self.sessions.iter().enumerate() {
            if let Some(session) = session {
                let mode = session.terminal_mode();
                let revision = app.terminals[index].revision;
                if mode & 1 == 0 {
                    app.terminals[index].set_mode(mode);
                }
                let bytes = session.take_terminal_output();
                if !bytes.is_empty() {
                    app.terminals[index].output(&bytes);
                    self.output_changed = true;
                }
                if mode & 1 != 0 {
                    app.terminals[index].set_mode(mode);
                }
                self.output_changed |= app.terminals[index].revision != revision;
                let (columns, rows) = app.terminals[index].dimensions;
                session.set_terminal_size(columns.max(1), rows.max(1));
            }
        }
        let pids: Vec<_> = self.world.process_ids().collect();
        // Retain each child's root even after its parent exits or is reaped.
        for &pid in &pids {
            if !self.roots.contains_key(&pid) {
                let root = self.world.root(pid).expect("Listed process exists");
                self.roots.insert(pid, root);
            }
        }
        for pid in pids {
            let (stdout, stderr) = self.world.take_output(pid).expect("Listed process exists");
            let root = self.roots[&pid];
            if let Some(task) = self.tasks.get_mut(&root) {
                let terminal = task.input.take_output();
                if let Err(error) = task
                    .output
                    .append(&stdout, &stderr)
                    .and_then(|_| task.output.append(&terminal, &[]))
                {
                    task.error = Some(error);
                    let owned: Vec<_> = self
                        .world
                        .process_ids()
                        .filter(|pid| self.world.root(*pid).ok() == Some(root))
                        .collect();
                    let _ = self.world.cancel(root);
                    for child in owned {
                        let _ = self.world.cancel(child);
                    }
                }
                continue;
            }
            if let Some(output) = self.tool_outputs.get_mut(&root) {
                match output.append(&stdout, &stderr) {
                    Ok(changed) => self.output_changed |= changed,
                    Err(error) => self.capture_error = Some(error),
                }
                continue;
            }
            let status = match self.world.status(root) {
                Ok(Status::Exited(0)) => String::from("Completed (exit code 0)"),
                Ok(Status::Exited(code)) if self.cancelled.contains(&root) => {
                    format!("Cancelled (exit code {code})")
                }
                Ok(Status::Exited(code)) => format!("Failed (exit code {code})"),
                Ok(Status::Trapped(error)) => format!("Failed: {error}"),
                Ok(_) => String::from("Running"),
                Err(_) => String::from(self.outputs[&root].status()),
            };
            if pid == root || !stdout.is_empty() || !stderr.is_empty() {
                self.output_changed |= self
                    .outputs
                    .get_mut(&root)
                    .expect("Root has an output panel")
                    .update(
                        app,
                        &String::from_utf8_lossy(&stdout),
                        &String::from_utf8_lossy(&stderr),
                        &status,
                    );
            }
        }
        self.roots.retain(|pid, _| self.world.status(*pid).is_ok());
        self.outputs
            .retain(|root, _| self.roots.values().any(|pid| pid == root));
        self.cancelled
            .retain(|root| self.outputs.contains_key(root));
        // A task completes only after its entire process tree has stopped.
        let completed: Vec<_> = self
            .tasks
            .iter()
            .filter_map(|(&root, task)| {
                (task.status.is_none()
                    && self
                        .world
                        .process_ids()
                        .filter(|pid| self.world.root(*pid).ok() == Some(root))
                        .all(|pid| !self.world.status(pid).unwrap().is_live()))
                .then_some(root)
            })
            .collect();
        for root in completed {
            let status = self
                .world
                .status(root)
                .expect("Owned task root exists")
                .clone();
            let task = self.tasks.get_mut(&root).unwrap();
            task.status = Some(status);
            if let Err(error) = task.output.flush() {
                task.error = Some(format!("Flush output logs: {error:?}"));
            }
            let pids: Vec<_> = self
                .world
                .process_ids()
                .filter(|pid| self.world.root(*pid).ok() == Some(root))
                .collect();
            for pid in pids {
                let _ = self.world.reap(pid);
                self.roots.remove(&pid);
            }
        }
        progressed
    }

    pub fn output_changed(&self) -> bool {
        self.output_changed
    }

    fn ensure_program(&mut self, name: &str) -> Result<(), String> {
        if !self.world.has_program(name) {
            let path = format!("{name}.wasm");
            let bytes = efi_agent_filesystem::text::read_bytes(
                self.filesystem().as_ref(),
                &path,
                48 * 1024 * 1024,
            )
            .map_err(|error| format!("Cannot load {path}: {error}"))?;
            self.world
                .register(name, &bytes)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    /// Execute model code without adding a second UI block to the tool result.
    pub fn run_tool(
        &mut self,
        kind: efi_agent_wasm::session::SessionKind,
        source: &str,
        app: &mut App,
        refresh: &mut impl FnMut(&mut App, bool) -> Result<bool, String>,
    ) -> Result<String, String> {
        self.run_code(kind, Some(source), false, app, refresh)
    }

    pub fn run_repl(
        &mut self,
        kind: efi_agent_wasm::session::SessionKind,
        source: Option<&str>,
        app: &mut App,
        refresh: &mut impl FnMut(&mut App, bool) -> Result<bool, String>,
    ) -> Result<(), String> {
        let notice = self.run_code(kind, source, true, app, refresh)?;
        if !notice.is_empty() {
            app.terminals[kind.slot()].append(&notice);
        }
        Ok(())
    }

    pub fn session_active(&self, kind: efi_agent_wasm::session::SessionKind) -> bool {
        self.sessions[kind.slot()]
            .as_ref()
            .is_some_and(|session| self.world.status(session.pid()).is_ok_and(Status::is_live))
    }

    fn run_code(
        &mut self,
        kind: efi_agent_wasm::session::SessionKind,
        source: Option<&str>,
        terminal: bool,
        app: &mut App,
        refresh: &mut impl FnMut(&mut App, bool) -> Result<bool, String>,
    ) -> Result<String, String> {
        if source.is_some_and(|source| source.contains('\0') || source.len() > 60 * 1024) {
            return Err("Code must contain no NUL and must be at most 60 KiB".into());
        }
        self.world
            .discover_programs()
            .map_err(|error| format!("Discover workspace programs: {error}"))?;
        self.ensure_program("ash")?;
        if matches!(kind, efi_agent_wasm::session::SessionKind::JavaScript) {
            self.ensure_program("quickjs")?;
        }
        let index = kind.slot();
        let mut notice = String::new();
        if let Some(session) = &self.sessions[index]
            && !self.world.status(session.pid()).is_ok_and(Status::is_live)
        {
            let pid = session.pid();
            self.stop_tool_tree(pid, false, app);
            if self.world.status(pid).is_ok() {
                self.world.reap(pid).expect("Session is terminal");
            }
            self.roots.remove(&pid);
            self.sessions[index] = None;
            self.tool_outputs.remove(&pid);
            notice.push_str("Previous session ended; started a new session.\n");
        }
        // Prepare fallible storage before creating a process, then register
        // output ownership before submitting source to the interpreter.
        let capture = ToolOutput::new(self.filesystem.as_ref(), !terminal)?;
        if self.sessions[index].is_none() {
            self.sessions[index] = Some(
                self.world
                    .start_session(kind)
                    .map_err(|error| error.to_string())?,
            );
        }
        let session = self.sessions[index].as_mut().unwrap();
        let pid = session.pid();
        self.roots.insert(pid, pid);
        self.tool_outputs.insert(pid, capture);
        match source {
            None => session.close_source(),
            Some(source) if terminal => session.submit_terminal(source),
            Some(source) => session.submit(source),
        }
        .map_err(|error| error.to_string())?;
        if terminal {
            let (columns, rows) = app.terminals[index].dimensions;
            session.set_terminal_size(columns.max(1), rows.max(1));
            app.terminals[index].set_mode(7);
        }
        let execution = (|| {
            loop {
                if let Some(status) = self.sessions[index].as_mut().unwrap().take_completion() {
                    return Ok(Some(status));
                }
                if !self
                    .world
                    .status(pid)
                    .expect("Tool process exists")
                    .is_live()
                {
                    return Ok(None);
                }
                self.poll(app);
                if let Some(error) = self.capture_error.take() {
                    return Err(error);
                }
                if terminal && self.output_changed {
                    refresh(app, false)?;
                }
                if refresh(app, true)? {
                    return Err(String::from("Request cancelled"));
                }
                if terminal {
                    while let Some(input) = app.terminals[index].input.pop_front() {
                        let session = self.sessions[index].as_ref().unwrap();
                        match input {
                            efi_agent_core::repl::Input::Bytes(text) => {
                                if session.write_stdin(text.as_bytes()).is_err() {
                                    app.terminals[index]
                                        .append("[stdin is closed or its 64 KiB queue is full]\n");
                                }
                            }
                            efi_agent_core::repl::Input::Eof => session.close_stdin(),
                        }
                    }
                }
                crate::event_loop::dispatch();
            }
        })();
        let retained =
            matches!(execution, Ok(Some(_))) && self.world.status(pid).is_ok_and(Status::is_live);
        let stopped = self.stop_tool_tree(pid, retained, app);
        let status = match &execution {
            Ok(Some(code)) => Status::Exited(*code),
            _ => self.world.status(pid).expect("Tool root exists").clone(),
        };
        let mut capture = self.tool_outputs.remove(&pid).expect("Tool capture exists");
        let capture_flush = capture
            .flush()
            .map_err(|error| format!("Flush output logs: {error:?}"));
        self.sessions[index].as_ref().unwrap().detach_terminal();
        if terminal {
            app.terminals[index].set_mode(7);
        }
        let mut output = if terminal {
            match &status {
                Status::Trapped(error) => format!("Trap: {error}\n"),
                _ => String::new(),
            }
        } else {
            notice + &capture.result(&status)
        };
        if retained {
            // Keep the completed log until the next evaluation starts.
            self.tool_outputs.insert(pid, capture);
        } else {
            self.sessions[index] = None;
            self.world.reap(pid).expect("Tool root is terminal");
            self.roots.remove(&pid);
            if !terminal {
                output.push_str("\n\nSession reset. The next call starts a new session.");
            }
        }
        if stopped != 0 {
            output.push_str(&format!(
                "\nStopped {stopped} unfinished child process(es)."
            ));
        }
        let storage = capture_flush.and_then(|()| self.flush_workspace());
        match (execution, storage) {
            (Ok(_), Ok(())) => Ok(output),
            (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(format!("{output}\n{error}")),
            (Err(execution), Err(storage)) => Err(format!("{output}\n{execution}\n{storage}")),
        }
    }

    /// Stop only this tool tree; the retained interpreter and unrelated roots
    /// remain under their respective owners.
    fn stop_tool_tree(&mut self, pid: u32, keep_root: bool, app: &mut App) -> usize {
        let pids: Vec<_> = self
            .world
            .process_ids()
            .filter(|child| {
                *child != pid && self.world.root(*child).expect("Listed process exists") == pid
            })
            .collect();
        let mut stopped = 0;
        // Stop the root before children. A live root waiting for a child would
        // otherwise reap that child during cancellation, invalidating this list.
        if !keep_root && self.world.status(pid).is_ok() {
            self.world.cancel(pid).expect("Tool root exists");
        }
        for &child in &pids {
            stopped += usize::from(self.world.status(child).unwrap().is_live());
            self.world.cancel(child).expect("Tool process exists");
        }
        self.poll(app);
        for child in pids {
            self.world.reap(child).expect("Tool process is terminal");
            self.roots.remove(&child);
        }
        stopped
    }

    pub fn clear_sessions(&mut self, app: &mut App) -> Result<(), String> {
        let owned: Vec<_> = self
            .world
            .process_ids()
            .filter(|pid| {
                self.world
                    .root(*pid)
                    .is_ok_and(|root| self.tasks.contains_key(&root))
            })
            .collect();
        for root in self.tasks.keys() {
            let _ = self.world.cancel(*root);
        }
        for pid in owned {
            let _ = self.world.cancel(pid);
        }
        self.poll(app);
        self.tasks.clear();
        for index in 0..self.sessions.len() {
            self.reset_session(index, app);
        }
        self.flush_workspace()
    }

    pub fn task(
        &mut self,
        request: efi_agent_core::protocol::TaskRequest,
        app: &mut App,
        refresh: &mut impl FnMut(&mut App, bool) -> Result<bool, String>,
    ) -> Result<String, String> {
        if request.wait_ms > 10_000 {
            return Err("Use wait_ms <= 10000 and poll again for longer waits".into());
        }
        if request
            .input
            .as_ref()
            .is_some_and(|input| input.len() > 65536)
        {
            return Err("Task input exceeds its 64 KiB queue; send smaller chunks".into());
        }
        let id = if request.action == "start" {
            let command = request
                .command
                .as_ref()
                .ok_or("task start requires command")?;
            self.world
                .discover_programs()
                .map_err(|error| error.to_string())?;
            self.ensure_program("ash")?;
            let output = ToolOutput::new(self.filesystem.as_ref(), true)?;
            let id = self
                .world
                .spawn(
                    alloc::vec!["ash".into(), "-c".into(), command.clone()],
                    Vec::new(),
                )
                .map_err(|error| error.to_string())?;
            let input = self
                .world
                .process_input(id, request.terminal)
                .map_err(|error| error.to_string())?;
            self.roots.insert(id, id);
            self.tasks.insert(
                id,
                Task {
                    input,
                    output,
                    status: None,
                    error: None,
                },
            );
            id
        } else if request.action == "list" {
            self.poll(app);
            return Ok(self
                .tasks
                .iter()
                .map(|(id, task)| {
                    format!(
                        "{id}: {}\n",
                        task.status
                            .as_ref()
                            .map_or_else(|| "Running".into(), |status| format!("{status:?}"))
                    )
                })
                .collect());
        } else {
            let id = request.id.ok_or("task action requires id")?;
            if !self.tasks.contains_key(&id) {
                return Err("Unknown task id".into());
            }
            match request.action.as_str() {
                "poll" | "input" => (),
                "cancel" => {
                    let pids: Vec<_> = self
                        .world
                        .process_ids()
                        .filter(|pid| self.world.root(*pid).ok() == Some(id))
                        .collect();
                    // Stop the parent before children to prevent wait/reap races.
                    if self.tasks[&id].status.is_none() {
                        self.world.cancel(id).map_err(|error| error.to_string())?;
                    }
                    for pid in pids {
                        let _ = self.world.cancel(pid);
                    }
                }
                "forget" => {
                    if self.tasks[&id].status.is_none() {
                        return Err("Cancel or finish the task before forgetting it".into());
                    }
                    self.tasks.remove(&id);
                    return Ok("Task handle released. Output files remain available.".into());
                }
                _ => {
                    return Err(
                        "Task action must be start, poll, input, cancel, list, or forget".into(),
                    );
                }
            }
            id
        };
        let task = &self.tasks[&id];
        if let Some(input) = request.input {
            if task.status.is_some() {
                return Err("Task has already finished".into());
            }
            task.input.write(input.as_bytes()).map_err(|_| "Task stdin is closed or its input queue is full; poll and send a smaller chunk")?;
        }
        if request.eof {
            task.input.close();
        }
        let deadline =
            crate::tcp::Deadline::new(core::time::Duration::from_millis(request.wait_ms.max(1)))?;
        loop {
            self.poll(app);
            if self.tasks[&id].status.is_some() || request.wait_ms == 0 || deadline.expired() {
                break;
            }
            if refresh(app, true)? {
                return Err(format!(
                    "Wait cancelled. Task {id} remains available; use task cancel to stop it."
                ));
            }
            crate::event_loop::idle(core::time::Duration::from_millis(1))?;
        }
        let task = &mut self.tasks.get_mut(&id).unwrap();
        task.output
            .flush()
            .map_err(|error| format!("Flush task output: {error:?}"))?;
        let status = format!(
            "Task id: {id}\nStatus: {}{}",
            task.status
                .as_ref()
                .map_or_else(|| "Running".into(), |status| format!("{status:?}")),
            task.error
                .as_ref()
                .map_or_else(String::new, |error| format!("\n{error}"))
        );
        Ok(task.output.contents(&status))
    }

    fn reset_session(&mut self, index: usize, app: &mut App) {
        if let Some(session) = self.sessions[index].take() {
            let pid = session.pid();
            self.stop_tool_tree(pid, false, app);
            self.tool_outputs.remove(&pid);
            if self.world.status(pid).is_ok() {
                self.world.reap(pid).expect("Session is terminal");
            }
            self.roots.remove(&pid);
        }
    }

    pub fn reset_js(&mut self, app: &mut App) -> Result<String, String> {
        self.reset_session(efi_agent_wasm::session::SessionKind::JavaScript.slot(), app);
        self.flush_workspace()?;
        Ok("JavaScript session reset. The next js call starts a new session. Shell state and files are preserved.".into())
    }

    pub fn command(
        &mut self,
        text: &str,
        app: &mut App,
        refresh: &mut impl FnMut(&mut App, bool) -> Result<bool, terminal::Error>,
    ) -> Result<String, terminal::Error> {
        let mut words = text.split_whitespace().skip(1);
        let result = match words.next() {
            Some("load") => {
                let module = match (words.next(), words.next(), words.next()) {
                    (Some(path), None, None) => module_name(path).map(|name| (name, path)),
                    (Some(name), Some(path), None) => Ok((name, path)),
                    _ => Err("Usage: /wasm load PATH or /wasm load NAME PATH".into()),
                };
                module.and_then(|(name, path)| {
                    efi_agent_filesystem::text::read_bytes(
                        self.filesystem().as_ref(),
                        path,
                        48 * 1024 * 1024,
                    )
                    .and_then(|bytes| {
                        self.world
                            .register(name, &bytes)
                            .map_err(|error| error.to_string())
                    })
                    .map(|()| format!("Loaded {name}"))
                })
            }
            Some(mode @ ("start" | "run")) => {
                let args: Vec<String> = words.map(String::from).collect();
                let command = args.join(" ");
                match self.world.spawn(args, Vec::new()) {
                    Ok(pid) => {
                        self.roots.insert(pid, pid);
                        let mut output = WasmOutput::new(app, pid, command);
                        output.update(app, "", "", "Running");
                        self.outputs.insert(pid, output);
                        refresh(app, false)?;
                        if mode == "run" {
                            while self.world.status(pid).is_ok_and(Status::is_live) {
                                self.poll(app);
                                if self.output_changed {
                                    refresh(app, false)?;
                                }
                                if refresh(app, true)? {
                                    self.cancelled.insert(pid);
                                    self.world.cancel(pid).expect("Running process exists");
                                    break;
                                }
                                crate::event_loop::dispatch();
                            }
                            self.poll(app);
                        }
                        Ok(String::new())
                    }
                    Err(error) => Err(error.to_string()),
                }
            }
            Some("ps") if words.next().is_none() => Ok(self
                .world
                .process_ids()
                .map(|pid| {
                    format!(
                        "{pid}: {:?}\n",
                        self.world.status(pid).expect("Listed process exists")
                    )
                })
                .collect()),
            Some(operation @ ("kill" | "reap")) => {
                match (
                    words.next().and_then(|word| word.parse::<u32>().ok()),
                    words.next(),
                ) {
                    (Some(pid), None) => if operation == "kill" {
                        self.cancelled.insert(pid);
                        self.world.cancel(pid).map(|()| format!("Cancelled {pid}"))
                    } else {
                        self.world
                            .reap(pid)
                            .map(|status| format!("Reaped {pid}: {status:?}"))
                    }
                    .map_err(|error| error.to_string()),
                    _ => Err(format!("Usage: /wasm {operation} PID")),
                }
            }
            _ => Ok(String::from(
                "/wasm load PATH (name from filename)\n/wasm load NAME PATH (explicit name)\n/wasm start NAME [ARGS...]\n/wasm run NAME [ARGS...] (Esc cancels)\n/wasm ps\n/wasm kill PID\n/wasm reap PID\nModule paths use the workspace. / or \\ starts at the workspace root.\nArguments are split on whitespace.",
            )),
        };
        Ok(result.unwrap_or_else(|error| format!("WASM: {error}")))
    }
}
