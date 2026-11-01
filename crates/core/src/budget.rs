//! Independent resource budgets for model requests and retained conversation data.
use alloc::{format, string::String};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Limits {
    pub request_bytes: usize,
    pub compact_at_bytes: usize,
    pub compact_target_bytes: usize,
    pub history_bytes: usize,
    pub response_message_bytes: usize,
    pub event_bytes: usize,
    pub response_wire_bytes: usize,
    pub summary_bytes: usize,
    pub ui_history_bytes: usize,
    pub context_window_tokens: Option<u64>,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            request_bytes: 4 * 1024 * 1024,
            compact_at_bytes: 3 * 1024 * 1024,
            compact_target_bytes: 2 * 1024 * 1024,
            history_bytes: 8 * 1024 * 1024,
            response_message_bytes: 4 * 1024 * 1024,
            event_bytes: 4 * 1024 * 1024,
            response_wire_bytes: 32 * 1024 * 1024,
            summary_bytes: 64 * 1024,
            ui_history_bytes: 8 * 1024 * 1024,
            context_window_tokens: None,
        }
    }
}

impl Limits {
    pub fn validate(&self) -> Result<(), String> {
        if !(4096 <= self.compact_target_bytes
            && self.compact_target_bytes < self.compact_at_bytes
            && self.compact_at_bytes < self.request_bytes
            && self.request_bytes <= self.history_bytes
            && self
                .request_bytes
                .saturating_add(self.response_message_bytes)
                <= self.history_bytes
            && self.summary_bytes > 0
            && self.summary_bytes < self.compact_target_bytes
            && self.response_message_bytes >= 4096
            && self.event_bytes >= 4096
            && self.response_wire_bytes >= self.event_bytes
            && self.response_wire_bytes >= self.response_message_bytes
            && self.ui_history_bytes >= 4096
            && self.context_window_tokens != Some(0))
        {
            return Err("Invalid model limits: require 4096 <= compact target < trigger < request, request + response message <= history, 0 < summary < target, response/event >= 4096, wire >= response/event, UI >= 4096, and positive optional token window".into());
        }
        for bytes in [
            self.history_bytes,
            self.request_bytes,
            self.response_message_bytes,
            self.event_bytes,
            self.response_wire_bytes,
            self.ui_history_bytes,
        ] {
            if bytes > 128 * 1024 * 1024 {
                return Err("Model resource limits must not exceed 128 MiB each".into());
            }
        }
        Ok(())
    }
}

pub fn exceeded(name: &str, observed: usize, limit: usize) -> String {
    format!("{name}: {observed} bytes exceeds {limit} bytes")
}

/// Length of the compact UTF-8 JSON encoding, without allocating that encoding.
pub fn json_bytes(value: &Value) -> usize {
    match value {
        Value::Null => 4,
        Value::Bool(true) => 4,
        Value::Bool(false) => 5,
        Value::Number(number) => alloc::string::ToString::to_string(number).len(),
        Value::String(text) => string_bytes(text),
        Value::Array(items) => {
            2 + items.iter().map(json_bytes).sum::<usize>() + items.len().saturating_sub(1)
        }
        Value::Object(items) => {
            2 + items
                .iter()
                .map(|(key, value)| string_bytes(key) + 1 + json_bytes(value))
                .sum::<usize>()
                + items.len().saturating_sub(1)
        }
    }
}

pub fn string_bytes(text: &str) -> usize {
    2 + text
        .bytes()
        .map(|byte| match byte {
            b'"' | b'\\' | b'\n' | b'\r' | b'\t' | 8 | 12 => 2,
            0..=31 => 6,
            _ => 1,
        })
        .sum::<usize>()
}

pub fn text_message_bytes(role: &str, content: &str) -> usize {
    2 + string_bytes("content")
        + 1
        + string_bytes(content)
        + 1
        + string_bytes("role")
        + 1
        + string_bytes(role)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    Task,
    Summary,
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use serde_json::json;

    #[test]
    fn byte_counter_matches_serializer_for_structured_random_json() {
        let controls: String = (0..=31).map(char::from).collect();
        let mut state = 0x37d4_079a_b83f_125du64;
        let words = ["", "中😀", "\"\\\n\r\t", controls.as_str(), "é/λ\u{2028}"];
        for index in 0..2048 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let word = words[index % words.len()];
            let value = json!({ word: [null, true, false, state, -(state as i64 as i128),
                word.repeat(index % 17), {"nested": [state as f64 / 37.0, -0.0, i64::MIN, u64::MAX]}] });
            assert_eq!(
                json_bytes(&value),
                serde_json::to_vec(&value).unwrap().len()
            );
            assert_eq!(string_bytes(word), serde_json::to_vec(word).unwrap().len());
        }
        for value in [json!([]), json!({}), json!(1e-200), json!(1e200)] {
            assert_eq!(json_bytes(&value), value.to_string().len());
        }
    }

    #[test]
    fn configuration_order_and_bounds_are_enforced() {
        let limits = Limits::default();
        limits.validate().unwrap();
        for field in [
            "compact_target_bytes",
            "compact_at_bytes",
            "request_bytes",
            "history_bytes",
            "response_message_bytes",
            "event_bytes",
            "response_wire_bytes",
            "summary_bytes",
            "ui_history_bytes",
        ] {
            let mut value = serde_json::to_value(&limits).unwrap();
            value[field] = json!(0);
            assert!(
                serde_json::from_value::<Limits>(value)
                    .unwrap()
                    .validate()
                    .is_err(),
                "{field}"
            );
        }
        assert!(serde_json::from_str::<Limits>(r#"{"unknown":1}"#).is_err());
        assert!(
            Limits {
                context_window_tokens: Some(0),
                ..limits
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn prompt_message_length_matches_serde_without_copying_for_admission() {
        for content in [
            String::from("plain"),
            String::from("中😀\u{0001}\n\"\\"),
            "q".repeat(1024 * 1024),
        ] {
            let message = crate::protocol::ChatMessage::text("user", content.clone());
            assert_eq!(
                text_message_bytes("user", &content),
                serde_json::to_vec(&message).unwrap().len()
            );
        }
    }
}
