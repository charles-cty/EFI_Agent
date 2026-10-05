//! Bounded Chat Completions SSE reader. Tool calls execute only after completion.
use efi_agent_core::protocol::{ChatMessage, FunctionCall, MAX_FRAME, ToolCall};
use serde_json::Value;
use std::io::{BufRead, BufReader, Read};

pub fn validate_configuration() -> Result<(), String> {
    let missing: Vec<_> = ["EFI_AGENT_API_BASE", "EFI_AGENT_API_KEY", "EFI_AGENT_MODEL"]
        .into_iter()
        .filter(|name| std::env::var(name).map_or(true, |value| value.trim().is_empty()))
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "Missing API configuration: {}. Set these variables on the launcher or relay host.",
            missing.join(", ")
        ));
    }
    let base = std::env::var("EFI_AGENT_API_BASE").map_err(|_| "Invalid API base URL")?;
    let url = reqwest::Url::parse(&base).map_err(|_| "Invalid EFI_AGENT_API_BASE URL")?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("EFI_AGENT_API_BASE must be an HTTP or HTTPS URL".into());
    }
    reasoning_effort(std::env::var("EFI_AGENT_REASONING_EFFORT").ok().as_deref())?;
    api_format(std::env::var("EFI_AGENT_API_FORMAT").ok().as_deref())?;
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ApiFormat {
    ChatCompletions,
    Responses,
}

impl ApiFormat {
    pub fn endpoint(self) -> &'static str {
        match self {
            Self::ChatCompletions => "chat/completions",
            Self::Responses => "responses",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::ChatCompletions => "Chat Completions",
            Self::Responses => "Responses",
        }
    }
}

pub fn api_format(value: Option<&str>) -> Result<ApiFormat, String> {
    match value.unwrap_or("chat_completions") {
        "chat_completions" => Ok(ApiFormat::ChatCompletions),
        "responses" => Ok(ApiFormat::Responses),
        _ => Err("EFI_AGENT_API_FORMAT must be chat_completions or responses".into()),
    }
}

/// Both API formats use the same bounded SSE framing.
pub struct SseReader<R: Read> {
    reader: BufReader<std::io::Take<R>>,
    wire_bytes: usize,
}

impl<R: Read> SseReader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader: BufReader::new(reader.take((8 * MAX_FRAME + 1) as u64)),
            wire_bytes: 0,
        }
    }

    pub fn next_data(&mut self) -> Result<String, String> {
        let mut data = String::new();
        loop {
            let mut line = Vec::new();
            let count = self
                .reader
                .read_until(b'\n', &mut line)
                .map_err(|e| e.to_string())?;
            self.wire_bytes += count;
            if self.wire_bytes > 8 * MAX_FRAME {
                return Err("Provider stream exceeds wire limit".into());
            }
            if count == 0 {
                return Err("Provider stream ended before [DONE] or response.completed".into());
            }
            let line = std::str::from_utf8(&line)
                .map_err(|_| "Provider stream is not UTF-8")?
                .trim_end_matches(['\r', '\n']);
            if line.is_empty() {
                if !data.is_empty() {
                    return Ok(data);
                }
            } else if let Some(value) = line.strip_prefix("data:") {
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(value.strip_prefix(' ').unwrap_or(value));
                if data.len() > MAX_FRAME {
                    return Err("Provider event exceeds limit".into());
                }
            }
        }
    }
}

pub fn reasoning_effort(value: Option<&str>) -> Result<&str, String> {
    let effort = value.unwrap_or("medium");
    if effort.is_empty() || effort.chars().any(char::is_whitespace) {
        return Err(
            "Reasoning effort must be one non-empty value; the provider validates support".into(),
        );
    }
    Ok(effort)
}

fn append_fragment(target: &mut String, value: &Value, bytes: &mut usize) -> Result<(), String> {
    if value.is_null() {
        return Ok(());
    }
    let fragment = value.as_str().ok_or("Provider delta field is not text")?;
    // JSON escaping is additive across string fragments. Count each fragment
    // once instead of serializing the growing message on every token.
    *bytes += serde_json::to_vec(fragment)
        .map_err(|e| e.to_string())?
        .len()
        - 2;
    if *bytes > MAX_FRAME - 2048 {
        return Err("Provider message exceeds limit".into());
    }
    target.push_str(fragment);
    Ok(())
}

