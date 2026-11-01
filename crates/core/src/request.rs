//! Provider request construction shared by admission checks and network sends.
use crate::{
    agent,
    budget::{self, Limits, Purpose},
    model::ApiFormat,
    protocol::ChatMessage,
};
use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use serde_json::{Value, json};

pub fn build(
    api: ApiFormat,
    model: &str,
    messages: &[ChatMessage],
    effort: &str,
    purpose: Purpose,
) -> Result<Value, String> {
    let mut body = match api {
        ApiFormat::Responses => crate::responses::request(model, messages, effort)?,
        ApiFormat::ChatCompletions => {
            let mut values = Vec::with_capacity(messages.len());
            for message in messages {
                let mut value = serde_json::to_value(message).map_err(|error| error.to_string())?;
                value
                    .as_object_mut()
                    .expect("A chat message is an object")
                    .remove("response_items");
                values.push(value);
            }
            json!({"model": model, "messages": values, "stream": true,
                "reasoning_effort": effort, "stream_options": {"include_usage": true},
                "tools": agent::tool_definitions(), "tool_choice": "auto"})
        }
    };
    if purpose == Purpose::Summary {
        body["tools"] = json!([]);
        body["tool_choice"] = json!("none");
    }
    Ok(body)
}

pub fn bytes(
    api: ApiFormat,
    model: &str,
    messages: &[ChatMessage],
    effort: &str,
    purpose: Purpose,
) -> Result<usize, String> {
    Ok(budget::json_bytes(&build(
        api, model, messages, effort, purpose,
    )?))
}

pub fn encode(body: &Value, limits: &Limits) -> Result<Vec<u8>, String> {
    let bytes = budget::json_bytes(body);
    if bytes > limits.request_bytes {
        return Err(budget::exceeded(
            "RequestBudgetExceeded",
            bytes,
            limits.request_bytes,
        ));
    }
    serde_json::to_vec(body).map_err(|error| error.to_string())
}

pub fn history_bytes(messages: &[ChatMessage]) -> Result<usize, String> {
    let mut bytes = 2 + messages.len().saturating_sub(1);
    for message in messages {
        bytes += message.canonical_bytes()?;
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn both_formats_measure_actual_complete_body_and_admit_inclusive_boundary() {
        let messages = vec![
            ChatMessage::text("system", "rules".into()),
            ChatMessage::text("user", "\u{0001}中\\\"\n".repeat(8192)),
        ];
        for api in [ApiFormat::ChatCompletions, ApiFormat::Responses] {
            for purpose in [Purpose::Task, Purpose::Summary] {
                let body = build(api, "test-model", &messages, "medium", purpose).unwrap();
                let expected = serde_json::to_vec(&body).unwrap();
                assert_eq!(
                    bytes(api, "test-model", &messages, "medium", purpose).unwrap(),
                    expected.len()
                );
                if purpose == Purpose::Summary {
                    assert_eq!(body["tools"], json!([]));
                    assert_eq!(body["tool_choice"], "none");
                }
                for delta in [-1isize, 0, 1] {
                    let limits = Limits {
                        request_bytes: expected.len().checked_add_signed(delta).unwrap(),
                        ..Limits::default()
                    };
                    assert_eq!(encode(&body, &limits).is_ok(), delta >= 0);
                }
            }
        }
    }

    #[test]
    fn responses_retains_one_authoritative_copy_and_local_identity_is_private() {
        let raw = json!({"type":"reasoning","id":"opaque","encrypted_content":"中".repeat(10000),"signature":null});
        let mut message = ChatMessage::text("assistant", "display duplicate".repeat(10000));
        message.response_items = vec![raw.clone()];
        message.checkpoint = true;
        let expected = serde_json::to_vec(&vec![raw]).unwrap().len();
        assert_eq!(message.canonical_bytes().unwrap(), expected);
        message.retain_canonical();
        assert!(message.content.is_none());
        assert_eq!(message.canonical_bytes().unwrap(), expected);
        let body = build(
            ApiFormat::Responses,
            "m",
            &[message],
            "medium",
            Purpose::Task,
        )
        .unwrap();
        assert!(body["input"][0].get("checkpoint").is_none());
    }

    #[test]
    fn full_512k_tool_content_roundtrips_as_utf8_json_without_changing_its_size() {
        for api in [ApiFormat::ChatCompletions, ApiFormat::Responses] {
            let content = "\u{0001}".repeat(512 * 1024);
            let mut call = ChatMessage::text("assistant", String::new());
            call.tool_calls.push(crate::protocol::ToolCall {
                id: "call".into(),
                kind: "function".into(),
                function: crate::protocol::FunctionCall {
                    name: "write".into(),
                    arguments: serde_json::to_string(&json!({"path":"large","content":content}))
                        .unwrap(),
                    extensions: Default::default(),
                },
                extensions: Default::default(),
            });
            let body = build(
                api,
                "model",
                &[ChatMessage::text("system", "rules".into()), call],
                "medium",
                Purpose::Task,
            )
            .unwrap();
            let encoded = serde_json::to_vec(&body).unwrap();
            assert_eq!(encoded.len(), budget::json_bytes(&body));
            let parsed: Value = serde_json::from_slice(&encoded).unwrap();
            let arguments = if api == ApiFormat::Responses {
                parsed["input"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|item| item["type"] == "function_call")
                    .and_then(|item| item["arguments"].as_str())
                    .unwrap()
            } else {
                parsed["messages"][1]["tool_calls"][0]["function"]["arguments"]
                    .as_str()
                    .unwrap()
            };
            let arguments: Value = serde_json::from_str(arguments).unwrap();
            assert_eq!(arguments["content"].as_str().unwrap().len(), 512 * 1024);
            assert!(encoded.len() > 3 * 1024 * 1024);
        }
    }
}
