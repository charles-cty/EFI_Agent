//! Minimal coding loop. Model state is separate from UI and slash commands.
use crate::protocol::{ChatMessage, Operation, ToolCall};
use alloc::{
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use serde::Deserialize;

pub const MAX_TOOL_ROUNDS: usize = 12;
pub const MAX_FILE_BYTES: usize = 512 * 1024;
const MAX_HISTORY_BYTES: usize = 640 * 1024;

pub trait Environment {
    fn complete(&mut self, messages: &[ChatMessage]) -> Result<ChatMessage, String>;
    fn execute(&mut self, operation: Operation) -> Result<String, String>;
}

pub enum Event<'a> {
    Assistant(&'a str),
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
}

impl Default for Agent {
    fn default() -> Self {
        Self {
            messages: vec![ChatMessage::text(
                "system",
                String::from(
                    "You are a minimal coding assistant running in UEFI. Use read, write, and edit to inspect and change the configured workspace. Read files before editing. edit requires one exact match. Tool errors are real; do not claim a change succeeded if it failed. Do not assume a shell or operating system exists.",
                ),
            )],
        }
    }
}

impl Agent {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn turn<E: Environment>(
        &mut self,
        prompt: String,
        environment: &mut E,
        mut observe: impl FnMut(Event<'_>),
    ) -> Result<(), String> {
        self.messages.push(ChatMessage::text("user", prompt));
        for round in 0..=MAX_TOOL_ROUNDS {
            if serde_json::to_vec(&self.messages)
                .map_err(|e| e.to_string())?
                .len()
                > MAX_HISTORY_BYTES
            {
                return Err(String::from(
                    "Conversation exceeds the context limit. Use /clear to start a new conversation.",
                ));
            }
            let reply = environment.complete(&self.messages)?;
            validate_reply(&reply)?;
            if let Some(content) = reply.content.as_deref().filter(|s| !s.is_empty()) {
                observe(Event::Assistant(content));
            }
            if reply.tool_calls.is_empty() {
                self.messages.push(reply);
                return Ok(());
            }
            let calls = reply.tool_calls.clone();
            self.messages.push(reply);
            for call in &calls {
                observe(Event::ToolStarted {
                    name: &call.function.name,
                    arguments: &call.function.arguments,
                });
                let result = if round == MAX_TOOL_ROUNDS {
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
        }
        Err(String::from(
            "Tool round limit reached. All pending calls have error results; no further operations were executed.",
        ))
    }
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
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Write {
    path: String,
    content: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Edit {
    path: String,
    old_text: String,
    new_text: String,
}

fn operation(call: &ToolCall) -> Result<Operation, String> {
    let arguments = &call.function.arguments;
    match call.function.name.as_str() {
        "read" => {
            let args: Read = serde_json::from_str(arguments)
                .map_err(|e| format!("Invalid read arguments: {e}"))?;
            Ok(Operation::Read { path: args.path })
        }
        "write" => {
            let args: Write = serde_json::from_str(arguments)
                .map_err(|e| format!("Invalid write arguments: {e}"))?;
            if args.content.len() > MAX_FILE_BYTES {
                return Err(String::from("File exceeds 512 KiB"));
            }
            Ok(Operation::Write {
                path: args.path,
                content: args.content,
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
        _ => Err(format!("Unknown tool: {}", call.function.name)),
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
    if length > MAX_FILE_BYTES {
        return Err(String::from("Edited file exceeds 512 KiB"));
    }
    text.replace_range(start..start + old.len(), new);
    Ok(text)
}

pub fn tool_definitions() -> serde_json::Value {
    serde_json::json!([
        {"type":"function","function":{"name":"read","description":"Read a UTF-8 file or list a directory in the workspace.","parameters":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false}}},
        {"type":"function","function":{"name":"write","description":"Create or overwrite a UTF-8 file in the workspace (512 KiB limit). Parent directory must exist.","parameters":{"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"],"additionalProperties":false}}},
        {"type":"function","function":{"name":"edit","description":"Replace one exact occurrence of old_text with new_text. Fails if missing or ambiguous.","parameters":{"type":"object","properties":{"path":{"type":"string"},"old_text":{"type":"string"},"new_text":{"type":"string"}},"required":["path","old_text","new_text"],"additionalProperties":false}}}
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
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
        assert!(edit_text("a".into(), "a", &"x".repeat(MAX_FILE_BYTES + 1)).is_err());
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
        fn complete(&mut self, messages: &[ChatMessage]) -> Result<ChatMessage, String> {
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
        fn complete(&mut self, messages: &[ChatMessage]) -> Result<ChatMessage, String> {
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
        assert_eq!(environment.executed, MAX_TOOL_ROUNDS);
        assert_eq!(environment.completions, MAX_TOOL_ROUNDS + 1);
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
}
