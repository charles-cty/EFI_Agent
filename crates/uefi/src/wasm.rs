use alloc::{
    collections::{BTreeMap, BTreeSet},
    format,
    rc::Rc,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use efi_agent_core::ui::App;
use efi_agent_core::wasm_output::WasmOutput;
use efi_agent_filesystem::{FileSystem, namespace::Namespace};
use efi_agent_wasm::{Status, World};

use crate::terminal;

const TOOL_STREAM_LIMIT: usize = 64 * 1024;

#[derive(Default)]
struct ToolOutput {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    truncated: [bool; 2],
}

impl ToolOutput {
    fn append(&mut self, stdout: &[u8], stderr: &[u8]) {
        for (index, (target, bytes)) in [(&mut self.stdout, stdout), (&mut self.stderr, stderr)]
            .into_iter()
            .enumerate()
        {
            let count = bytes.len().min(TOOL_STREAM_LIMIT - target.len());
            target.extend_from_slice(&bytes[..count]);
            self.truncated[index] |= count < bytes.len();
        }
    }

    fn result(&self, status: &Status) -> String {
        let status = match status {
            Status::Exited(code) => format!("Exit code: {code}"),
            Status::Trapped(error) => format!("Trap: {error}"),
            _ => unreachable!("Tool result requires a terminal process"),
        };
        let mut result = status;
        for (index, (name, bytes)) in [
            ("Standard output", &self.stdout),
            ("Standard error", &self.stderr),
        ]
        .into_iter()
        .enumerate()
        {
            result.push_str(&format!("\n\n{name}:\n{}", String::from_utf8_lossy(bytes)));
            if self.truncated[index] {
                result.push_str("\n[Output truncated at 64 KiB]");
            }
        }
        result
    }
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
}

impl Runtime {
    pub fn new(
        workspace: &str,
        ipv4: Option<efi_agent_core::config::StaticIpv4>,
        dns: ([u8; 4], u16),
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
            if let Some(output) = self.tool_outputs.get_mut(&root) {
                output.append(&stdout, &stderr);
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
        progressed
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
        python: bool,
        source: &str,
        app: &mut App,
        refresh: &mut impl FnMut(&mut App, bool) -> Result<bool, String>,
    ) -> Result<String, String> {
        if source.contains('\0') || source.len() > 60 * 1024 {
            return Err("Code must contain no NUL and must be at most 60 KiB".into());
        }
        self.world
            .discover_programs()
            .map_err(|error| format!("Discover workspace programs: {error}"))?;
        self.ensure_program("ash")?;
        if python {
            self.ensure_program("python")?;
        }
        let program = if python { "python" } else { "ash" };
        let pid = self
            .world
            .spawn(vec![program.into(), "-c".into(), source.into()], Vec::new())
            .map_err(|error| error.to_string())?;
        self.roots.insert(pid, pid);
        self.tool_outputs.insert(pid, ToolOutput::default());
        let execution = (|| {
            while self
                .world
                .status(pid)
                .expect("Tool process exists")
                .is_live()
            {
                self.poll(app);
                if refresh(app, true)? {
                    return Err(String::from("Request cancelled"));
                }
                crate::event_loop::dispatch();
            }
            Ok(())
        })();
        // Terminate descendants as well, including children adopted by PID 0.
        // Unrelated background slash-command roots remain active.
        let pids: Vec<_> = self
            .world
            .process_ids()
            .filter(|child| self.world.root(*child).expect("Listed process exists") == pid)
            .collect();
        for &child in &pids {
            self.world.cancel(child).expect("Tool process exists");
        }
        self.poll(app);
        let status = self.world.status(pid).expect("Tool root exists");
        let output = self
            .tool_outputs
            .remove(&pid)
            .expect("Tool capture exists")
            .result(status);
        for child in pids {
            self.world.reap(child).expect("Tool process is terminal");
            self.roots.remove(&child);
        }
        let storage = self.flush_workspace();
        match (execution, storage) {
            (Ok(()), Ok(())) => Ok(output),
            (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(format!("{output}\n{error}")),
            (Err(execution), Err(storage)) => Err(format!("{output}\n{execution}\n{storage}")),
        }
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
                "/wasm load PATH (name from filename)\n/wasm load NAME PATH (explicit name)\n/wasm start NAME [ARGS...]\n/wasm run NAME [ARGS...] (Esc cancels)\n/wasm ps\n/wasm kill PID\n/wasm reap PID\nRelative module paths use the workspace. / or \\ starts at the boot volume root.\nArguments are split on whitespace.",
            )),
        };
        Ok(result.unwrap_or_else(|error| format!("WASM: {error}")))
    }
}
