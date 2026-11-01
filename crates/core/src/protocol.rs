use alloc::{string::String, vec::Vec};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Operation {
    Shell {
        command: String,
    },
    Js {
        code: String,
    },
    ResetJs,
    Status,
    Effort {
        value: Option<String>,
    },
    Read {
        path: String,
    },
    Write {
        path: String,
        content: String,
    },
    Edit {
        path: String,
        old_text: String,
        new_text: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    pub title: String,
    pub collapsible: bool,
    pub expanded: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Local identity only; never sent to a provider.
    #[serde(skip)]
    pub checkpoint: bool,
    pub role: String,
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasoning_details: Vec<serde_json::Value>,
    /// Responses output items retain opaque reasoning state for later rounds.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub response_items: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    pub fn canonical_bytes(&self) -> Result<usize, String> {
        if !self.response_items.is_empty() {
            return Ok(2
                + self
                    .response_items
                    .iter()
                    .map(crate::budget::json_bytes)
                    .sum::<usize>()
                + self.response_items.len().saturating_sub(1));
        }
        serde_json::to_value(self)
            .map(|value| crate::budget::json_bytes(&value))
            .map_err(|error| alloc::string::ToString::to_string(&error))
    }

    /// Keep the authoritative Responses items after display and dispatch.
    pub fn retain_canonical(&mut self) {
        if !self.response_items.is_empty() {
            self.content = None;
            self.reasoning_content = None;
            self.reasoning = None;
            self.reasoning_summary = None;
            self.reasoning_details.clear();
            self.tool_calls.clear();
        }
    }

    pub fn text(role: &str, content: String) -> Self {
        Self {
            checkpoint: false,
            role: role.into(),
            content: Some(content),
            reasoning_content: None,
            reasoning: None,
            reasoning_summary: None,
            reasoning_details: Vec::new(),
            response_items: Vec::new(),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: FunctionCall,
    #[serde(default, flatten)]
    pub extensions: serde_json::Map<String, serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
    #[serde(default, flatten)]
    pub extensions: serde_json::Map<String, serde_json::Value>,
}