/// Opaque metadata is a value, not a text fragment. Reject conflicting scalars
/// rather than guess how to combine signatures or encrypted state.
fn merge_metadata(target: &mut Value, source: &Value) -> Result<(), String> {
    if let (Some(target), Some(source)) = (target.as_object_mut(), source.as_object()) {
        for (name, value) in source {
            if let Some(previous) = target.get_mut(name) {
                merge_metadata(previous, value)?;
            } else {
                target.insert(name.clone(), value.clone());
            }
        }
        return Ok(());
    }
    if target != source {
        return Err("Provider returned conflicting opaque tool metadata".into());
    }
    Ok(())
}

fn retain_extensions(
    target: &mut serde_json::Map<String, Value>,
    source: &Value,
    known: &[&str],
    bytes: &mut usize,
) -> Result<(), String> {
    let Some(fields) = source.as_object() else {
        return Ok(());
    };
    for (name, value) in fields
        .iter()
        .filter(|(name, _)| !known.contains(&name.as_str()))
    {
        *bytes += name.len() + serde_json::to_vec(value).map_err(|e| e.to_string())?.len();
        if *bytes > MAX_FRAME - 2048 {
            return Err("Provider message exceeds limit".into());
        }
        if let Some(previous) = target.get_mut(name) {
            merge_metadata(previous, value)?;
        } else {
            target.insert(name.clone(), value.clone());
        }
    }
    Ok(())
}

#[cfg(test)]
pub fn read_stream(
    reader: impl Read,
    progress: &mut dyn FnMut(&str) -> Result<(), String>,
) -> Result<ChatMessage, String> {
    read_stream_with_usage(reader, progress, &mut |_| {})
}

