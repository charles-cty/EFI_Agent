//! Minimal coding loop. Model state is separate from UI and slash commands.
use crate::protocol::{ChatMessage, Operation, ToolCall};
use crate::{
    budget::{self, Limits, Purpose},
    request,
};
use alloc::{format, string::String, vec, vec::Vec};
use serde::Deserialize;

pub const MAX_FILE_BYTES: usize = 512 * 1024;

#[cfg(test)]
#[path = "agent_budget_tests.rs"]
mod budget_tests;
#[cfg(test)]
#[path = "agent_execution_tests.rs"]
mod execution_tests;

pub trait Environment {
    fn complete(
        &mut self,
        messages: &[ChatMessage],
        purpose: Purpose,
        progress: &mut dyn FnMut(&str),
    ) -> Result<ChatMessage, String>;
    fn execute(&mut self, operation: Operation) -> Result<String, String>;
    fn limits(&self) -> Limits {
        Limits::default()
    }
    fn request_bytes(&self, messages: &[ChatMessage], purpose: Purpose) -> Result<usize, String> {
        request::bytes(
            crate::model::ApiFormat::ChatCompletions,
            "",
            messages,
            "medium",
            purpose,
        )
    }
    fn request_tokens(&self, messages: &[ChatMessage]) -> Result<u64, String> {
        Ok(budget::estimated_tokens(&request::build(
            crate::model::ApiFormat::ChatCompletions,
            "",
            messages,
            "medium",
            Purpose::Task,
        )?))
    }
    /// Maximum escaped result bytes, including framing, before any effects.
    fn result_reserve(&self, operation: &Operation) -> usize {
        match operation {
            Operation::ReadRange { path, limit, .. } => 12 * limit + 32768 + 6 * path.len(),
            Operation::Read { path } => 6 * MAX_FILE_BYTES + 32768 + 6 * path.len(),
            Operation::Write { path, .. } | Operation::Edit { path, .. } => 32768 + 6 * path.len(),
            Operation::Shell { .. } | Operation::Js { .. } | Operation::Task { .. } => {
                6 * 2 * 65536 + 8192
            }
            _ => 32 * 1024,
        }
    }
    /// Sticky for one turn. A cancelled environment must not execute more tools.
    fn cancelled(&self) -> bool {
        false
    }
    fn retryable_error(&self, _error: &str) -> bool {
        false
    }
    fn pause_retry(&mut self, _milliseconds: u64) -> Result<(), String> {
        Ok(())
    }
}

pub enum Event<'a> {
    Notice(&'a str),
    ModelStarted,
    AssistantDelta(&'a str),
    Assistant(&'a str),
    Reasoning {
        text: &'a str,
        summary: bool,
    },
    ToolStarted {
        name: &'a str,
        arguments: &'a str,
    },
    ToolFinished {
        name: &'a str,
        result: &'a str,
        failed: bool,
    },
}

pub struct Agent {
    messages: Vec<ChatMessage>,
    limits: Limits,
    last_request_bytes: usize,
    compactions: usize,
    summary_requests: usize,
    last_compaction: Option<(usize, usize)>,
}

impl Default for Agent {
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            last_request_bytes: 0,
            compactions: 0,
            summary_requests: 0,
            last_compaction: None,
            messages: vec![ChatMessage::text(
                "system",
                String::from(
                    "You are a coding assistant running real WASM programs in UEFI. BusyBox ash provides POSIX shell syntax. Use read, write and edit for files; read before editing and use byte ranges for large files or binary data. File tool paths use the shared guest root independently of shell cwd. write stages replacements atomically; append=true adds a chunk. edit requires one unique exact match. Use shell for short commands and js for JavaScript with print, console.log/error, top-level await and await ash`command`. Each retains its interpreter state, including shell cwd/environment and JS globals. Use task for long builds, background services or interactive input: start returns an id, poll waits and reads previews, input sends bytes/EOF, cancel stops that task tree, and forget releases a completed handle. Each task has a fresh shell; set its cwd/environment in its command. Tasks survive model rounds and requests. Cancelling a task or its wait preserves the retained interpreters. shell/js have empty stdin and stop unfinished children after evaluation; their exit, trap or cancellation can reset the affected interpreter. reset_js resets only JS. /clear resets interpreters and stops tasks; /compact preserves them. Complete stdout/stderr are saved at the reported paths; previews show both ends. Use ranged read for omitted output, and release old log files when finished. Run Python through python/python3 scripts or quoted heredocs; standard subprocess is supported. Installed commands resolve through PATH and /bin, executable WASM files and executable scripts with absolute shebangs. bash is an alias for ash. /bin, /dev, /proc and /tmp are virtual. Node APIs, a Linux kernel, apk and arbitrary native Linux binaries are unavailable. Tool errors are real; do not claim success after failure. Automatic compaction summarizes completed exchanges without replaying tools or resetting interpreters. /status shows execution/context budgets and provider token usage. Byte limits are separate from the estimated token-window admission check.",
                ),
            )],
        }
    }
}

