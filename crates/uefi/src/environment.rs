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

    pub fn host(&mut self, operation: Operation) -> Result<String, String> {
        match self {
            Self::Vm(bridge) => bridge.call(operation),
            Self::Native { .. } => Err(String::from(
                "Host commands are only available in VM mode; native agent tools use the UEFI workspace",
            )),
        }
    }
}

impl Environment for Runtime {
    fn complete(&mut self, messages: &[ChatMessage]) -> Result<ChatMessage, String> {
        match self {
            Self::Vm(bridge) => bridge.complete(messages),
            Self::Native { relay, .. } => relay.complete(messages),
        }
    }
    fn execute(&mut self, operation: Operation) -> Result<String, String> {
        match self {
            Self::Vm(bridge) => bridge.execute(operation),
            Self::Native { config, .. } => match operation {
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
