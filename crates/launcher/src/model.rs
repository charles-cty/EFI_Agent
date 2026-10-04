//! Bounded Chat Completions SSE reader. Tool calls execute only after completion.
use efi_agent_core::protocol::{ChatMessage, FunctionCall, MAX_FRAME, ToolCall};
use serde_json::Value;
use std::io::{BufRead, BufReader, Read};

pub fn reasoning_effort(value: Option<&str>) -> Result<&str, String> {
    match value.unwrap_or("medium") {
        effort @ ("none" | "minimal" | "low" | "medium" | "high" | "xhigh") => Ok(effort),
        _ => Err(
            "EFI_AGENT_REASONING_EFFORT must be none, minimal, low, medium, high, or xhigh".into(),
        ),
    }
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

pub fn read_stream(
    reader: impl Read,
    progress: &mut dyn FnMut(&str) -> Result<(), String>,
) -> Result<ChatMessage, String> {
    // Bound wire data as well as the assembled message. Read one byte beyond
    // the limit so exhaustion cannot be mistaken for a valid EOF.
    let mut reader = BufReader::new(reader.take((8 * MAX_FRAME + 1) as u64));
    let mut wire_bytes = 0;
    let mut data = String::new();
    let mut message = ChatMessage::text("assistant", String::new());
    let mut finished = false;
    let mut message_bytes = 0;
    loop {
        let mut line = Vec::new();
        let count = reader
            .read_until(b'\n', &mut line)
            .map_err(|e| e.to_string())?;
        wire_bytes += count;
        if wire_bytes > 8 * MAX_FRAME {
            return Err("Provider stream exceeds wire limit".into());
        }
        if count == 0 {
            return Err("Provider stream ended before [DONE]".into());
        }
        let line = std::str::from_utf8(&line).map_err(|_| "Provider stream is not UTF-8")?;
        let line = line.trim_end_matches(['\r', '\n']);
        if !line.is_empty() {
            if let Some(value) = line.strip_prefix("data:") {
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(value.strip_prefix(' ').unwrap_or(value));
                if data.len() > MAX_FRAME {
                    return Err("Provider event exceeds limit".into());
                }
            }
            continue;
        }
        if data.is_empty() {
            continue;
        }
        if data == "[DONE]" {
            if !finished {
                return Err("Provider stream has no finish reason".into());
            }
            if message.content.as_deref() == Some("") && !message.tool_calls.is_empty() {
                message.content = None;
            }
            return Ok(message);
        }
        let event: Value =
            serde_json::from_str(&data).map_err(|e| format!("Invalid provider event: {e}"))?;
        data.clear();
        if event.get("error").is_some() {
            return Err("Provider reported a streaming error".into());
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
                            },
                        });
                    }
                    let target = &mut message.tool_calls[index as usize];
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
        assert!(reasoning_effort(Some("typo")).is_err());
    }
}