pub fn read_stream_with_usage(
    reader: impl Read,
    progress: &mut dyn FnMut(&str) -> Result<(), String>,
    usage: &mut dyn FnMut(&Value),
) -> Result<ChatMessage, String> {
    let mut reader = SseReader::new(reader);
    let mut message = ChatMessage::text("assistant", String::new());
    let mut finished = false;
    let mut message_bytes = 0;
    loop {
        let data = reader.next_data()?;
        if data == "[DONE]" {
            if !finished {
                return Err("Provider stream has no finish reason".into());
            }
            if message.content.as_deref() == Some("") && !message.tool_calls.is_empty() {
                message.content = None;
            }
            if serde_json::to_vec(&message)
                .map_err(|e| e.to_string())?
                .len()
                > MAX_FRAME - 2048
            {
                return Err("Provider message exceeds limit".into());
            }
            return Ok(message);
        }
        let event: Value =
            serde_json::from_str(&data).map_err(|e| format!("Invalid provider event: {e}"))?;
        if event.get("error").is_some() {
            return Err("Provider reported a streaming error".into());
        }
        if event["usage"].is_object() {
            usage(&event["usage"]);
        }
        let choices = event["choices"]
            .as_array()
            .ok_or("Provider event has no choices")?;
        for choice in choices {
            if choice["index"].as_u64() != Some(0) {
                return Err("Provider returned an unexpected choice index".into());
            }
            if finished {
                return Err("Provider returned data after finish reason".into());
            }
            let delta = &choice["delta"];
            if let Some(role) = delta["role"].as_str()
                && role != "assistant"
            {
                return Err("Provider returned an invalid assistant role".into());
            }
            let text = delta["content"].as_str().unwrap_or_default();
            append_fragment(
                message.content.get_or_insert_default(),
                &delta["content"],
                &mut message_bytes,
            )?;
            if !delta["reasoning_content"].is_null() {
                append_fragment(
                    message.reasoning_content.get_or_insert_default(),
                    &delta["reasoning_content"],
                    &mut message_bytes,
                )?;
            }
            for (target, field) in [
                (&mut message.reasoning, "reasoning"),
                (&mut message.reasoning_summary, "reasoning_summary"),
            ] {
                if delta[field].is_string() {
                    append_fragment(
                        target.get_or_insert_default(),
                        &delta[field],
                        &mut message_bytes,
                    )?;
                }
            }
            if let Some(details) = delta["reasoning_details"].as_array() {
                for detail in details {
                    let index = detail["index"]
                        .as_u64()
                        .unwrap_or(message.reasoning_details.len() as u64);
                    if index >= 64 || index > message.reasoning_details.len() as u64 {
                        return Err("Invalid reasoning detail index".into());
                    }
                    if index == message.reasoning_details.len() as u64 {
                        message.reasoning_details.push(serde_json::json!({}));
                    }
                    let target = message.reasoning_details[index as usize]
                        .as_object_mut()
                        .ok_or("Invalid reasoning detail")?;
                    let fields = detail.as_object().ok_or("Invalid reasoning detail")?;
                    for (name, value) in fields {
                        if matches!(name.as_str(), "text" | "summary" | "data") {
                            if !value.is_string() {
                                if !value.is_null() || !target.contains_key(name) {
                                    target.insert(name.clone(), value.clone());
                                    message_bytes +=
                                        serde_json::to_vec(value).map_err(|e| e.to_string())?.len();
                                    if message_bytes > MAX_FRAME - 2048 {
                                        return Err("Provider message exceeds limit".into());
                                    }
                                }
                                continue;
                            }
                            if target.get(name).is_some_and(|previous| {
                                !previous.is_null() && !previous.is_string()
                            }) {
                                return Err("Provider changed reasoning detail field type".into());
                            }
                            let mut text = target
                                .get(name)
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_owned();
                            append_fragment(&mut text, value, &mut message_bytes)?;
                            target.insert(name.clone(), Value::String(text));
                        } else {
                            message_bytes +=
                                serde_json::to_vec(value).map_err(|e| e.to_string())?.len();
                            if message_bytes > MAX_FRAME - 2048 {
                                return Err("Provider message exceeds limit".into());
                            }
                            target.insert(name.clone(), value.clone());
                        }
                    }
                }
            }
            if !delta["tool_calls"].is_null() && !delta["tool_calls"].is_array() {
                return Err("Provider tool_calls delta is not an array".into());
            }
            if let Some(calls) = delta["tool_calls"].as_array() {
                for call in calls {
                    let index = call["index"].as_u64().ok_or("Tool delta has no index")?;
                    if index >= 8 || index > message.tool_calls.len() as u64 {
                        return Err("Provider returned an invalid tool index".into());
                    }
                    if index == message.tool_calls.len() as u64 {
                        message.tool_calls.push(ToolCall {
                            id: String::new(),
                            kind: String::new(),
                            function: FunctionCall {
                                name: String::new(),
                                arguments: String::new(),
                                extensions: Default::default(),
                            },
                            extensions: Default::default(),
                        });
                    }
                    let target = &mut message.tool_calls[index as usize];
                    retain_extensions(
                        &mut target.extensions,
                        call,
                        &["index", "id", "type", "function"],
                        &mut message_bytes,
                    )?;
                    retain_extensions(
                        &mut target.function.extensions,
                        &call["function"],
                        &["name", "arguments"],
                        &mut message_bytes,
                    )?;
                    for (field, source) in [
                        (&mut target.id, &call["id"]),
                        (&mut target.kind, &call["type"]),
                        (&mut target.function.name, &call["function"]["name"]),
                        (
                            &mut target.function.arguments,
                            &call["function"]["arguments"],
                        ),
                    ] {
                        append_fragment(field, source, &mut message_bytes)?;
                    }
                }
            }
            progress(text)?;
            if let Some(reason) = choice["finish_reason"].as_str() {
                if !matches!(reason, "stop" | "tool_calls") {
                    return Err(format!("Provider completion stopped with {reason}"));
                }
                finished = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tool_extensions_and_null_reasoning_survive_stream_and_rpc() {
        let details = serde_json::json!({"index":0,"type":"reasoning.encrypted","text":null,"summary":null,"data":"OPAQUE_419","extension":{"nested":[null,"中"]}});
        let wire = format!(
            "{}{}data: [DONE]\n\n",
            event(
                serde_json::json!({"reasoning_details":[details],"tool_calls":[
                    {"index":0,"id":"call_731","type":"function","extra_content":{"provider":{"thought_signature":"SIGNED_STATE_419"}},"function":{"name":"read","arguments":"{","signature":null}}
                ]}),
                Value::Null
            ),
            event(
                serde_json::json!({"tool_calls":[
                    {"index":0,"extra_content":{"provider":{"thought_signature":"SIGNED_STATE_419","more":{"opaque":true}}},"function":{"arguments":"}"}}
                ]}),
                Value::String("tool_calls".into())
            )
        );
        let message = read_stream(wire.as_bytes(), &mut |_| Ok(())).unwrap();
        let message: ChatMessage =
            serde_json::from_slice(&serde_json::to_vec(&message).unwrap()).unwrap();
        let next = serde_json::to_value(message).unwrap();
        assert_eq!(next["reasoning_details"][0], details);
        assert_eq!(
            next["tool_calls"][0]["extra_content"]["provider"]["thought_signature"],
            "SIGNED_STATE_419"
        );
        assert_eq!(next["tool_calls"][0]["function"]["signature"], Value::Null);
        assert_eq!(next["tool_calls"][0]["function"]["arguments"], "{}");
        assert!(next["tool_calls"][0].get("index").is_none());
    }

    #[test]
    fn opaque_conflicts_are_errors_and_null_text_does_not_erase_fragments() {
        for value in [Value::Null, serde_json::json!("changed")] {
            let wire = format!(
                "{}{}data: [DONE]\n\n",
                event(
                    serde_json::json!({"tool_calls":[{"index":0,"signature":"original"}]}),
                    Value::Null
                ),
                event(
                    serde_json::json!({"tool_calls":[{"index":0,"signature":value}]}),
                    Value::String("tool_calls".into())
                )
            );
            assert!(
                read_stream(wire.as_bytes(), &mut |_| Ok(()))
                    .unwrap_err()
                    .contains("conflicting opaque")
            );
        }
        let wire = format!(
            "{}{}{}data: [DONE]\n\n",
            event(
                serde_json::json!({"reasoning_details":[{"index":0,"text":null}]}),
                Value::Null
            ),
            event(
                serde_json::json!({"reasoning_details":[{"index":0,"text":"left 中"}]}),
                Value::Null
            ),
            event(
                serde_json::json!({"reasoning_details":[{"index":0,"text":null}]}),
                Value::String("stop".into())
            )
        );
        assert_eq!(
            read_stream(wire.as_bytes(), &mut |_| Ok(()))
                .unwrap()
                .reasoning_details[0]["text"],
            "left 中"
        );
    }
    #[test]
    fn reasoning_alias_summary_and_details_merge_without_showing_encrypted_state() {
        let wire = format!(
            "{}{}data: [DONE]\n\n",
            event(
                serde_json::json!({"content":"answer", "reasoning":"raw ", "reasoning_summary":"brief ", "reasoning_details":[
                    {"index":0,"type":"reasoning.text","text":"left "},
                    {"index":1,"type":"reasoning.encrypted","data":"opaque"}
                ]}),
                Value::Null
            ),
            event(
                serde_json::json!({"reasoning":"中", "reasoning_summary":"summary", "reasoning_details":[
                    {"index":0,"text":"right"}, {"index":1,"data":" state"}
                ]}),
                Value::String("stop".into())
            )
        );
        let mut answer = String::new();
        let message = read_stream(wire.as_bytes(), &mut |text| {
            answer.push_str(text);
            Ok(())
        })
        .unwrap();
        assert_eq!(answer, "answer");
        assert_eq!(message.reasoning.as_deref(), Some("raw 中"));
        assert_eq!(message.reasoning_summary.as_deref(), Some("brief summary"));
        assert_eq!(message.reasoning_details[0]["text"], "left right");
        assert_eq!(message.reasoning_details[1]["data"], "opaque state");
        assert!(matches!(
            api_format(None).unwrap(),
            ApiFormat::ChatCompletions
        ));
        assert!(matches!(
            api_format(Some("responses")).unwrap(),
            ApiFormat::Responses
        ));
        assert!(api_format(Some("auto")).is_err());
    }

    fn event(delta: Value, finish: Value) -> String {
        format!(
            "data: {}\r\n\r\n",
            serde_json::json!({"choices": [{"index": 0, "delta": delta, "finish_reason": finish}]})
        )
    }

    #[test]
    fn interleaved_tools_reasoning_and_text_survive_byte_reads() {
        struct Bytes(std::io::Cursor<Vec<u8>>);
        impl Read for Bytes {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                self.0.read(&mut buffer[..1])
            }
        }
        let wire = format!(
            "{}{}{}{}data: [DONE]\n\n",
            event(
                serde_json::json!({"role":"assistant","reasoning_content":"思考", "content":"left 中", "tool_calls":[
                    {"index":0,"id":"a","type":"function","function":{"name":"read","arguments":"{\"path\":"}},
                    {"index":1,"id":"b","type":"function","function":{"name":"write","arguments":"{"}}
                ]}),
                Value::Null
            ),
            event(
                serde_json::json!({"reasoning_content":" retained", "content":" right", "tool_calls":[
                    {"index":1,"function":{"arguments":"\"path\":\"new\",\"content\":\"x\"}"}},
                    {"index":0,"function":{"arguments":"\"seed\"}"}}
                ]}),
                Value::Null
            ),
            event(serde_json::json!({}), Value::String("tool_calls".into())),
            "data: {\"choices\":[]}\n\n"
        );
        let mut observed = String::new();
        let message = read_stream(
            Bytes(std::io::Cursor::new(wire.into_bytes())),
            &mut |text| {
                observed.push_str(text);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(observed, "left 中 right");
        assert_eq!(message.content.as_deref(), Some(observed.as_str()));
        assert_eq!(message.reasoning_content.as_deref(), Some("思考 retained"));
        assert_eq!(
            message.tool_calls[0].function.arguments,
            r#"{"path":"seed"}"#
        );
        assert_eq!(
            message.tool_calls[1].function.arguments,
            r#"{"path":"new","content":"x"}"#
        );
        let roundtrip: ChatMessage =
            serde_json::from_slice(&serde_json::to_vec(&message).unwrap()).unwrap();
        assert_eq!(roundtrip.reasoning_content, message.reasoning_content);
    }

    #[test]
    fn incomplete_failed_and_oversized_streams_never_complete() {
        let cases = [
            event(serde_json::json!({"content":"partial"}), Value::Null),
            "data: [DONE]\n\n".into(),
            format!(
                "{}data: [DONE]\n\n",
                event(serde_json::json!({}), Value::String("length".into()))
            ),
            "data: {\"error\":{\"message\":\"failed\"}}\n\n".into(),
            event(serde_json::json!({"tool_calls":[{"index":8}]}), Value::Null),
            format!("data: {}\n\n", "x".repeat(MAX_FRAME + 1)),
            ": heartbeat\n".repeat(MAX_FRAME),
        ];
        for wire in cases {
            assert!(read_stream(wire.as_bytes(), &mut |_| Ok(())).is_err());
        }
        let wire = event(serde_json::json!({"content":"partial"}), Value::Null);
        assert_eq!(
            read_stream(wire.as_bytes(), &mut |_| Err("disconnected".into())).unwrap_err(),
            "disconnected"
        );
    }

    #[test]
    fn reasoning_defaults_and_explicit_values() {
        assert_eq!(reasoning_effort(None).unwrap(), "medium");
        for value in ["none", "minimal", "low", "medium", "high", "xhigh"] {
            assert_eq!(reasoning_effort(Some(value)).unwrap(), value);
        }
        assert!(reasoning_effort(Some("")).is_err());
        assert_eq!(
            reasoning_effort(Some("future-budget")).unwrap(),
            "future-budget"
        );
        assert!(reasoning_effort(Some("high low")).is_err());
    }
}
