use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
// Responses API adapter. Only a completed response can authorize tool calls.
use crate::model::{Read, SseReader};
use crate::reasoning_stream::{ReasoningStream, retain_unindexed};
use crate::{
    agent,
    protocol::{ChatMessage, FunctionCall, MAX_MESSAGE_BYTES, ToolCall},
};
use serde_json::{Value, json};

pub fn request(model: &str, messages: &[ChatMessage], effort: &str) -> Result<Value, String> {
    let mut input = Vec::new();
    for message in messages {
        if message.role == "assistant" && !message.response_items.is_empty() {
            input.extend(message.response_items.iter().cloned());
        } else if message.role == "tool" {
            input.push(json!({"type":"function_call_output", "call_id":message.tool_call_id.as_deref().ok_or("Tool result has no call ID")?, "output":message.content.as_deref().unwrap_or_default()}));
        } else {
            if let Some(content) = &message.content {
                input.push(json!({"role":message.role, "content":content}));
            }
            for call in &message.tool_calls {
                input.push(json!({"type":"function_call", "call_id":call.id, "name":call.function.name, "arguments":call.function.arguments}));
            }
        }
    }
    let tools: Vec<_> = agent::tool_definitions()
        .as_array()
        .ok_or("Invalid tool definitions")?
        .iter()
        .map(|tool| {
            let mut function = tool["function"].clone();
            function["type"] = json!("function");
            function
        })
        .collect();
    Ok(
        json!({"model":model, "input":input, "tools":tools, "tool_choice":"auto", "stream":true,
        "store":false, "include":["reasoning.encrypted_content"], "reasoning":{"effort":effort,"summary":"auto"}}),
    )
}