impl Agent {
    pub fn configure_limits(&mut self, limits: Limits) -> Result<(), String> {
        limits.validate()?;
        self.limits = limits;
        Ok(())
    }
    pub fn history_status(&self) -> Result<String, String> {
        let bytes = request::history_bytes(&self.messages)?;
        Ok(format!(
            "History: {} messages, {bytes}/{} canonical JSON bytes\nLast measured task request: {}/{} bytes\nCompaction trigger/target: {}/{} bytes\nCompactions: {}; summary requests: {}; last before/after: {:?}\nResponse message/event/wire limits: {}/{}/{} bytes\nUI history limit: {} bytes\nCurrent history tokens: unavailable (no exact tokenizer)\nConfigured model context window: {}\nTool round budget: {:?} (None = until completion or cancellation)\nProvider retries: {}; idle timeout: {} seconds\nReserved response tokens: {}; token-window admission uses an estimate",
            self.messages.len(),
            self.limits.history_bytes,
            self.last_request_bytes,
            self.limits.request_bytes,
            self.limits.compact_at_bytes,
            self.limits.compact_target_bytes,
            self.compactions,
            self.summary_requests,
            self.last_compaction,
            self.limits.response_message_bytes,
            self.limits.event_bytes,
            self.limits.response_wire_bytes,
            self.limits.ui_history_bytes,
            self.limits
                .context_window_tokens
                .map_or_else(|| "unavailable".into(), |value| format!("{value} tokens")),
            self.limits.max_tool_rounds,
            self.limits.provider_retries,
            self.limits.provider_timeout_seconds,
            self.limits.response_token_reserve,
        ))
    }

