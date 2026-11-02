//! Firmware provider requests, boot-volume file operations and code tool dispatch.
use crate::{dns, files, tls};
use alloc::{
    format,
    rc::Rc,
    string::{String, ToString},
    vec::Vec,
};
use efi_agent_core::{
    agent::{self, Environment},
    budget::{Limits, Purpose},
    config::AgentConfig,
    http::{Body, Url},
    model::{self, Read},
    protocol::{ChatMessage, Operation},
};
use serde_json::Value;

pub struct Runtime {
    pub config: AgentConfig,
    attempts: u64,
    summary_attempts: u64,
    last_usage: Option<Value>,
    last_summary_usage: Option<Value>,
    last_task_estimate: u64,
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

fn ipv4(address: [u8; 4]) -> String {
    format!(
        "{}.{}.{}.{}",
        address[0], address[1], address[2], address[3]
    )
}

impl Runtime {
    pub fn load(_vm: bool) -> Result<Self, String> {
        let text = files::read("\\EFI\\AGENT\\CONFIG.JSON")?;
        let config: AgentConfig =
            serde_json::from_str(&text).map_err(|_| "Invalid EFI/AGENT/CONFIG.JSON")?;
        config.validate()?;
        Ok(Self {
            config,
            attempts: 0,
            summary_attempts: 0,
            last_usage: None,
            last_summary_usage: None,
            last_task_estimate: 0,
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
                status.push_str(&format!("Task requests: {}; summary requests: {}\nToken subtotals include both purposes. Last token fields below refer to task requests.\n", self.attempts - self.summary_attempts, self.summary_attempts));
                if let Some(usage) = &self.last_summary_usage {
                    status.push_str(&format!("Last summary usage (provider): {usage}\n"));
                }
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
        purpose: Purpose,
        progress: &mut dyn FnMut(&str),
        poll: &mut dyn FnMut() -> bool,
    ) -> Result<ChatMessage, String> {
        let api = model::api_format(Some(&self.config.api_format))?;
        let body = efi_agent_core::request::build(
            api,
            &self.config.model,
            messages,
            &self.config.reasoning_effort,
            purpose,
        )?;
        if purpose == Purpose::Task {
            self.last_task_estimate = efi_agent_core::budget::estimated_tokens(&body);
        }
        let body = efi_agent_core::request::encode(&body, &self.config.limits)?;
        let url = Url::parse(&self.config.api_base)?;
        let address = dns::resolve(
            &url.host,
            self.config.dns_address,
            self.config.dns_port,
            self.config.ipv4.as_ref(),
            poll,
        )
        .map_err(|error| {
            format!(
                "DNS A query for {} via {}:{} failed: {error}",
                url.host,
                ipv4(self.config.dns_address),
                self.config.dns_port,
            )
        })?;
        let ca = self
            .config
            .ca_certificate
            .as_ref()
            .map(|path| files::read_bytes(path))
            .transpose()?;
        self.attempts += 1;
        if purpose == Purpose::Summary {
            self.summary_attempts += 1;
            self.last_summary_usage = None;
        } else {
            self.last_usage = None;
        }
        let endpoint = format!(
            "API TCP {}.{}.{}.{}:{}",
            address[0], address[1], address[2], address[3], url.port,
        );
        let mut socket = tls::Socket::connect(
            &url,
            address,
            ca,
            self.config.ipv4.as_ref(),
            core::time::Duration::from_secs(self.config.limits.provider_timeout_seconds),
            poll,
        )
        .map_err(|error| format!("{endpoint} connection failed: {error}"))?;
        let header = format!(
            "POST {} HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nAccept: text/event-stream\r\nAccept-Encoding: identity\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
            url.endpoint(api.endpoint()),
            url.authority,
            self.config.api_key,
            body.len()
        );
        socket
            .send(header.as_bytes())
            .map_err(|error| format!("{endpoint} request handshake failed: {error}"))?;
        socket
            .send(&body)
            .map_err(|error| format!("{endpoint} request send failed: {error}"))?;
        let mut response = Body::with_limit(socket, self.config.limits.response_wire_bytes)
            .map_err(|error| format!("{endpoint} response read failed: {error}"))?;
        if !(200..300).contains(&response.status) {
            let mut detail = Vec::new();
            let mut bytes = [0; 1024];
            while detail.len() < 64 * 1024 {
                let count = response
                    .read(&mut bytes)
                    .map_err(|error| format!("{endpoint} error response read failed: {error}"))?;
                if count == 0 {
                    break;
                }
                detail.extend_from_slice(&bytes[..count]);
            }
            let error = serde_json::from_slice::<Value>(&detail).ok();
            if error.as_ref().is_some_and(|value| {
                matches!(
                    value["error"]["code"].as_str(),
                    Some("context_length_exceeded" | "max_context_length")
                ) || value["error"]["type"].as_str() == Some("context_length_exceeded")
            }) {
                return Err(
                    "ModelContextExceeded: provider rejected the context token length".into(),
                );
            }
            let detail = error
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
                &self.config.limits,
            ),
            model::ApiFormat::Responses => efi_agent_core::responses::read_stream_with_limits(
                response,
                &mut |text| {
                    progress(text);
                    Ok(())
                },
                &mut |v| usage = Some(v.clone()),
                &self.config.limits,
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
        if purpose == Purpose::Summary {
            self.last_summary_usage = usage;
        } else {
            self.last_usage = usage;
        }
        result.map_err(|error| format!("{endpoint} response stream failed: {error}"))
    }
}
pub struct Interactive<'a> {
    pub runtime: &'a mut Runtime,
    pub poll: &'a mut dyn FnMut() -> bool,
    pub cancelled: bool,
    pub execute_code:
        &'a mut dyn FnMut(efi_agent_wasm::session::SessionKind, &str) -> Result<String, String>,
    pub reset_js: &'a mut dyn FnMut() -> Result<String, String>,
    pub task: &'a mut dyn FnMut(efi_agent_core::protocol::TaskRequest) -> Result<String, String>,
    pub filesystem: Rc<dyn efi_agent_filesystem::FileSystem>,
}
impl Interactive<'_> {
    fn check(&mut self) -> bool {
        self.cancelled |= (self.poll)();
        self.cancelled
    }
}
impl Environment for Interactive<'_> {
    fn retryable_error(&self, error: &str) -> bool {
        [
            "Provider HTTP 408:",
            "Provider HTTP 429:",
            "Provider HTTP 500:",
            "Provider HTTP 502:",
            "Provider HTTP 503:",
            "Provider HTTP 504:",
            "timed out",
            "CONNECTION_RESET",
            "NO_RESPONSE",
            "TIMEOUT",
            "UnexpectedEof",
        ]
        .iter()
        .any(|marker| error.contains(marker))
    }
    fn pause_retry(&mut self, milliseconds: u64) -> Result<(), String> {
        let deadline = crate::tcp::Deadline::new(core::time::Duration::from_millis(milliseconds))?;
        while !deadline.expired() {
            if self.check() {
                return Err("Request cancelled".into());
            }
            crate::event_loop::idle(core::time::Duration::from_millis(10))?;
        }
        Ok(())
    }
    fn limits(&self) -> Limits {
        self.runtime.config.limits.clone()
    }
    fn request_bytes(&self, messages: &[ChatMessage], purpose: Purpose) -> Result<usize, String> {
        let config = &self.runtime.config;
        efi_agent_core::request::bytes(
            model::api_format(Some(&config.api_format))?,
            &config.model,
            messages,
            &config.reasoning_effort,
            purpose,
        )
    }
    fn result_reserve(&self, operation: &Operation) -> usize {
        match operation {
            Operation::ReadRange { path, limit, .. } => 12 * limit + 32768 + 6 * path.len(),
            // A background root can grow a file between admission and execution.
            Operation::Read { path } => 6 * agent::MAX_FILE_BYTES + 32768 + 6 * path.len(),
            Operation::Write { path, .. } | Operation::Edit { path, .. } => 32768 + 6 * path.len(),
            Operation::Shell { .. } | Operation::Js { .. } | Operation::Task { .. } => {
                6 * 2 * 65536 + 8192
            }
            _ => 32768,
        }
    }
    fn request_tokens(&self, messages: &[ChatMessage]) -> Result<u64, String> {
        let config = &self.runtime.config;
        let estimate = efi_agent_core::budget::estimated_tokens(&efi_agent_core::request::build(
            model::api_format(Some(&config.api_format))?,
            &config.model,
            messages,
            &config.reasoning_effort,
            Purpose::Task,
        )?);
        let actual = self
            .runtime
            .last_usage
            .as_ref()
            .and_then(|usage| usage["prompt_tokens"].as_u64());
        Ok(match actual {
            Some(actual)
                if self.runtime.last_task_estimate > 0
                    && actual > self.runtime.last_task_estimate =>
            {
                (u128::from(estimate) * u128::from(actual))
                    .div_ceil(u128::from(self.runtime.last_task_estimate))
                    .min(u128::from(u64::MAX)) as u64
            }
            _ => estimate,
        })
    }
    fn cancelled(&self) -> bool {
        self.cancelled
    }
    fn complete(
        &mut self,
        messages: &[ChatMessage],
        purpose: Purpose,
        progress: &mut dyn FnMut(&str),
    ) -> Result<ChatMessage, String> {
        if self.check() {
            return Err("Request cancelled".into());
        }
        let cancelled = &mut self.cancelled;
        let poll = &mut self.poll;
        self.runtime.complete(messages, purpose, progress, &mut || {
            *cancelled |= poll();
            *cancelled
        })
    }
    fn execute(&mut self, operation: Operation) -> Result<String, String> {
        if self.check() {
            return Err("Request cancelled; operation was not executed".into());
        }
        let filesystem = self.filesystem.as_ref();
        use efi_agent_filesystem::text;
        match operation {
            Operation::Task { request } => (self.task)(request),
            Operation::ReadRange {
                path,
                offset,
                limit,
                hex,
            } => text::read_range(filesystem, &path, offset, limit, hex),
            Operation::Append { path, content } => {
                text::append(filesystem, &path, &content)?;
                Ok("Content appended".into())
            }
            Operation::Shell { command } => {
                (self.execute_code)(efi_agent_wasm::session::SessionKind::Shell, &command)
            }
            Operation::Js { code } => {
                (self.execute_code)(efi_agent_wasm::session::SessionKind::JavaScript, &code)
            }
            Operation::ResetJs => (self.reset_js)(),
            Operation::Read { path } => text::read_or_list(filesystem, &path),
            Operation::Write { path, content } => {
                text::save(filesystem, &path, &content)?;
                Ok("File saved".into())
            }
            Operation::Edit {
                path,
                old_text,
                new_text,
            } => {
                let text =
                    String::from_utf8(text::read_bytes(filesystem, &path, 48 * 1024 * 1024)?)
                        .map_err(|_| "Edit target is not UTF-8")?;
                let edited = agent::edit_text(text, &old_text, &new_text)?;
                text::save(filesystem, &path, &edited)?;
                Ok("File edited".into())
            }
            _ => Err("Control requests are not file tools".into()),
        }
    }
}
