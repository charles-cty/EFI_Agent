use alloc::{
    format,
    rc::Rc,
    string::{String, ToString},
    vec::Vec,
};
use efi_agent_core::ui::App;
use efi_agent_wasm::{Status, World};

use crate::{files, terminal};

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
    reported: Vec<u32>,
}

impl Runtime {
    pub fn new(
        workspace: &str,
        ipv4: Option<efi_agent_core::config::StaticIpv4>,
        dns: ([u8; 4], u16),
    ) -> Self {
        let mut world = World::new();
        world.set_clock(Rc::new(crate::wasm_clock::FirmwareClock));
        world.set_random(Rc::new(FirmwareRandom));
        world.set_network(Rc::new(crate::tcp::cooperative::FirmwareNetwork(ipv4)));
        world.set_dns_server(dns.0, dns.1);
        world.set_filesystem(Rc::new(crate::wasm_filesystem::FirmwareFileSystem(
            workspace.into(),
        )));
        Self {
            world,
            reported: Vec::new(),
        }
    }

    /// Dispatch one Wasm quantum at APPLICATION between firmware and UI polls.
    pub fn poll(&mut self, app: &mut App) -> bool {
        let progressed = self.world.step().is_some();
        let pids: Vec<_> = self.world.process_ids().collect();
        for pid in pids {
            let (stdout, stderr) = self.world.take_output(pid).expect("Listed process exists");
            if !stdout.is_empty() {
                app.message("wasm", String::from_utf8_lossy(&stdout).into_owned());
            }
            if !stderr.is_empty() {
                app.message("wasm stderr", String::from_utf8_lossy(&stderr).into_owned());
            }
            let status = self.world.status(pid).expect("Listed process exists");
            if !status.is_live() && !self.reported.contains(&pid) {
                app.message("wasm", format!("Process {pid}: {status:?}"));
                self.reported.push(pid);
            }
        }
        self.reported.retain(|pid| self.world.status(*pid).is_ok());
        progressed
    }

    pub fn command(
        &mut self,
        text: &str,
        app: &mut App,
        refresh: &mut impl FnMut(&mut App, bool) -> Result<bool, terminal::Error>,
    ) -> Result<String, terminal::Error> {
        let mut words = text.split_whitespace().skip(1);
        let result = match words.next() {
            Some("load") => match (words.next(), words.next(), words.next()) {
                (Some(name), Some(path), None) => files::read_wasm(path)
                    .and_then(|bytes| {
                        self.world
                            .register(name, &bytes)
                            .map_err(|error| error.to_string())
                    })
                    .map(|()| format!("Loaded {name}")),
                _ => Err("Usage: /wasm load NAME UEFI_PATH".into()),
            },
            Some(mode @ ("start" | "run")) => {
                let args: Vec<String> = words.map(String::from).collect();
                match self.world.spawn(args, Vec::new()) {
                    Ok(pid) => {
                        if mode == "run" {
                            while self.world.status(pid).is_ok_and(Status::is_live) {
                                let messages = app.messages.len();
                                self.poll(app);
                                if app.messages.len() != messages {
                                    refresh(app, false)?;
                                }
                                if refresh(app, true)? {
                                    self.world.cancel(pid).expect("Running process exists");
                                    break;
                                }
                                crate::event_loop::dispatch();
                            }
                            self.poll(app);
                        }
                        Ok(format!(
                            "Process {pid}: {:?}",
                            self.world.status(pid).expect("Root process exists")
                        ))
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
                "/wasm load NAME UEFI_PATH\n/wasm start NAME [ARGS...]\n/wasm run NAME [ARGS...] (Esc cancels)\n/wasm ps\n/wasm kill PID\n/wasm reap PID\nArguments are split on whitespace.",
            )),
        };
        Ok(result.unwrap_or_else(|error| format!("WASM: {error}")))
    }
}
