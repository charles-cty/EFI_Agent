use std::time::{Duration, Instant};

/// Some terminal hosts consume Ctrl+V and inject ordinary key records instead
/// of a paste event. Hold a matching clipboard prefix before any Enter reaches
/// the guest. Mismatched or slow normal typing retains its original bytes.
#[derive(Default)]
pub struct ClipboardInput {
    expected: Vec<u8>,
    buffered: Vec<u8>,
    last: Option<Instant>,
}

pub enum Input {
    Pending,
    Keys(Vec<u8>),
    Paste(String),
}

impl ClipboardInput {
    pub fn pending(&self) -> bool {
        !self.buffered.is_empty()
    }

    pub fn expired(&mut self, now: Instant) -> Option<Input> {
        if self
            .last
            .is_some_and(|last| now.duration_since(last) >= Duration::from_millis(150))
        {
            self.expected.clear();
            self.last = None;
            Some(self.drain())
        } else {
            None
        }
    }

    pub fn push(&mut self, bytes: &[u8], clipboard: Option<String>, now: Instant) -> Input {
        if self.buffered.is_empty() {
            let Some(text) = clipboard else {
                return Input::Keys(bytes.to_vec());
            };
            let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
            if !normalized.contains('\n') || normalized.len() > 65536 {
                return Input::Keys(bytes.to_vec());
            }
            self.expected = normalized.replace('\n', "\r").into_bytes();
        }
        let matched_newline = self.buffered.contains(&b'\r');
        self.buffered.extend_from_slice(bytes);
        self.last = Some(now);
        if self.expected == self.buffered {
            self.expected.clear();
            self.last = None;
            return Input::Paste(
                String::from_utf8(std::mem::take(&mut self.buffered))
                    .expect("Matched UTF-8 clipboard"),
            );
        }
        if !self.expected.starts_with(&self.buffered) {
            self.expected.clear();
            self.last = None;
            if matched_newline {
                self.drain()
            } else {
                Input::Keys(std::mem::take(&mut self.buffered))
            }
        } else {
            Input::Pending
        }
    }

    fn drain(&mut self) -> Input {
        let bytes = std::mem::take(&mut self.buffered);
        // Once an injected newline matched the clipboard, never replay it as
        // a submission, even if later records are delayed or transformed.
        if bytes.contains(&b'\r') {
            Input::Paste(String::from_utf8_lossy(&bytes).into_owned())
        } else {
            Input::Keys(bytes)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn injected_clipboard_newlines_never_escape_before_paste_is_complete() {
        let mut input = ClipboardInput::default();
        let now = Instant::now();
        let text = "first 中\r\nsecond\r\n";
        assert!(
            matches!(input.push(b"\r", Some(text.into()), now), Input::Keys(value) if value == b"\r")
        );
        let bytes = text.replace("\r\n", "\r").into_bytes();
        for (index, byte) in bytes.iter().enumerate() {
            let result = input.push(&[*byte], Some(text.into()), now);
            if index + 1 == bytes.len() {
                assert!(matches!(result, Input::Paste(value) if value == "first 中\rsecond\r"));
            } else {
                assert!(matches!(result, Input::Pending));
            }
        }
        assert!(
            matches!(input.push(b"x", Some(text.into()), now), Input::Keys(value) if value == b"x")
        );
        assert!(matches!(
            input.push(b"f", Some(text.into()), now),
            Input::Pending
        ));
        assert!(
            matches!(input.expired(now + Duration::from_millis(151)), Some(Input::Keys(value)) if value == b"f")
        );
        assert!(matches!(
            input.push(b"f", Some(text.into()), now),
            Input::Pending
        ));
        assert!(matches!(input.push(b"z", None, now), Input::Keys(value) if value == b"fz"));
        input.push("first 中\r".as_bytes(), Some(text.into()), now);
        assert!(
            matches!(input.expired(now + Duration::from_millis(151)), Some(Input::Paste(value)) if value == "first 中\r")
        );
        input.push("first 中\r".as_bytes(), Some(text.into()), now);
        assert!(
            matches!(input.push(b"X", None, now), Input::Paste(value) if value == "first 中\rX")
        );
    }
}
