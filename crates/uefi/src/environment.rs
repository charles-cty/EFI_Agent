use crate::{bridge::Bridge, files};
use alloc::string::String;
use efi_agent_core::{
    agent::{self, Environment},
    config::NativeConfig,
    protocol::{ChatMessage, Operation},
};

pub enum Runtime {
    Vm(Bridge),
    Native { config: NativeConfig, relay: Bridge },
}

impl Runtime {
    pub fn load(vm: bool) -> Result<Self, String> {
        if vm {
            return Ok(Self::Vm(Bridge::vm()));
        }
        let text = files::read("\\EFI\\AGENT\\NATIVE.JSON")?;
        let config: NativeConfig = serde_json::from_str(&text)
            .map_err(|_| String::from("Invalid EFI/AGENT/NATIVE.JSON configuration"))?;
        config.validate()?;
        let relay = Bridge::new(config.relay_address, config.relay_port);
        Ok(Self::Native { config, relay })
    }

    pub fn host(
        &mut self,
        operation: Operation,
        poll: &mut dyn FnMut() -> bool,
    ) -> Result<String, String> {
        match self {
            Self::Vm(bridge) => bridge.call(operation, poll),
            Self::Native { .. } => Err(String::from(
                "Host commands are only available in VM mode; native agent tools use the UEFI workspace",
            )),
        }
    }
}

pub struct Interactive<'a> {
    pub runtime: &'a mut Runtime,
    pub poll: &'a mut dyn FnMut() -> bool,
    pub cancelled: bool,
}

impl Interactive<'_> {
    fn check(&mut self) -> bool {
        self.cancelled |= (self.poll)();
        self.cancelled
    }
}

impl Environment for Interactive<'_> {
    fn cancelled(&self) -> bool {
        self.cancelled
    }

    fn complete(&mut self, messages: &[ChatMessage]) -> Result<ChatMessage, String> {
        if self.check() {
            return Err("Request cancelled".into());
        }
        let cancelled = &mut self.cancelled;
        let poll = &mut self.poll;
        let mut control = || {
            *cancelled |= poll();
            *cancelled
        };
        match self.runtime {
            Runtime::Vm(bridge) => bridge.complete(messages, &mut control),
            Runtime::Native { relay, .. } => relay.complete(messages, &mut control),
        }
    }
    fn execute(&mut self, operation: Operation) -> Result<String, String> {
        if self.check() {
            return Err("Request cancelled; operation was not executed".into());
        }
        let cancelled = &mut self.cancelled;
        let poll = &mut self.poll;
        let mut control = || {
            *cancelled |= poll();
            *cancelled
        };
        match self.runtime {
            Runtime::Vm(bridge) => bridge.call(operation, &mut control),
            Runtime::Native { config, .. } => match operation {
                Operation::Read { path } | Operation::List { path } => {
                    files::read_or_list(&config.resolve(&path)?)
                }
                Operation::Write { path, content } => {
                    files::write(&config.resolve(&path)?, &content)?;
                    Ok("File saved".into())
                }
                Operation::Edit {
                    path,
                    old_text,
                    new_text,
                } => {
                    let path = config.resolve(&path)?;
                    let text = files::read(&path)?;
                    let edited = agent::edit_text(text, &old_text, &new_text)?;
                    files::write(&path, &edited)?;
                    Ok("File edited".into())
                }
                Operation::Complete { .. } => {
                    Err(String::from("Model requests are not file tools"))
                }
            },
        }
    }
}
