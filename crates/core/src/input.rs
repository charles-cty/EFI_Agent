use alloc::vec::Vec;

#[derive(Debug, PartialEq, Eq)]
pub enum Key {
    Character(char),
    Enter,
    Backspace,
    Escape,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    Delete,
    Newline,
    Quit,
    Resize(u16, u16),
}

/// Keeps partial UTF-8 and CSI input across serial reads.
#[derive(Default)]
pub struct Decoder {
    pending: Vec<u8>,
    paste: bool,
}

impl Decoder {
    pub fn push(&mut self, byte: u8) -> Option<Key> {
        self.pending.push(byte);
        if self.pending[0] == 27 {
            if self.pending.len() == 1 {
                return None;
            }
            if self.pending[1] != b'[' {
                self.pending.clear();
                if byte == 13 || byte == 10 {
                    return Some(Key::Newline);
                }
                return Some(Key::Escape);
            }
            if self.pending.len() < 3 {
                return None;
            }
            if !(0x40..=0x7e).contains(&byte) {
                if self.pending.len() > 32 {
                    self.pending.clear();
                }
                return None;
            }
            let result = match byte {
                b'A' => Some(Key::Up),
                b'B' => Some(Key::Down),
                b'C' => Some(Key::Right),
                b'D' => Some(Key::Left),
                b'H' => Some(Key::Home),
                b'F' => Some(Key::End),
                b'~' => match &self.pending[2..self.pending.len() - 1] {
                    b"3" => Some(Key::Delete),
                    b"1" | b"7" => Some(Key::Home),
                    b"4" | b"8" => Some(Key::End),
                    b"200" => {
                        self.paste = true;
                        None
                    }
                    b"201" => {
                        self.paste = false;
                        None
                    }
                    _ => None,
                },
                b't' => {
                    let Ok(text) = core::str::from_utf8(&self.pending[2..self.pending.len() - 1])
                    else {
                        self.pending.clear();
                        return None;
                    };
                    let mut parts = text.split(';');
                    match (parts.next(), parts.next(), parts.next(), parts.next()) {
                        (Some("8"), Some(rows), Some(cols), None) => {
                            match (cols.parse::<u16>(), rows.parse::<u16>()) {
                                (Ok(w), Ok(h)) if w > 0 && h > 0 => Some(Key::Resize(w, h)),
                                _ => None,
                            }
                        }
                        _ => None,
                    }
                }
                _ => None,
            };
            self.pending.clear();
            return result;
        }
        let result = match self.pending[0] {
            3 => Some(Key::Quit),
            10 => Some(Key::Newline),
            13 if self.paste => Some(Key::Newline),
            13 => Some(Key::Enter),
            9 if self.paste => Some(Key::Character('\t')),
            8 | 127 => Some(Key::Backspace),
            _ => match core::str::from_utf8(&self.pending) {
                Ok(text) => text
                    .chars()
                    .next()
                    .filter(|c| !c.is_control())
                    .map(Key::Character),
                Err(error) if error.error_len().is_none() && self.pending.len() < 4 => return None,
                Err(_) => None,
            },
        };
        self.pending.clear();
        if self.paste && matches!(result, Some(Key::Quit | Key::Backspace)) {
            None
        } else {
            result
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragmented_unicode_and_resize() {
        let mut decoder = Decoder::default();
        let keys: Vec<_> = "中\x1b[8;29;103t\x1b[A"
            .bytes()
            .filter_map(|b| decoder.push(b))
            .collect();
        assert_eq!(keys, [Key::Character('中'), Key::Resize(103, 29), Key::Up]);
    }

    #[test]
    fn malformed_resize_recovers_and_dimensions_are_asymmetric() {
        let mut decoder = Decoder::default();
        let input = b"\x1b[8;\xff;10tz\x1b[8;0;90t\x1b[8;14;81t";
        let keys: Vec<_> = input.iter().filter_map(|b| decoder.push(*b)).collect();
        assert_eq!(keys, [Key::Character('z'), Key::Resize(81, 14)]);
    }
    #[test]
    fn multiline_paste_does_not_submit_or_quit() {
        let mut decoder = Decoder::default();
        let keys: Vec<_> = b"\x1b[200~a\rb\n\x03\x1b[201~\r\x1b[D\x1b[3~"
            .iter()
            .filter_map(|b| decoder.push(*b))
            .collect();
        assert_eq!(
            keys,
            [
                Key::Character('a'),
                Key::Newline,
                Key::Character('b'),
                Key::Newline,
                Key::Enter,
                Key::Left,
                Key::Delete
            ]
        );
    }
}