    pub fn compact<E: Environment>(
        &mut self,
        environment: &mut E,
        mut observe: impl FnMut(Event<'_>),
    ) -> Result<String, String> {
        self.limits = environment.limits();
        self.limits.validate()?;
        let before = environment.request_bytes(&self.messages, Purpose::Task)?;
        let candidate =
            crate::history::compact(&self.messages, environment, &self.limits, &mut |text| {
                observe(Event::Notice(text))
            })?;
        let Some(candidate) = candidate else {
            return Ok(
                "No earlier completed exchanges can be compacted. Interpreter state is preserved."
                    .into(),
            );
        };
        let after = environment.request_bytes(&candidate.messages, Purpose::Task)?;
        if after >= before {
            return Err(
                "CompactionFailed: summary did not reduce request size; history is unchanged"
                    .into(),
            );
        }
        let retained = request::history_bytes(&candidate.messages)?;
        if after > self.limits.request_bytes || retained > self.limits.history_bytes {
            return Err(format!(
                "CurrentTurnBudgetExceeded: compacted request {after}/{} bytes, history {retained}/{} bytes; history and interpreters are preserved",
                self.limits.request_bytes, self.limits.history_bytes
            ));
        }
        self.messages = candidate.messages;
        self.compactions += 1;
        self.summary_requests += candidate.requests;
        self.last_compaction = Some((before, after));
        self.last_request_bytes = after;
        Ok(format!(
            "Conversation compacted: {before} -> {after} request bytes. Interpreter state is preserved."
        ))
    }

    fn prepare<E: Environment>(
        &mut self,
        environment: &mut E,
        observe: &mut impl FnMut(Event<'_>),
    ) -> Result<(), String> {
        self.limits = environment.limits();
        self.limits.validate()?;
        self.last_request_bytes = environment.request_bytes(&self.messages, Purpose::Task)?;
        let mut retained = request::history_bytes(&self.messages)?;
        if self.last_request_bytes >= self.limits.compact_at_bytes
            || retained > self.limits.history_bytes
            || self.token_budget_exceeded(environment)?
        {
            if let Err(error) = self.compact(environment, &mut *observe) {
                if self.last_request_bytes > self.limits.request_bytes
                    || retained > self.limits.history_bytes
                {
                    return Err(error);
                }
                observe(Event::Notice(
                    "Automatic compaction was not committed; the request still fits configured hard limits.",
                ));
            }
            self.last_request_bytes = environment.request_bytes(&self.messages, Purpose::Task)?;
            retained = request::history_bytes(&self.messages)?;
        }
        if self.last_request_bytes > self.limits.request_bytes {
            return Err(budget::exceeded(
                "RequestBudgetExceeded",
                self.last_request_bytes,
                self.limits.request_bytes,
            ));
        }
        if retained > self.limits.history_bytes {
            return Err(budget::exceeded(
                "HistoryBudgetExceeded",
                retained,
                self.limits.history_bytes,
            ));
        }
        if self.token_budget_exceeded(environment)? {
            return Err("ContextTokenBudgetExceeded: estimated request plus reserved response exceeds the configured model window; reduce current-turn data or increase the configured window".into());
        }
        Ok(())
    }

    fn token_budget_exceeded<E: Environment>(&self, environment: &E) -> Result<bool, String> {
        Ok(match self.limits.context_window_tokens {
            Some(window) => {
                environment
                    .request_tokens(&self.messages)?
                    .saturating_add(self.limits.response_token_reserve)
                    > window
            }
            None => false,
        })
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn turn<E: Environment>(
        &mut self,
        prompt: String,
        environment: &mut E,
        mut observe: impl FnMut(Event<'_>),
    ) -> Result<(), String> {
        self.limits = environment.limits();
        self.limits.validate()?;
        let mut retained = request::history_bytes(&self.messages)?;
        let mut request_bytes = environment.request_bytes(&self.messages, Purpose::Task)?;
        if request_bytes >= self.limits.compact_at_bytes || retained > self.limits.history_bytes {
            if let Err(error) = self.compact(environment, &mut observe) {
                if request_bytes > self.limits.request_bytes || retained > self.limits.history_bytes
                {
                    return Err(error);
                }
                observe(Event::Notice(
                    "Automatic compaction was not committed; the request still fits configured hard limits.",
                ));
            }
            retained = request::history_bytes(&self.messages)?;
            request_bytes = environment.request_bytes(&self.messages, Purpose::Task)?;
        }
        let user_message_bytes = budget::text_message_bytes("user", &prompt);
        let projected_history = retained
            .saturating_add(usize::from(!self.messages.is_empty()))
            .saturating_add(user_message_bytes);
        let projected_request = request_bytes
            .saturating_add(usize::from(!self.messages.is_empty()))
            .saturating_add(user_message_bytes);
        if projected_history > self.limits.history_bytes {
            return Err(budget::exceeded(
                "HistoryBudgetExceeded",
                projected_history,
                self.limits.history_bytes,
            ));
        }
        if projected_request > self.limits.request_bytes {
            return Err(budget::exceeded(
                "RequestBudgetExceeded",
                projected_request,
                self.limits.request_bytes,
            ));
        }
        self.messages.push(ChatMessage::text("user", prompt));
        let mut context_retried = false;
        for round in 0usize.. {
            if let Err(error) = self.prepare(environment, &mut observe) {
                if round == 0
                    && self
                        .messages
                        .last()
                        .is_some_and(|message| message.role == "user")
                {
                    self.messages.pop();
                }
                return Err(error);
            }
            observe(Event::ModelStarted);
            let mut streamed = false;
            let response = complete_with_retry(
                environment,
                &self.messages,
                Purpose::Task,
                &mut |delta: &str| {
                    if !delta.is_empty() {
                        streamed = true;
                        observe(Event::AssistantDelta(delta));
                    }
                },
            );
            let mut reply = match response {
                Err(error) if error.starts_with("ModelContextExceeded:") && !context_retried => {
                    context_retried = true;
                    let before = self.compactions;
                    self.compact(environment, &mut observe)?;
                    if self.compactions == before {
                        return Err(error);
                    }
                    observe(Event::ModelStarted);
                    streamed = false;
                    complete_with_retry(environment, &self.messages, Purpose::Task, &mut |delta| {
                        if !delta.is_empty() {
                            streamed = true;
                            observe(Event::AssistantDelta(delta));
                        }
                    })?
                }
                response => response?,
            };
            if environment.cancelled() {
                return Err(String::from("Request cancelled"));
            }
            validate_reply(&reply)?;
            for text in [
                reply.reasoning_content.as_deref(),
                reply.reasoning.as_deref(),
            ]
            .into_iter()
            .flatten()
            .filter(|text| !text.is_empty())
            {
                observe(Event::Reasoning {
                    text,
                    summary: false,
                });
            }
            if let Some(text) = reply
                .reasoning_summary
                .as_deref()
                .filter(|text| !text.is_empty())
            {
                observe(Event::Reasoning {
                    text,
                    summary: true,
                });
            }
            for detail in &reply.reasoning_details {
                let (field, summary) = match detail["type"].as_str() {
                    Some("reasoning.text") => ("text", false),
                    Some("reasoning.summary") => ("summary", true),
                    _ => continue,
                };
                if let Some(text) = detail[field].as_str().filter(|text| !text.is_empty()) {
                    observe(Event::Reasoning { text, summary });
                }
            }
            if !streamed && let Some(content) = reply.content.as_deref().filter(|s| !s.is_empty()) {
                observe(Event::Assistant(content));
            }
            if reply.tool_calls.is_empty() {
                reply.retain_canonical();
                self.messages.push(reply);
                return Ok(());
            }
            let calls = core::mem::take(&mut reply.tool_calls);
            reply.retain_canonical();
            // Chat Completions needs the call fields in its retained message.
            if reply.response_items.is_empty() {
                reply.tool_calls = calls.clone();
            }
            self.messages.push(reply);
            let reserve = calls.iter().fold(0usize, |sum, call| {
                sum.saturating_add(operation(call).map_or(32 * 1024, |operation| {
                    environment.result_reserve(&operation)
                }))
            });
            // Always pair every accepted call, including admission and compaction failures.
            let admission = (|| -> Result<bool, String> {
                let projected = environment
                    .request_bytes(&self.messages, Purpose::Task)?
                    .saturating_add(reserve);
                if projected > self.limits.request_bytes {
                    self.compact(environment, &mut observe)?;
                }
                Ok(environment
                    .request_bytes(&self.messages, Purpose::Task)?
                    .saturating_add(reserve)
                    > self.limits.request_bytes
                    || request::history_bytes(&self.messages)?.saturating_add(reserve)
                        > self.limits.history_bytes)
            })();
            let rejected = !matches!(admission, Ok(false));
            for call in &calls {
                observe(Event::ToolStarted {
                    name: &call.function.name,
                    arguments: &call.function.arguments,
                });
                let result = if rejected {
                    Err(format!(
                        "ToolResultBudgetExceeded: result group cannot fit request/history limits {}/{} bytes; operation was not executed. Request fewer or smaller results",
                        self.limits.request_bytes, self.limits.history_bytes
                    ))
                } else if environment.cancelled() {
                    Err(String::from(
                        "Request cancelled; operation was not executed",
                    ))
                } else if self
                    .limits
                    .max_tool_rounds
                    .is_some_and(|limit| round >= limit)
                {
                    Err(String::from(
                        "Tool round limit reached; operation was not executed",
                    ))
                } else {
                    operation(call).and_then(|operation| environment.execute(operation))
                };
                let failed = result.is_err();
                let content = match result {
                    Ok(text) => text,
                    Err(error) => format!("Tool error: {error}"),
                };
                observe(Event::ToolFinished {
                    name: &call.function.name,
                    result: &content,
                    failed,
                });
                let mut message = ChatMessage::text("tool", content);
                message.tool_call_id = Some(call.id.clone());
                self.messages.push(message);
            }
            admission?;
            if environment.cancelled() {
                return Err(String::from("Request cancelled"));
            }
            if self
                .limits
                .max_tool_rounds
                .is_some_and(|limit| round >= limit)
            {
                return Err(String::from(
                    "Tool round budget reached. Send another request to continue from retained results; pending operations were not executed.",
                ));
            }
        }
        unreachable!("Tool loop ends by completion, cancellation, or an explicit budget")
    }
}

fn complete_with_retry<E: Environment>(
    environment: &mut E,
    messages: &[ChatMessage],
    purpose: Purpose,
    progress: &mut dyn FnMut(&str),
) -> Result<ChatMessage, String> {
    let retries = environment.limits().provider_retries;
    for attempt in 0..=retries {
        let mut streamed = false;
        let response = environment.complete(messages, purpose, &mut |text| {
            streamed |= !text.is_empty();
            progress(text);
        });
        match response {
            Err(error)
                if !streamed
                    && attempt < retries
                    && !environment.cancelled()
                    && environment.retryable_error(&error) =>
            {
                environment.pause_retry(250u64.saturating_mul(1 << attempt.min(8)))?;
            }
            result => return result,
        }
    }
    unreachable!("Last provider attempt returns its result")
}

fn validate_reply(reply: &ChatMessage) -> Result<(), String> {
    if reply.role != "assistant" || reply.tool_call_id.is_some() {
        return Err(String::from(
            "Provider returned an invalid assistant message",
        ));
    }
    if reply.tool_calls.len() > 8 {
        return Err(String::from("Provider returned more than eight tool calls"));
    }
    if reply.tool_calls.is_empty() && reply.content.is_none() {
        return Err(String::from(
            "Provider returned neither text nor tool calls",
        ));
    }
    for (index, call) in reply.tool_calls.iter().enumerate() {
        if call.id.is_empty()
            || call.kind != "function"
            || reply.tool_calls[..index]
                .iter()
                .any(|previous| previous.id == call.id)
        {
            return Err(String::from(
                "Provider returned invalid or duplicate tool call IDs",
            ));
        }
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Read {
    path: String,
    #[serde(default)]
    offset: Option<u64>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    hex: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Write {
    path: String,
    content: String,
    #[serde(default)]
    append: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Edit {
    path: String,
    old_text: String,
    new_text: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Shell {
    command: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JavaScript {
    code: String,
}

fn operation(call: &ToolCall) -> Result<Operation, String> {
    let arguments = &call.function.arguments;
    match call.function.name.as_str() {
        "task" => {
            let request = serde_json::from_str(arguments)
                .map_err(|error| format!("Invalid task arguments: {error}"))?;
            Ok(Operation::Task { request })
        }
        "shell" => {
            let args: Shell = serde_json::from_str(arguments)
                .map_err(|e| format!("Invalid shell arguments: {e}"))?;
            Ok(Operation::Shell {
                command: args.command,
            })
        }
        "js" => {
            let args: JavaScript = serde_json::from_str(arguments)
                .map_err(|e| format!("Invalid js arguments: {e}"))?;
            Ok(Operation::Js { code: args.code })
        }
        "reset_js" => {
            let args: serde_json::Map<String, serde_json::Value> = serde_json::from_str(arguments)
                .map_err(|e| format!("Invalid reset_js arguments: {e}"))?;
            if !args.is_empty() {
                return Err("reset_js takes an empty object".into());
            }
            Ok(Operation::ResetJs)
        }
        "read" => {
            let args: Read = serde_json::from_str(arguments)
                .map_err(|e| format!("Invalid read arguments: {e}"))?;
            if args.offset.is_some() || args.limit.is_some() || args.hex {
                let limit = args.limit.unwrap_or(64 * 1024);
                if !(4..=MAX_FILE_BYTES).contains(&limit) {
                    return Err("Read range limit must be between 4 and 524288 bytes".into());
                }
                Ok(Operation::ReadRange {
                    path: args.path,
                    offset: args.offset.unwrap_or(0),
                    limit,
                    hex: args.hex,
                })
            } else {
                Ok(Operation::Read { path: args.path })
            }
        }
        "write" => {
            let args: Write = serde_json::from_str(arguments)
                .map_err(|e| format!("Invalid write arguments: {e}"))?;
            Ok(if args.append {
                Operation::Append {
                    path: args.path,
                    content: args.content,
                }
            } else {
                Operation::Write {
                    path: args.path,
                    content: args.content,
                }
            })
        }
        "edit" => {
            let args: Edit = serde_json::from_str(arguments)
                .map_err(|e| format!("Invalid edit arguments: {e}"))?;
            Ok(Operation::Edit {
                path: args.path,
                old_text: args.old_text,
                new_text: args.new_text,
            })
        }
        _ => Err(format!(
            "Unknown tool: {}",
            call.function.name.chars().take(128).collect::<String>()
        )),
    }
}

/// Refuse zero or multiple matches, including overlapping occurrences.
pub fn edit_text(mut text: String, old: &str, new: &str) -> Result<String, String> {
    if old.is_empty() {
        return Err(String::from("old_text must not be empty"));
    }
    let start = text
        .find(old)
        .ok_or("old_text was not found; file was not changed")?;
    let next = start + old.chars().next().expect("Non-empty old_text").len_utf8();
    if text[next..].contains(old) {
        return Err(String::from(
            "old_text has multiple matches; file was not changed",
        ));
    }
    let length = text.len() - old.len() + new.len();
    if length > 48 * 1024 * 1024 {
        return Err(String::from("Edited file exceeds 48 MiB"));
    }
    text.replace_range(start..start + old.len(), new);
    Ok(text)
}

pub fn tool_definitions() -> serde_json::Value {
    serde_json::json!([
        {
            "type": "function",
            "function": {
                "name": "read",
                "description": "Read a UTF-8 file or sorted directory names. Without range arguments, read the complete text up to 512 KiB. For larger files or output logs, set offset (byte offset) and limit (4..524288 bytes, default 65536). Ranged reads return JSON with content, encoding, next_offset, total_bytes and eof. UTF-8 pages end at a character boundary; continue from next_offset. Set hex=true for exact binary bytes. Paths use the shared root independently of shell cwd.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "offset": { "type": "integer", "minimum": 0 },
                        "limit": { "type": "integer", "minimum": 4, "maximum": 524288 },
                        "hex": { "type": "boolean" },
                        "path": {
                            "type": "string",
                            "description": "Path in the shared guest root, independent of shell cwd. Use / separators; a leading / is optional. Parent components (..) are rejected."
                        }
                    },
                    "required": [
                        "path"
                    ],
                    "additionalProperties": false
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "write",
                "description": "Create or atomically replace a UTF-8 file, creating missing parent directories. Replacement is staged in a sibling file and committed by rename; a staging failure preserves the original. Atomic replacement requires a regular-file destination and replaces its inode. Set append=true to append a chunk; append is not atomic and failures can leave partial appended content. Content is bounded by model request and filesystem capacity, not a 512 KiB tool cap.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path in the shared guest root, independent of shell cwd. Use / separators; a leading / is optional. Parent components (..) are rejected."
                        },
                        "append": { "type": "boolean" },
                        "content": {
                            "type": "string",
                            "description": "Replacement content, or a chunk to append when append=true."
                        }
                    },
                    "required": [
                        "path",
                        "content"
                    ],
                    "additionalProperties": false
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "edit",
                "description": "Atomically replace exactly one occurrence of old_text with new_text in a UTF-8 file up to the filesystem file-size limit (48 MiB). old_text must be nonempty and unique, including overlapping matches. Missing or multiple matches leave the file unchanged. A sibling staging file preserves original bytes if staging fails; regular-file destinations only.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path in the shared guest root, independent of shell cwd. Use / separators; a leading / is optional. Parent components (..) are rejected."
                        },
                        "old_text": {
                            "type": "string"
                        },
                        "new_text": {
                            "type": "string"
                        }
                    },
                    "required": [
                        "path",
                        "old_text",
                        "new_text"
                    ],
                    "additionalProperties": false
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "shell",
                "description": "Run BusyBox ash in a retained session. Variables, environment and cwd persist. Stdin is empty and there is no controlling terminal; use task for live input, a terminal, background services or long builds. Return exit status plus separate stdout/stderr previews. Full output is saved to stdout_path/stderr_path; use ranged read to retrieve it. Previews retain the first and last 32 KiB per stream. Nonzero exit is a normal result. Remaining children are stopped when the call finishes. Exit, trap or cancellation resets this shell with a notice; task cancellation preserves it. /clear resets sessions and tasks; /compact preserves them.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "Shell source, at most 60 KiB UTF-8 and no NUL; supports quotes, newlines, pipes and redirections."
                        }
                    },
                    "required": [
                        "command"
                    ],
                    "additionalProperties": false
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "js",
                "description": "Run JavaScript in retained QuickJS. Global bindings/functions persist; redeclaring global let/const fails. Use print, console.log/error and top-level await. await ash`command` runs a fresh ash child, separate from the shell tool. Interpolation quotes words; arrays expand to words. ProcessOutput provides exitCode/stdout/stderr/stdall/ok and text/json/lines/buffer. Nonzero exit rejects; .nothrow() returns it. ash({cwd,env,input,quiet,verbose,nothrow,prefix,postfix}) configures children. Inner command input/capture is bounded to 512 KiB per stream. Complete interpreter output is saved to stdout_path/stderr_path; previews show the first and last 32 KiB per stream. Use ranged read for full bytes. Use task for long-running commands, a terminal or live input. Node APIs, module imports, timers, streaming pipes, signals and command timeouts are unavailable. Ordinary exceptions preserve state; quit, trap, cancellation or an unresolved top-level Promise without an event source resets it. reset_js releases only JS; /clear resets interpreters and tasks; /compact preserves them.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "code": {
                            "type": "string",
                            "description": "JavaScript source in the retained global context, at most 60 KiB UTF-8 and no NUL."
                        }
                    },
                    "required": [
                        "code"
                    ],
                    "additionalProperties": false
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "task",
                "description": "Manage a persistent ash command with a task id. start requires command and returns immediately by default. Optional terminal=true supplies a controlling terminal with merged output; otherwise stdout/stderr remain separate. input sends UTF-8 bytes; eof closes stdin. poll can wait_ms up to 10000; cancel stops only that task tree; list reports handles; forget releases a completed handle and keeps its logs. Tasks continue across model rounds and requests, with separate shell state. Full logs are saved at stdout_path/stderr_path; read them by byte range. The first and last 32 KiB per stream are previews. Cancelling a wait leaves the task running. /clear stops all tasks. Use tasks for builds, servers, watchers and interactive commands.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["start", "poll", "input", "cancel", "list", "forget"] },
                        "command": { "type": "string" },
                        "id": { "type": "integer", "minimum": 1 },
                        "input": { "type": "string" },
                        "eof": { "type": "boolean" },
                        "terminal": { "type": "boolean" },
                        "wait_ms": { "type": "integer", "minimum": 0, "maximum": 10000 }
                    },
                    "required": ["action"],
                    "additionalProperties": false
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "reset_js",
                "description": "Stop and release the retained JavaScript interpreter and its children. Discards JavaScript bindings and settings; preserves shell state, files and independent roots. The next js call starts a fresh interpreter. Safe before the first js call.",
                "parameters": {
                    "type": "object",
                    "properties": {},
                    "required": [],
                    "additionalProperties": false
                }
            }
        }
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn local_terminal_transcript_is_absent_from_model_requests() {
        struct InspectManual;
        impl Environment for InspectManual {
            fn complete(
                &mut self,
                messages: &[ChatMessage],
                _purpose: Purpose,
                _: &mut dyn FnMut(&str),
            ) -> Result<ChatMessage, String> {
                assert_eq!(messages.len(), 2);
                assert_eq!(
                    messages[1].content.as_deref(),
                    Some("Continue from my value")
                );
                Ok(ChatMessage::text("assistant", "Value is available".into()))
            }
            fn execute(&mut self, _: Operation) -> Result<String, String> {
                panic!("Recording manual execution must not run a model tool");
            }
        }
        let mut agent = Agent::default();
        let before = agent.history_status().unwrap();
        let mut terminal = crate::repl::Terminal::default();
        terminal.begin(
            crate::repl::Language::JavaScript,
            "let privateValue = 41;\nprivateValue + 1",
        );
        terminal.output(b"42\n");
        assert_eq!(agent.history_status().unwrap(), before);
        agent
            .turn("Continue from my value".into(), &mut InspectManual, |_| {})
            .unwrap();
    }

    #[test]
    fn code_tools_preserve_source_and_reject_invalid_calls() {
        for (name, field, source) in [
            ("shell", "command", "printf '%s\\n' 'a b'\necho 中 >&2"),
            ("js", "code", "let value = 'a b';\nprint(value, '中')"),
        ] {
            let mut call: ToolCall = serde_json::from_value(serde_json::json!({
                "id": "code", "type": "function", "function": {
                    "name": name, "arguments": serde_json::json!({field: source}).to_string()
                }
            }))
            .unwrap();
            let actual = match operation(&call).unwrap() {
                Operation::Shell { command } => command,
                Operation::Js { code } => code,
                _ => panic!("Code tool must dispatch a code operation"),
            };
            assert_eq!(actual, source);
            for invalid in [
                "{}",
                "{\"code\":12}",
                "{\"command\":\"echo x\",\"extra\":true}",
            ] {
                call.function.arguments = invalid.into();
                assert!(operation(&call).is_err());
            }
        }
        let definitions = tool_definitions();
        let names: Vec<_> = definitions
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["function"]["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            ["read", "write", "edit", "shell", "js", "task", "reset_js"]
        );
        let removed: ToolCall = serde_json::from_value(serde_json::json!({
            "id": "removed", "type": "function", "function": {
                "name": "python", "arguments": "{\"code\":\"print(1)\"}"
            }
        }))
        .unwrap();
        assert!(operation(&removed).is_err());
        let mut reset = removed;
        reset.function.name = "reset_js".into();
        reset.function.arguments = "{}".into();
        assert!(matches!(operation(&reset).unwrap(), Operation::ResetJs));
        for arguments in ["null", "[]", "{\"extra\":true}"] {
            reset.function.arguments = arguments.into();
            assert!(operation(&reset).is_err());
        }
    }
    #[test]
    fn unique_edit_including_overlaps_and_unicode() {
        assert_eq!(
            edit_text("left 中 right".into(), "中", "UTF-8").unwrap(),
            "left UTF-8 right"
        );
        assert!(edit_text("abc".into(), "z", "x").is_err());
        assert!(edit_text("abc abc".into(), "abc", "x").is_err());
        assert!(edit_text("aaa".into(), "aa", "x").is_err());
        assert!(edit_text("中中中".into(), "中中", "x").is_err());
        assert!(edit_text("abc".into(), "", "x").is_err());
        assert_eq!(edit_text("abc".into(), "abc", "").unwrap(), "");
        assert_eq!(
            edit_text("a".into(), "a", &"x".repeat(MAX_FILE_BYTES + 1))
                .unwrap()
                .len(),
            MAX_FILE_BYTES + 1
        );
    }

    #[test]
    fn edit_matches_independent_exhaustive_oracle() {
        // Enumerate every candidate character boundary instead of using the
        // production search method. Include multi-byte characters and overlap.
        let alphabet = ['a', 'b', '中'];
        let mut texts = vec![String::new()];
        for length in 1..=5 {
            for mut number in 0..3usize.pow(length) {
                let mut text = String::new();
                for _ in 0..length {
                    text.push(alphabet[number % 3]);
                    number /= 3;
                }
                texts.push(text);
            }
        }
        for text in &texts {
            for old in texts.iter().filter(|s| s.chars().count() <= 3) {
                let positions: Vec<_> = text
                    .char_indices()
                    .map(|(index, _)| index)
                    .filter(|index| text[*index..].starts_with(old))
                    .collect();
                let result = edit_text(text.clone(), old, "Z");
                if !old.is_empty() && positions.len() == 1 {
                    let index = positions[0];
                    let expected = format!("{}Z{}", &text[..index], &text[index + old.len()..]);
                    assert_eq!(result.unwrap(), expected);
                } else {
                    assert!(result.is_err());
                }
            }
        }
    }

    struct ErrorThenAnswer {
        calls: usize,
    }
    impl Environment for ErrorThenAnswer {
        fn complete(
            &mut self,
            messages: &[ChatMessage],
            _purpose: Purpose,
            _progress: &mut dyn FnMut(&str),
        ) -> Result<ChatMessage, String> {
            self.calls += 1;
            if self.calls == 1 {
                serde_json::from_str(r#"{"role":"assistant","content":null,"tool_calls":[{"id":"read-731","type":"function","function":{"name":"read","arguments":"{\"path\":\"missing.txt\"}"}}]}"#).map_err(|e|e.to_string())
            } else {
                let last = messages.last().unwrap();
                assert_eq!(last.role, "tool");
                assert_eq!(last.tool_call_id.as_deref(), Some("read-731"));
                assert_eq!(last.content.as_deref(), Some("Tool error: File not found"));
                Ok(ChatMessage::text(
                    "assistant",
                    "Cannot read the missing file".into(),
                ))
            }
        }
        fn execute(&mut self, _: Operation) -> Result<String, String> {
            Err("File not found".into())
        }
    }

    #[test]
    fn failed_tool_returns_correlated_result_to_model() {
        let mut environment = ErrorThenAnswer { calls: 0 };
        Agent::default()
            .turn("Read missing.txt".into(), &mut environment, |_| {})
            .unwrap();
        assert_eq!(environment.calls, 2);
    }

    struct Repeating {
        executed: usize,
        completions: usize,
    }
    impl Environment for Repeating {
        fn limits(&self) -> Limits {
            Limits {
                max_tool_rounds: Some(12),
                ..Limits::default()
            }
        }
        fn complete(
            &mut self,
            messages: &[ChatMessage],
            _purpose: Purpose,
            _progress: &mut dyn FnMut(&str),
        ) -> Result<ChatMessage, String> {
            if self.completions > 0 {
                let last = messages.last().unwrap();
                assert_eq!(last.role, "tool");
                assert_eq!(last.tool_call_id.as_deref(), Some("repeat"));
            }
            self.completions += 1;
            serde_json::from_str(r#"{"role":"assistant","content":null,"tool_calls":[{"id":"repeat","type":"function","function":{"name":"read","arguments":"{\"path\":\"file.txt\"}"}}]}"#).map_err(|e|e.to_string())
        }
        fn execute(&mut self, _: Operation) -> Result<String, String> {
            self.executed += 1;
            Ok("text".into())
        }
    }

    #[test]
    fn round_limit_retires_pending_calls_without_more_file_operations() {
        let mut agent = Agent::default();
        let mut environment = Repeating {
            executed: 0,
            completions: 0,
        };
        assert!(
            agent
                .turn("repeat".into(), &mut environment, |_| {})
                .is_err()
        );
        assert_eq!(environment.executed, 12);
        assert_eq!(environment.completions, 13);
        let last = agent.messages.last().unwrap();
        assert_eq!(last.role, "tool");
        assert!(last.content.as_deref().unwrap().contains("not executed"));
    }

    #[test]
    fn duplicate_ids_reject_entire_reply() {
        let mut reply:ChatMessage=serde_json::from_str(r#"{"role":"assistant","content":null,"tool_calls":[{"id":"duplicate","type":"function","function":{"name":"read","arguments":"{}"}}]}"#).unwrap();
        reply.tool_calls.push(reply.tool_calls[0].clone());
        assert!(validate_reply(&reply).is_err());
    }

    #[test]
    fn streamed_text_is_observed_once_and_only_success_enters_history() {
        struct Streaming {
            fail: bool,
        }
        impl Environment for Streaming {
            fn complete(
                &mut self,
                _messages: &[ChatMessage],
                _purpose: Purpose,
                progress: &mut dyn FnMut(&str),
            ) -> Result<ChatMessage, String> {
                progress("left 中");
                progress(" right");
                if self.fail {
                    Err("truncated stream".into())
                } else {
                    Ok(ChatMessage::text("assistant", "left 中 right".into()))
                }
            }
            fn execute(&mut self, _operation: Operation) -> Result<String, String> {
                panic!("A text stream must not execute tools")
            }
        }
        for fail in [false, true] {
            let mut agent = Agent::default();
            let mut text = String::new();
            let mut full_replies = 0;
            let result = agent.turn(
                "prompt".into(),
                &mut Streaming { fail },
                |event| match event {
                    Event::AssistantDelta(delta) => text.push_str(delta),
                    Event::Assistant(_) => full_replies += 1,
                    _ => {}
                },
            );
            assert_eq!(result.is_err(), fail);
            assert_eq!(text, "left 中 right");
            assert_eq!(full_replies, 0);
            assert_eq!(agent.messages.len(), if fail { 2 } else { 3 });
            if !fail {
                assert_eq!(agent.messages[2].content.as_deref(), Some(text.as_str()));
            }
        }
    }

    #[test]
    fn cancelled_tool_batch_retires_ids_and_allows_next_prompt() {
        struct CancelAfterRead {
            cancelled: bool,
            executed: usize,
            completions: usize,
        }
        impl Environment for CancelAfterRead {
            fn result_reserve(&self, _: &Operation) -> usize {
                1024
            }
            fn cancelled(&self) -> bool {
                self.cancelled
            }
            fn complete(
                &mut self,
                messages: &[ChatMessage],
                _purpose: Purpose,
                _progress: &mut dyn FnMut(&str),
            ) -> Result<ChatMessage, String> {
                self.completions += 1;
                if self.completions == 1 {
                    let mut reply = ChatMessage::text("assistant", "Inspect first".into());
                    for id in ["first", "second", "third"] {
                        reply.tool_calls.push(
                            serde_json::from_value(serde_json::json!({
                                "id": id, "type": "function", "function": {
                                    "name": "read", "arguments": "{\"path\":\"file.txt\"}"
                                }
                            }))
                            .unwrap(),
                        );
                    }
                    Ok(reply)
                } else {
                    assert_eq!(
                        messages[messages.len() - 1].content.as_deref(),
                        Some("try again")
                    );
                    let results = &messages[messages.len() - 4..messages.len() - 1];
                    for (result, id) in results.iter().zip(["first", "second", "third"]) {
                        assert_eq!(result.role, "tool");
                        assert_eq!(result.tool_call_id.as_deref(), Some(id));
                    }
                    assert_eq!(results[0].content.as_deref(), Some("read completed"));
                    assert!(
                        results[1]
                            .content
                            .as_deref()
                            .unwrap()
                            .contains("not executed")
                    );
                    assert!(
                        results[2]
                            .content
                            .as_deref()
                            .unwrap()
                            .contains("not executed")
                    );
                    Ok(ChatMessage::text("assistant", "recovered".into()))
                }
            }
            fn execute(&mut self, _: Operation) -> Result<String, String> {
                self.executed += 1;
                self.cancelled = true;
                Ok("read completed".into())
            }
        }
        let mut agent = Agent::default();
        let mut environment = CancelAfterRead {
            cancelled: false,
            executed: 0,
            completions: 0,
        };
        assert_eq!(
            agent
                .turn("inspect".into(), &mut environment, |_| {})
                .unwrap_err(),
            "Request cancelled"
        );
        assert_eq!(environment.executed, 1);
        assert_eq!(environment.completions, 1);
        environment.cancelled = false;
        agent
            .turn("try again".into(), &mut environment, |_| {})
            .unwrap();
        assert_eq!(environment.executed, 1);
    }
}
