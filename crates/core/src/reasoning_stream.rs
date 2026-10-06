use alloc::{
    string::{String, ToString},
    vec::Vec,
};
// Retain Responses reasoning items by output index, without interpreting state.
use crate::protocol::MAX_MESSAGE_BYTES;
use alloc::collections::BTreeMap;
use serde_json::{Value, json};

#[derive(Default)]
pub struct ReasoningStream {
    items: BTreeMap<usize, Value>,
    parts: BTreeMap<(usize, bool, usize), String>,
}

fn index(value: &Value, limit: usize) -> Result<usize, String> {
    value
        .as_u64()
        .filter(|value| *value < limit as u64)
        .map(|value| value as usize)
        .ok_or_else(|| "Invalid Responses reasoning index".into())
}

/// Completed values take precedence. Stream events supply missing fields only.
fn fill_missing(target: &mut Value, source: &Value) {
    if let (Some(target), Some(source)) = (target.as_object_mut(), source.as_object()) {
        for (name, value) in source {
            if let Some(previous) = target.get_mut(name) {
                fill_missing(previous, value);
            } else {
                target.insert(name.clone(), value.clone());
            }
        }
    }
}

impl ReasoningStream {
    pub fn item(&mut self, event: &Value) -> Result<(), String> {
        let item = &event["item"];
        if item["type"] != "reasoning" {
            return Ok(());
        }
        let output_index = index(&event["output_index"], 256)?;
        let mut item = item.clone();
        if let Some(previous) = self.items.get(&output_index) {
            if let (Some(old), Some(new)) = (previous["id"].as_str(), item["id"].as_str())
                && old != new
            {
                return Err("Provider changed reasoning item identity".into());
            }
            fill_missing(&mut item, previous);
        }
        self.items.insert(output_index, item);
        self.check_size()
    }

    pub fn text(&mut self, event: &Value, summary: bool) -> Result<bool, String> {
        let output_index = if let Some(value) = event.get("output_index") {
            index(value, 256)?
        } else if let Some(id) = event["item_id"].as_str() {
            self.items
                .iter()
                .find(|(_, item)| item["id"].as_str() == Some(id))
                .map(|(index, _)| *index)
                .ok_or("Reasoning delta has no known item")?
        } else {
            // Some providers omit identity on display deltas. The caller may
            // use these only when a unique final reasoning item can be found.
            return Ok(false);
        };
        let part_index = index(
            &event[if summary {
                "summary_index"
            } else {
                "content_index"
            }],
            64,
        )?;
        let item = self
            .items
            .entry(output_index)
            .or_insert_with(|| json!({"type":"reasoning"}));
        if let Some(id) = event["item_id"].as_str() {
            if item["id"].as_str().is_some_and(|previous| previous != id) {
                return Err("Provider changed reasoning item identity".into());
            }
            item["id"] = json!(id);
        }
        self.parts
            .entry((output_index, summary, part_index))
            .or_default()
            .push_str(
                event["delta"]
                    .as_str()
                    .ok_or("Response reasoning delta is not text")?,
            );
        self.check_size()?;
        Ok(true)
    }

    fn check_size(&self) -> Result<(), String> {
        let item_bytes: usize = self
            .items
            .values()
            .map(|item| serde_json::to_vec(item).map(|value| value.len()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
            .into_iter()
            .sum();
        let part_bytes: usize = self.parts.values().map(String::len).sum();
        if item_bytes + part_bytes > MAX_MESSAGE_BYTES - 2048 {
            return Err("Provider reasoning state exceeds limit".into());
        }
        Ok(())
    }

    pub fn finish(mut self, response: &Value) -> Result<Value, String> {
        for ((output_index, summary, part_index), text) in self.parts {
            let item = self
                .items
                .get_mut(&output_index)
                .ok_or("Missing reasoning item")?;
            let field = if summary { "summary" } else { "content" };
            if item.get(field).is_none() {
                item[field] = json!([]);
            }
            let parts = item[field]
                .as_array_mut()
                .ok_or("Invalid reasoning parts")?;
            if part_index > parts.len() {
                return Err("Missing reasoning part before delta".into());
            }
            if part_index == parts.len() {
                parts.push(json!({"type":if summary { "summary_text" } else { "reasoning_text" }, "text":text}));
            } else if parts[part_index].get("text").is_none() {
                parts[part_index]["text"] = json!(text);
            }
        }
        let mut response = response.clone();
        let output = response["output"]
            .as_array_mut()
            .ok_or("Provider response has no output")?;
        for (output_index, item) in self.items {
            let matching = if let Some(id) = item["id"].as_str() {
                output
                    .iter()
                    .position(|candidate| candidate["id"].as_str() == Some(id))
            } else {
                output
                    .get(output_index)
                    .filter(|candidate| candidate["type"] == "reasoning")
                    .map(|_| output_index)
            };
            if let Some(position) = matching {
                fill_missing(&mut output[position], &item);
                // Fill absent text parts, but never replace a provider value.
                for field in ["summary", "content"] {
                    if let (Some(target), Some(source)) = (
                        output[position]
                            .get_mut(field)
                            .and_then(Value::as_array_mut),
                        item[field].as_array(),
                    ) {
                        for (index, part) in source.iter().enumerate() {
                            if index == target.len() {
                                target.push(part.clone());
                            } else if let Some(previous) = target.get_mut(index) {
                                fill_missing(previous, part);
                            }
                        }
                    }
                }
            } else {
                output.insert(output_index.min(output.len()), item);
            }
        }
        Ok(response)
    }
}

/// Unindexed text is safe to replay only with an unambiguous reasoning item.
pub fn retain_unindexed(response: &mut Value, text: &str, summary: bool) -> Result<(), String> {
    if text.is_empty() {
        return Ok(());
    }
    let output = response["output"]
        .as_array_mut()
        .ok_or("Provider response has no output")?;
    let field = if summary { "summary" } else { "content" };
    if output.iter().any(|item| {
        item["type"] == "reasoning"
            && item[field].as_array().is_some_and(|parts| {
                parts
                    .iter()
                    .any(|part| part["text"].as_str().is_some_and(|text| !text.is_empty()))
            })
    }) {
        return Ok(());
    }
    let positions: Vec<_> = output
        .iter()
        .enumerate()
        .filter(|(_, item)| item["type"] == "reasoning")
        .map(|(index, _)| index)
        .collect();
    if positions.len() != 1 {
        return Err("Cannot associate unindexed reasoning text with an output item".into());
    }
    let item = &mut output[positions[0]];
    if item.get(field).is_none() {
        item[field] = json!([]);
    }
    let parts = item[field]
        .as_array_mut()
        .ok_or("Invalid reasoning parts")?;
    if !parts.is_empty() {
        return Err("Cannot associate unindexed reasoning text with a content part".into());
    }
    parts
        .push(json!({"type":if summary { "summary_text" } else { "reasoning_text" }, "text":text}));
    Ok(())
}
