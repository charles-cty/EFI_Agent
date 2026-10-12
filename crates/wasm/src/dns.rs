//! Bounded DNS A queries over the cooperative TCP provider.
use crate::network::Connection;
use alloc::{boxed::Box, string::String, vec, vec::Vec};
use core::fmt;

const INVALID: i32 = 28;

pub(crate) fn hostname(name: &str) -> Result<String, i32> {
    let name = name.strip_suffix('.').unwrap_or(name);
    if name.is_empty() || name.len() > 253 {
        return Err(INVALID);
    }
    for label in name.split('.') {
        if label.is_empty()
            || label.len() > 63
            || label.starts_with('-')
            || label.ends_with('-')
            || !label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(INVALID);
        }
    }
    Ok(name.to_ascii_lowercase())
}

fn request(name: &str, id: [u8; 2]) -> Vec<u8> {
    let mut message = vec![id[0], id[1], 1, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    for label in name.split('.') {
        message.push(label.len() as u8);
        message.extend_from_slice(label.as_bytes());
    }
    message.extend_from_slice(&[0, 0, 1, 0, 1]);
    let mut frame = (message.len() as u16).to_be_bytes().to_vec();
    frame.extend(message);
    frame
}

pub(crate) struct Query {
    connection: Box<dyn Connection>,
    name: String,
    id: [u8; 2],
    request: Vec<u8>,
    sent: usize,
    response: Vec<u8>,
    connected: bool,
    pub deadline: u64,
}

impl fmt::Debug for Query {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DnsQuery")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl Query {
    pub fn new(connection: Box<dyn Connection>, name: String, id: [u8; 2], deadline: u64) -> Self {
        let request = request(&name, id);
        Self {
            connection,
            name,
            id,
            request,
            deadline,
            sent: 0,
            response: Vec::new(),
            connected: false,
        }
    }

    pub fn poll(&mut self) -> Result<Option<[u8; 4]>, i32> {
        if !self.connected {
            if self
                .connection
                .poll_connect()
                .map_err(crate::network::NetworkError::errno)?
                .is_none()
            {
                return Ok(None);
            }
            self.connected = true;
        }
        if self.sent < self.request.len() {
            let Some(count) = self
                .connection
                .write(&self.request[self.sent..])
                .map_err(crate::network::NetworkError::errno)?
            else {
                return Ok(None);
            };
            assert!(
                count <= self.request.len() - self.sent,
                "DNS write exceeds request"
            );
            if count == 0 {
                return Err(29);
            }
            self.sent += count;
            // Keep each poll bounded even if the provider accepts short writes.
            return Ok(None);
        }
        let required = if self.response.len() < 2 {
            2
        } else {
            usize::from(u16::from_be_bytes([self.response[0], self.response[1]])) + 2
        };
        if self.response.len() >= 2 && required < 14 {
            return Err(INVALID);
        }
        if self.response.len() == required {
            return answer(&self.response[2..], &self.name, self.id).map(Some);
        }
        let maximum = required - self.response.len();
        let Some(bytes) = self
            .connection
            .read(maximum)
            .map_err(crate::network::NetworkError::errno)?
        else {
            return Ok(None);
        };
        assert!(bytes.len() <= maximum, "DNS read exceeds frame");
        if bytes.is_empty() {
            return Err(INVALID);
        }
        self.response.extend(bytes);
        Ok(None)
    }
}

fn word(bytes: &[u8], offset: usize) -> Result<u16, i32> {
    let value = bytes.get(offset..offset + 2).ok_or(INVALID)?;
    Ok(u16::from_be_bytes([value[0], value[1]]))
}

/// Compression pointers must refer backwards. Bounding every traversal also
/// rejects pointer cycles and limits parsing work independently of record data.
fn name(bytes: &[u8], offset: &mut usize) -> Result<String, i32> {
    let mut cursor = *offset;
    let mut end = None;
    let mut labels = Vec::new();
    let mut length = 0;
    for _ in 0..256 {
        let size = *bytes.get(cursor).ok_or(INVALID)?;
        if size & 0xc0 == 0xc0 {
            let target = (word(bytes, cursor)? & 0x3fff) as usize;
            if target < 12 || target >= cursor {
                return Err(INVALID);
            }
            end.get_or_insert(cursor + 2);
            cursor = target;
            continue;
        }
        if size & 0xc0 != 0 {
            return Err(INVALID);
        }
        cursor += 1;
        if size == 0 {
            *offset = end.unwrap_or(cursor);
            return Ok(labels.join("."));
        }
        let label = bytes.get(cursor..cursor + size as usize).ok_or(INVALID)?;
        let label = core::str::from_utf8(label).map_err(|_| INVALID)?;
        length += label.len() + usize::from(!labels.is_empty());
        if length > 253 {
            return Err(INVALID);
        }
        labels.push(label.to_ascii_lowercase());
        cursor += size as usize;
    }
    Err(INVALID)
}

fn answer(bytes: &[u8], expected: &str, id: [u8; 2]) -> Result<[u8; 4], i32> {
    if bytes.len() < 12
        || bytes[..2] != id
        || bytes[2] & 0x80 == 0
        || bytes[2] & 0x7a != 0
        || word(bytes, 4)? != 1
    {
        return Err(INVALID);
    }
    let mut cursor = 12;
    if name(bytes, &mut cursor)? != expected
        || word(bytes, cursor)? != 1
        || word(bytes, cursor + 2)? != 1
    {
        return Err(INVALID);
    }
    cursor += 4;
    match bytes[3] & 15 {
        0 => {}
        2 => return Err(6),
        3 => return Err(44),
        _ => return Err(INVALID),
    }
    let counts = [word(bytes, 6)?, word(bytes, 8)?, word(bytes, 10)?];
    if counts.iter().map(|count| *count as usize).sum::<usize>() > 1024 {
        return Err(INVALID);
    }
    let mut records = Vec::new();
    for (section, count) in counts.into_iter().enumerate() {
        for _ in 0..count {
            let owner = name(bytes, &mut cursor)?;
            let kind = word(bytes, cursor)?;
            let class = word(bytes, cursor + 2)?;
            let size = word(bytes, cursor + 8)? as usize;
            cursor += 10;
            let end = cursor.checked_add(size).ok_or(INVALID)?;
            let data = bytes.get(cursor..end).ok_or(INVALID)?;
            if section == 0 && class == 1 {
                if kind == 1 {
                    if size != 4 {
                        return Err(INVALID);
                    }
                    records.push((owner, None, Some(data.try_into().expect("Four octets"))));
                } else if kind == 5 {
                    let mut target_cursor = cursor;
                    let target = name(bytes, &mut target_cursor)?;
                    if target_cursor != end {
                        return Err(INVALID);
                    }
                    records.push((owner, Some(target), None));
                }
            }
            cursor = end;
        }
    }
    if cursor != bytes.len() {
        return Err(INVALID);
    }
    let mut current: String = expected.into();
    let mut visited = Vec::new();
    for _ in 0..16 {
        if visited.contains(&current) {
            return Err(INVALID);
        }
        visited.push(current.clone());
        if records
            .iter()
            .any(|(owner, target, _)| owner == &current && target.is_some())
            && records
                .iter()
                .any(|(owner, _, address)| owner == &current && address.is_some())
        {
            return Err(INVALID);
        }
        if let Some((_, _, Some(address))) = records
            .iter()
            .find(|(owner, _, address)| *owner == current && address.is_some())
        {
            return Ok(*address);
        }
        let targets: Vec<_> = records
            .iter()
            .filter_map(|(owner, target, _)| {
                (owner == &current).then_some(target.as_ref()).flatten()
            })
            .collect();
        let Some(target) = targets.first() else {
            return Err(44);
        };
        if targets.iter().any(|value| value != target) {
            return Err(INVALID);
        }
        current = (*target).clone();
    }
    Err(INVALID)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(owner: &[u8], kind: u16, data: &[u8]) -> Vec<u8> {
        let mut bytes = owner.to_vec();
        bytes.extend_from_slice(&kind.to_be_bytes());
        bytes.extend_from_slice(&[0, 1, 0, 0, 0, 60]);
        bytes.extend_from_slice(&(data.len() as u16).to_be_bytes());
        bytes.extend_from_slice(data);
        bytes
    }

    fn response(records: &[Vec<u8>]) -> Vec<u8> {
        let mut bytes = request("a.example", [0x31, 0x79])[2..].to_vec();
        bytes[2] = 0x81;
        bytes[3] = 0x80;
        bytes[6..8].copy_from_slice(&(records.len() as u16).to_be_bytes());
        for record in records {
            bytes.extend(record);
        }
        bytes
    }

    #[test]
    fn cname_order_conflicts_loops_and_rdata_boundaries() {
        let a = b"\xc0\x0c";
        let b = b"\x01b\x07example\x00";
        let c = b"\x01c\x07example\x00";
        let alias = record(a, 5, b);
        let address = record(b, 1, &[203, 0, 113, 7]);
        // The A record can precede the CNAME that identifies its owner.
        for records in [
            vec![alias.clone(), address.clone()],
            vec![address.clone(), alias.clone()],
            vec![alias.clone(), alias.clone(), address.clone()],
        ] {
            assert_eq!(
                answer(&response(&records), "a.example", [0x31, 0x79]),
                Ok([203, 0, 113, 7])
            );
        }
        for records in [
            vec![alias.clone(), record(a, 5, c), address.clone()],
            vec![alias.clone(), record(a, 1, &[1, 2, 3, 4]), address.clone()],
            vec![alias.clone(), record(b, 5, a)],
            vec![record(a, 5, &[1, b'b'])],
            vec![record(a, 5, &[0, 0])],
            vec![record(a, 1, &[1, 2, 3])],
            vec![record(a, 1, &[1, 2, 3, 4, 5])],
        ] {
            assert_eq!(
                answer(&response(&records), "a.example", [0x31, 0x79]),
                Err(INVALID)
            );
        }
    }

    #[test]
    fn hostname_limits_normalization_and_invalid_labels() {
        assert_eq!(hostname("MiXeD.Example."), Ok("mixed.example".into()));
        assert!(hostname(&"a".repeat(63)).is_ok());
        assert_eq!(hostname(&"a".repeat(64)), Err(INVALID));
        let maximum = [
            "a".repeat(63),
            "b".repeat(63),
            "c".repeat(63),
            "d".repeat(61),
        ]
        .join(".");
        assert_eq!(maximum.len(), 253);
        assert!(hostname(&maximum).is_ok());
        assert_eq!(hostname(&(maximum + "e")), Err(INVALID));
        for invalid in [
            "",
            ".",
            "a..b",
            "-a.b",
            "a-.b",
            "a\0b",
            "a_b",
            "你好.example",
        ] {
            assert_eq!(hostname(invalid), Err(INVALID));
        }
    }

    #[test]
    fn compressed_answer_requires_complete_frame_matching_question_and_owner() {
        let mut message = vec![0x12, 0x73, 0x81, 0x80, 0, 1, 0, 1, 0, 0, 0, 0];
        message.extend_from_slice(b"\x01a\x07example\x00\x00\x01\x00\x01");
        let record = message.len();
        message
            .extend_from_slice(b"\xc0\x0c\x00\x01\x00\x01\x00\x00\x00\x3c\x00\x04\xcb\x00\x71\x07");
        assert_eq!(
            answer(&message, "a.example", [0x12, 0x73]),
            Ok([203, 0, 113, 7])
        );
        for end in 0..message.len() {
            assert_eq!(
                answer(&message[..end], "a.example", [0x12, 0x73]),
                Err(INVALID)
            );
        }
        assert_eq!(answer(&message, "b.example", [0x12, 0x73]), Err(INVALID));
        assert_eq!(answer(&message, "a.example", [0x12, 0x74]), Err(INVALID));
        let mut cycle = message.clone();
        cycle[record + 1] = record as u8;
        assert_eq!(answer(&cycle, "a.example", [0x12, 0x73]), Err(INVALID));
        let mut unrelated = message.clone();
        unrelated[record + 1] = 14; // owner "example" is unrelated to a.example
        assert_eq!(answer(&unrelated, "a.example", [0x12, 0x73]), Err(44));
        message.push(0);
        assert_eq!(answer(&message, "a.example", [0x12, 0x73]), Err(INVALID));
    }
}
