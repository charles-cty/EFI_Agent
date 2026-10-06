//! The firmware owns provider requests and boot-volume file operations.
use crate::{dns, files, tls};
use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use efi_agent_core::{
    agent::{self, Environment},
    config::AgentConfig,
    http::{Body, Url},
    model::{self, Read},
    protocol::{ChatMessage, Operation},
};
use serde_json::Value;

pub struct Runtime {
    pub config: AgentConfig,
    attempts: u64,
    last_usage: Option<Value>,
    totals: [u64; 5],
    reports: [u64; 5],
}
const USAGE_FIELDS: [(&str, &str); 5] = [
    ("Input", "/prompt_tokens"),
    ("Output (includes reasoning)", "/completion_tokens"),
    ("Total", "/total_tokens"),
    ("Cached input", "/prompt_tokens_details/cached_tokens"),
    ("Reasoning", "/completion_tokens_details/reasoning_tokens"),
];
impl Runtime {
    pub fn load(_vm: bool) -> Result<Self, String> {
        let text = files::read("\\EFI\\AGENT\\CONFIG.JSON")?;
        let config: AgentConfig =
            serde_json::from_str(&text).map_err(|_| "Invalid EFI/AGENT/CONFIG.JSON")?;
        config.validate()?;
        Ok(Self {
            config,
            attempts: 0,
            last_usage: None,
            totals: [0; 5],
            reports: [0; 5],
        })
    }
    pub fn control(&mut self, operation: Operation) -> Result<String, String> {
        match operation {
            Operation::Status => {
                let mut status = format!(
                    "API: {}\nFormat: {}\nModel: {}\nAPI key: configured\nReasoning effort: {}\nDirect firmware requests: {}\nStatistics scope: this firmware session\n",
                    self.config.api_base,
                    self.config.api_format,
                    self.config.model,
                    self.config.reasoning_effort,
                    self.attempts
                );
                for (index, (label, path)) in USAGE_FIELDS.iter().enumerate() {
                    let last = self
                        .last_usage
                        .as_ref()
                        .and_then(|value| value.pointer(path))
                        .and_then(Value::as_u64)
                        .map_or_else(|| "unavailable".into(), |n| n.to_string());
                    let total = if self.reports[index] == 0 {
                        "unavailable".into()
                    } else {
                        self.totals[index].to_string()
                    };
                    status.push_str(&format!(
                        "{label} tokens: {last}; reported subtotal: {total} ({} requests)\n",
                        self.reports[index]
                    ));
                }
                let ratio = self
                    .last_usage
                    .as_ref()
                    .and_then(|v| {
                        let input = v["prompt_tokens"].as_u64()?;
                        let cached = v["prompt_tokens_details"]["cached_tokens"].as_u64()?;
                        (input > 0 && cached <= input)
                            .then(|| format!("{:.2}%", cached as f64 * 100.0 / input as f64))
                    })
                    .unwrap_or_else(|| "unavailable".into());
                status.push_str(&format!("Last cached input / input: {ratio}"));
                Ok(status)
            }
            Operation::Effort { value } => {
                if let Some(value) = value {
                    model::reasoning_effort(Some(&value))?;
                    self.config.reasoning_effort = value;
                }
                Ok(format!(
                    "Reasoning effort: {} (provider validates support)",
                    self.config.reasoning_effort
                ))
            }
            _ => Err("Unsupported local control operation".into()),
        }
    }
    fn complete(
        &mut self,
        messages: &[ChatMessage],
        progress: &mut dyn FnMut(&str),
        poll: &mut dyn FnMut() -> bool,
    ) -> Result<ChatMessage, String> {
        let api = model::api_format(Some(&self.config.api_format))?;
        let body = if api == model::ApiFormat::Responses {
            efi_agent_core::responses::request(
                &self.config.model,
                messages,
                &self.config.reasoning_effort,
            )?
        } else {
            let mut messages = serde_json::to_value(messages).map_err(|e| e.to_string())?;
            for message in messages.as_array_mut().ok_or("Invalid message list")? {
                message
                    .as_object_mut()
                    .ok_or("Invalid message")?
                    .remove("response_items");
            }
            serde_json::json!({"model":self.config.model,"messages":messages,"stream":true,"reasoning_effort":self.config.reasoning_effort,"stream_options":{"include_usage":true},"tools":agent::tool_definitions(),"tool_choice":"auto"})
        };
        let body = serde_json::to_vec(&body).map_err(|e| e.to_string())?;
        if body.len() > efi_agent_core::protocol::MAX_MESSAGE_BYTES {
            return Err("Provider request exceeds limit".into());
        }
        let url = Url::parse(&self.config.api_base)?;
        let address = dns::resolve(
            &url.host,
            self.config.dns_address,
            self.config.dns_port,
            self.config.ipv4.as_ref(),
            poll,
        )?;
        let ca = self
            .config
            .ca_certificate
            .as_ref()
            .map(|path| files::read_bytes(path))
            .transpose()?;
        self.attempts += 1;
        self.last_usage = None;
        let mut socket = tls::Socket::connect(&url, address, ca, self.config.ipv4.as_ref(), poll)?;
        let header = format!(
            "POST {} HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nAccept: text/event-stream\r\nAccept-Encoding: identity\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
            url.endpoint(api.endpoint()),
            url.authority,
            self.config.api_key,
            body.len()
        );
        socket.send(header.as_bytes())?;
        socket.send(&body)?;
        let mut response = Body::new(socket)?;
        if !(200..300).contains(&response.status) {
            let mut detail = Vec::new();
            let mut bytes = [0; 1024];
            while detail.len() < 64 * 1024 {
                let count = response.read(&mut bytes)?;
                if count == 0 {
                    break;
                }
                detail.extend_from_slice(&bytes[..count]);
            }
            let detail = serde_json::from_slice::<Value>(&detail)
                .ok()
                .and_then(|v| v["error"]["message"].as_str().map(String::from))
                .unwrap_or_else(|| "Provider request failed".into());
            let detail: String = detail
                .chars()
                .filter(|c| !c.is_control())
                .take(2000)
                .collect();
            return Err(format!("Provider HTTP {}: {detail}", response.status));
        }
        if !response
            .content_type
            .split(';')
            .next()
            .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("text/event-stream"))
        {
            return Err("Provider must return text/event-stream".into());
        }
        let mut usage = None;
        let result = match api {
            model::ApiFormat::ChatCompletions => model::read_stream_with_usage(
                response,
                &mut |text| {
                    progress(text);
                    Ok(())
                },
                &mut |v| usage = Some(v.clone()),
            ),
            model::ApiFormat::Responses => efi_agent_core::responses::read_stream(
                response,
                &mut |text| {
                    progress(text);
                    Ok(())
                },
                &mut |v| usage = Some(v.clone()),
            ),
        };
        if let Some(value) = &usage {
            for (index, (_, path)) in USAGE_FIELDS.iter().enumerate() {
                if let Some(tokens) = value.pointer(path).and_then(Value::as_u64) {
                    self.totals[index] = self.totals[index].saturating_add(tokens);
                    self.reports[index] += 1;
                }
            }
        }
        self.last_usage = usage;
        result
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
    fn complete(
        &mut self,
        messages: &[ChatMessage],
        progress: &mut dyn FnMut(&str),
    ) -> Result<ChatMessage, String> {
        if self.check() {
            return Err("Request cancelled".into());
        }
        let cancelled = &mut self.cancelled;
        let poll = &mut self.poll;
        self.runtime.complete(messages, progress, &mut || {
            *cancelled |= poll();
            *cancelled
        })
    }
    fn execute(&mut self, operation: Operation) -> Result<String, String> {
        if self.check() {
            return Err("Request cancelled; operation was not executed".into());
        }
        let config = &self.runtime.config;
        match operation {
            Operation::Read { path } => files::read_or_list(&config.resolve(&path)?),
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
            _ => Err("Control requests are not file tools".into()),
        }
    }
}
