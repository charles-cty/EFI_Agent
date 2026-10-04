use alloc::{string::String, vec::Vec};
use serde::{Deserialize, Serialize};

/// A frame is a big-endian u32 length followed by UTF-8 JSON.
pub const MAX_FRAME: usize = 1024 * 1024;
pub const BRIDGE_PORT: u16 = 7420;

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    pub operation: Operation,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Operation {
    List {
        path: String,
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
    Complete {
        messages: Vec<ChatMessage>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    pub fn text(role: &str, content: String) -> Self {
        Self {
            role: role.into(),
            content: Some(content),
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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub id: u64,
    pub result: Result<String, String>,
}

pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, &'static str> {
    let body = serde_json::to_vec(value).map_err(|_| "Cannot encode JSON")?;
    if body.len() > MAX_FRAME {
        return Err("Frame exceeds limit");
    }
    let mut frame = Vec::with_capacity(body.len() + 4);
    frame.extend_from_slice(&(body.len() as u32).to_be_bytes());
    frame.extend_from_slice(&body);
    Ok(frame)
}

pub fn frame_length(header: [u8; 4]) -> Result<usize, &'static str> {
    let length = u32::from_be_bytes(header) as usize;
    if length == 0 || length > MAX_FRAME {
        return Err("Invalid frame length");
    }
    Ok(length)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_boundaries() {
        assert!(frame_length(0u32.to_be_bytes()).is_err());
        assert_eq!(frame_length(1u32.to_be_bytes()), Ok(1));
        assert_eq!(
            frame_length((MAX_FRAME as u32).to_be_bytes()),
            Ok(MAX_FRAME)
        );
        assert!(frame_length((MAX_FRAME as u32 + 1).to_be_bytes()).is_err());
        assert_eq!(frame_length(256u32.to_be_bytes()), Ok(256));
    }
}