fn text_parts(parts: &Value, kinds: &[&str], field: &str) -> String {
    parts
        .as_array()
        .into_iter()
        .flatten()
        .filter(|part| {
            part["type"]
                .as_str()
                .is_some_and(|kind| kinds.contains(&kind))
        })
        .filter_map(|part| part[field].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn completed(response: &Value) -> Result<ChatMessage, String> {
    if response["status"].as_str() != Some("completed") {
        return Err("Provider response was not completed".into());
    }
    let output = response["output"]
        .as_array()
        .ok_or("Provider response has no output")?;
    let mut message = ChatMessage::text("assistant", String::new());
    let mut answers = Vec::new();
    let mut summaries = Vec::new();
    let mut reasoning = Vec::new();
    for item in output {
        match item["type"].as_str() {
            Some("message") => {
                if item["role"].as_str() != Some("assistant") {
                    return Err("Invalid response message role".into());
                }
                let text = text_parts(&item["content"], &["output_text"], "text");
                if !text.is_empty() {
                    answers.push(text);
                }
                let refusal = text_parts(&item["content"], &["refusal"], "refusal");
                if !refusal.is_empty() {
                    answers.push(refusal);
                }
            }
            Some("reasoning") => {
                let summary = text_parts(&item["summary"], &["summary_text"], "text");
                if !summary.is_empty() {
                    summaries.push(summary);
                }
                let text = text_parts(&item["content"], &["reasoning_text", "text"], "text");
                if !text.is_empty() {
                    reasoning.push(text);
                }
            }
            Some("function_call") => {
                if message.tool_calls.len() >= 8 {
                    return Err("Provider returned more than eight tool calls".into());
                }
                message.tool_calls.push(ToolCall {
                    id: item["call_id"]
                        .as_str()
                        .ok_or("Function call has no call_id")?
                        .into(),
                    kind: "function".into(),
                    function: FunctionCall {
                        name: item["name"]
                            .as_str()
                            .ok_or("Function call has no name")?
                            .into(),
                        arguments: item["arguments"]
                            .as_str()
                            .ok_or("Function call has no arguments")?
                            .into(),
                        extensions: Default::default(),
                    },
                    extensions: Default::default(),
                });
            }
            _ => return Err("Unsupported Responses output item".into()),
        }
    }
    message.content = if answers.is_empty() {
        None
    } else {
        Some(answers.join("\n"))
    };
    if !reasoning.is_empty() {
        message.reasoning_content = Some(reasoning.join("\n"));
    }
    if !summaries.is_empty() {
        message.reasoning_summary = Some(summaries.join("\n"));
    }
    message.response_items = output.clone();
    if serde_json::to_vec(&message)
        .map_err(|e| e.to_string())?
        .len()
        > MAX_MESSAGE_BYTES - 2048
    {
        return Err("Provider message exceeds limit".into());
    }
    Ok(message)
}

/// Normalize only provider-supplied fields for the common usage display.
pub fn usage(value: &Value) -> Value {
    let mut result = json!({});
    for (source, target) in [
        ("input_tokens", "prompt_tokens"),
        ("output_tokens", "completion_tokens"),
        ("total_tokens", "total_tokens"),
        ("input_tokens_details", "prompt_tokens_details"),
        ("output_tokens_details", "completion_tokens_details"),
    ] {
        if let Some(field) = value.get(source) {
            result[target] = field.clone();
        }
    }
    result
}

pub fn read_stream(
    reader: impl Read,
    progress: &mut dyn FnMut(&str) -> Result<(), String>,
    report_usage: &mut dyn FnMut(&Value),
) -> Result<ChatMessage, String> {
    let mut reader = SseReader::new(reader);
    let mut streamed_bytes = 0;
    let mut summaries = String::new();
    let mut reasoning = String::new();
    let mut state = ReasoningStream::default();
    loop {
        let data = reader.next_data()?;
        let event: Value =
            serde_json::from_str(&data).map_err(|e| format!("Invalid Responses event: {e}"))?;
        match event["type"].as_str() {
            Some("response.output_item.added" | "response.output_item.done") => {
                state.item(&event)?
            }
            Some("response.reasoning_summary_text.delta" | "response.reasoning_text.delta") => {
                let text = event["delta"]
                    .as_str()
                    .ok_or("Response reasoning delta is not text")?;
                streamed_bytes += text.len();
                if streamed_bytes > MAX_MESSAGE_BYTES - 2048 {
                    return Err("Provider message exceeds limit".into());
                }
                let summary = event["type"] == "response.reasoning_summary_text.delta";
                if !state.text(&event, summary)? {
                    if summary {
                        summaries.push_str(text);
                    } else {
                        reasoning.push_str(text);
                    }
                }
            }
            Some("response.output_text.delta" | "response.refusal.delta") => {
                let text = event["delta"]
                    .as_str()
                    .ok_or("Response text delta is not text")?;
                streamed_bytes += text.len();
                if streamed_bytes > MAX_MESSAGE_BYTES - 2048 {
                    return Err("Provider message exceeds limit".into());
                }
                progress(text)?;
            }
            Some("response.completed" | "response.incomplete" | "response.failed") => {
                let response = &event["response"];
                if response["usage"].is_object() {
                    report_usage(&usage(&response["usage"]));
                }
                if event["type"] != "response.completed" {
                    return Err(format!(
                        "Provider Responses stream stopped with {}",
                        event["type"]
                    ));
                }
                let mut response = state.finish(response)?;
                retain_unindexed(&mut response, &summaries, true)?;
                retain_unindexed(&mut response, &reasoning, false)?;
                let message = completed(&response)?;
                if serde_json::to_vec(&message)
                    .map_err(|e| e.to_string())?
                    .len()
                    > MAX_MESSAGE_BYTES - 2048
                {
                    return Err("Provider message exceeds limit".into());
                }
                return Ok(message);
            }
            Some("error") => return Err("Provider reported a Responses streaming error".into()),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    fn streamed(events: &[Value]) -> Result<ChatMessage, String> {
        let wire: String = events
            .iter()
            .map(|event| format!("data: {event}\n\n"))
            .collect();
        read_stream(wire.as_bytes(), &mut |_| Ok(()), &mut |_| {})
    }

    #[test]
    fn stream_reasoning_reconstruction_matches_independent_subset_oracle() {
        // The expected sequence is fixed independently of the reconstruction.
        // Vary which reasoning items the final response omits and reverse the
        // arrival order of full snapshots. Serialization round trips must preserve it.
        let expected = json!([
            {"type":"reasoning","id":"rs_left","summary":[],"encrypted_content":"LEFT","signature":null},
            {"type":"function_call","id":"fc_a","call_id":"call_a","name":"read","arguments":"{}"},
            {"type":"reasoning","id":"rs_middle","summary":[{"type":"summary_text","text":"中"}],"encrypted_content":"MIDDLE"},
            {"type":"function_call","id":"fc_b","call_id":"call_b","name":"edit","arguments":"{}"},
            {"type":"reasoning","id":"rs_right","summary":[],"encrypted_content":"RIGHT","nested":{"a":[null,83]}}
        ]);
        let expected = expected.as_array().unwrap();
        for omitted in 0..8 {
            for reverse in [false, true] {
                let final_output: Vec<_> = expected
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| index % 2 != 0 || omitted & (1 << (index / 2)) == 0)
                    .map(|(_, item)| item.clone())
                    .collect();
                let mut snapshots: Vec<_> = expected
                    .iter()
                    .enumerate()
                    .filter(|(_, item)| item["type"] == "reasoning")
                    .collect();
                if reverse {
                    snapshots.reverse();
                }
                let mut events: Vec<_> = snapshots.iter().map(|(index, item)| json!({"type":"response.output_item.done","output_index":index,"item":item})).collect();
                events.push(json!({"type":"response.completed","response":{"status":"completed","output":final_output}}));
                let message = streamed(&events).unwrap();
                let message: ChatMessage =
                    serde_json::from_slice(&serde_json::to_vec(&message).unwrap()).unwrap();
                assert_eq!(
                    request("model", &[message], "medium").unwrap()["input"],
                    json!(expected),
                    "omitted={omitted}, reverse={reverse}"
                );
            }
        }
    }

    #[test]
    fn stream_only_reasoning_is_replayed_in_order_and_final_items_are_not_duplicated() {
        let before = json!({"type":"reasoning","id":"rs_before","summary":[],"encrypted_content":"opaque-before","signature":null});
        let after = json!({"type":"reasoning","id":"rs_after","summary":[],"encrypted_content":"opaque-after","extensions":{"nested":[null,"中"]}});
        let call = json!({"type":"function_call","id":"fc_731","call_id":"call_731","name":"read","arguments":"{}"});
        for final_output in [json!([call]), json!([before, call, after])] {
            let message = streamed(&[
                json!({"type":"response.output_item.added","output_index":0,"item":{"type":"reasoning","id":"rs_before","summary":[]}}),
                json!({"type":"response.output_item.done","output_index":0,"item":before}),
                json!({"type":"response.output_item.done","output_index":2,"item":after}),
                json!({"type":"response.completed","response":{"status":"completed","output":final_output}}),
            ]).unwrap();
            let message: ChatMessage =
                serde_json::from_slice(&serde_json::to_vec(&message).unwrap()).unwrap();
            let body = request("model", &[message], "medium").unwrap();
            assert_eq!(body["input"], json!([before, call, after]));
        }
    }

    #[test]
    fn indexed_text_replays_at_its_part_and_final_snapshots_take_precedence() {
        let item =
            json!({"type":"reasoning","id":"rs_731","encrypted_content":"opaque","summary":[]});
        let mut final_response = response();
        final_response["output"][0] = item.clone();
        let events = vec![
            json!({"type":"response.output_item.added","output_index":0,"item":item}),
            json!({"type":"response.reasoning_text.delta","output_index":0,"item_id":"rs_731","content_index":0,"delta":"left 中"}),
            json!({"type":"response.reasoning_text.delta","output_index":0,"item_id":"rs_731","content_index":0,"delta":" right"}),
            json!({"type":"response.reasoning_summary_text.delta","output_index":0,"item_id":"rs_731","summary_index":0,"delta":"brief"}),
            json!({"type":"response.completed","response":final_response}),
        ];
        let message = streamed(&events).unwrap();
        let body = request("model", &[message], "medium").unwrap();
        assert_eq!(body["input"][0]["content"][0]["text"], "left 中 right");
        assert_eq!(body["input"][0]["summary"][0]["text"], "brief");
        let mut events = events;
        events.last_mut().unwrap()["response"]["output"][0]["content"] =
            json!([{"type":"reasoning_text","text":"authoritative","signature":null}]);
        let message = streamed(&events).unwrap();
        assert_eq!(
            message.response_items[0]["content"],
            json!([{"type":"reasoning_text","text":"authoritative","signature":null}])
        );
    }

    #[test]
    fn unindexed_text_requires_an_unambiguous_item_and_never_displays_without_replay() {
        let reasoning =
            json!({"type":"reasoning","id":"rs_419","summary":[],"encrypted_content":"opaque"});
        let mut final_response = response();
        final_response["output"].as_array_mut().unwrap().remove(0);
        let events = vec![
            json!({"type":"response.output_item.done","output_index":0,"item":reasoning}),
            json!({"type":"response.reasoning_text.delta","delta":"stream-only text"}),
            json!({"type":"response.completed","response":final_response}),
        ];
        let message = streamed(&events).unwrap();
        assert_eq!(
            message.reasoning_content.as_deref(),
            Some("stream-only text")
        );
        assert_eq!(
            request("model", &[message], "medium").unwrap()["input"][0]["content"][0]["text"],
            "stream-only text"
        );
        assert!(
            streamed(&events[1..])
                .unwrap_err()
                .contains("Cannot associate")
        );
        let partial = &events[..2];
        assert!(streamed(partial).is_err());
    }

    fn response() -> Value {
        json!({"status":"completed", "output":[
            {"id":"rs_731", "type":"reasoning", "encrypted_content":"OPAQUE_419", "summary":[{"type":"summary_text","text":"Summary 中"}], "content":[{"type":"reasoning_text","text":"Provider reasoning"}]},
            {"type":"function_call", "call_id":"call_83", "name":"read", "arguments":"{\"path\":\"seed.txt\"}"}
        ], "usage":{"input_tokens":731,"output_tokens":419,"total_tokens":1150,"input_tokens_details":{"cached_tokens":0},"output_tokens_details":{"reasoning_tokens":19}}})
    }

    #[test]
    fn responses_preserve_reasoning_and_call_ids_across_tool_rounds() {
        let message = completed(&response()).unwrap();
        assert_eq!(message.reasoning_summary.as_deref(), Some("Summary 中"));
        assert_eq!(
            message.reasoning_content.as_deref(),
            Some("Provider reasoning")
        );
        let mut result = ChatMessage::text("tool", "seed content".into());
        result.tool_call_id = Some("call_83".into());
        let body = request("test-model", &[message, result], "future-budget").unwrap();
        assert_eq!(body["input"][0]["encrypted_content"], "OPAQUE_419");
        assert_eq!(body["input"][1]["call_id"], "call_83");
        assert_eq!(body["input"][2]["type"], "function_call_output");
        assert_eq!(body["input"][2]["call_id"], "call_83");
        assert_eq!(body["reasoning"]["summary"], "auto");
        assert_eq!(body["tools"][0]["type"], "function");
        assert_eq!(
            usage(&response()["usage"])["prompt_tokens_details"]["cached_tokens"],
            0
        );
        assert!(
            usage(&json!({"input_tokens":83}))
                .get("completion_tokens")
                .is_none()
        );
    }

    #[test]
    fn responses_stream_requires_completion_and_does_not_execute_partial_tools() {
        let completed = json!({"type":"response.completed", "response":response()});
        let wire = format!(
            "data: {{\"type\":\"response.output_text.delta\",\"delta\":\"中 answer\"}}\n\ndata: {completed}\n\n"
        );
        let mut text = String::new();
        let mut stats = None;
        let message = read_stream(
            wire.as_bytes(),
            &mut |delta| {
                text.push_str(delta);
                Ok(())
            },
            &mut |usage| stats = Some(usage.clone()),
        )
        .unwrap();
        assert_eq!(text, "中 answer");
        assert_eq!(message.tool_calls[0].id, "call_83");
        assert_eq!(stats.unwrap()["prompt_tokens"], 731);
        for kind in ["response.incomplete", "response.failed"] {
            let wire = format!("data: {}\n\n", json!({"type":kind,"response":response()}));
            assert!(read_stream(wire.as_bytes(), &mut |_| Ok(()), &mut |_| {}).is_err());
        }
        assert!(
            read_stream(
                b"data: {\"type\":\"response.function_call_arguments.delta\",\"delta\":\"{}\"}\n\n"
                    .as_slice(),
                &mut |_| Ok(()),
                &mut |_| {}
            )
            .is_err()
        );
    }
}
