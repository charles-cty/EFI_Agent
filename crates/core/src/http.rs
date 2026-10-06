//! Bounded HTTP/1.1 response framing for a direct provider connection.
use crate::model::Read;
use alloc::{format, string::String, vec::Vec};

#[derive(Clone, Debug)]
pub struct Url {
    pub tls: bool,
    pub host: String,
    pub port: u16,
    pub authority: String,
    pub path: String,
}
impl Url {
    pub fn parse(base: &str) -> Result<Self, String> {
        let (tls, rest) = if let Some(value) = base.strip_prefix("https://") {
            (true, value)
        } else if let Some(value) = base.strip_prefix("http://") {
            (false, value)
        } else {
            return Err("API base must be an HTTP or HTTPS URL".into());
        };
        if !rest.is_ascii()
            || rest.chars().any(|c| c.is_control() || c.is_whitespace())
            || rest.contains(['?', '#', '@', '\\'])
        {
            return Err("API base contains unsupported URL characters".into());
        }
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        let (host, port) = if let Some((host, port)) = authority.split_once(':') {
            (host, port.parse::<u16>().map_err(|_| "Invalid API port")?)
        } else {
            (authority, if tls { 443 } else { 80 })
        };
        if host.len() > 253
            || host.is_empty()
            || port == 0
            || host.split('.').any(|part| {
                part.is_empty()
                    || part.len() > 63
                    || part.starts_with('-')
                    || part.ends_with('-')
                    || !part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            })
        {
            return Err("API host must be an IPv4 address or DNS name".into());
        }
        Ok(Self {
            tls,
            host: host.into(),
            port,
            authority: authority.into(),
            path: format!("/{}", path.trim_end_matches('/')),
        })
    }
    pub fn endpoint(&self, endpoint: &str) -> String {
        format!("{}/{}", self.path.trim_end_matches('/'), endpoint)
    }
}

pub struct Body<R: Read> {
    source: R,
    buffer: [u8; 4096],
    offset: usize,
    length: usize,
    remaining: Option<usize>,
    chunked: bool,
    chunk_end: bool,
    done: bool,
    bytes: usize,
    pub status: u16,
    pub content_type: String,
}
impl<R: Read> Body<R> {
    pub fn new(source: R) -> Result<Self, String> {
        let mut body = Self {
            source,
            buffer: [0; 4096],
            offset: 0,
            length: 0,
            remaining: None,
            chunked: false,
            chunk_end: false,
            done: false,
            bytes: 0,
            status: 0,
            content_type: String::new(),
        };
        let status = body.line(8192)?;
        let mut parts = status.split_whitespace();
        if !matches!(parts.next(), Some("HTTP/1.1" | "HTTP/1.0")) {
            return Err("Invalid provider HTTP status line".into());
        }
        body.status = parts
            .next()
            .ok_or("Missing HTTP status")?
            .parse()
            .map_err(|_| "Invalid HTTP status")?;
        let mut header_bytes = status.len();
        let mut length = None;
        let mut transfer = None;
        loop {
            let line = body.line(8192)?;
            header_bytes += line.len() + 2;
            if header_bytes > 32 * 1024 {
                return Err("HTTP headers exceed limit".into());
            }
            if line.is_empty() {
                break;
            }
            let (name, value) = line.split_once(':').ok_or("Invalid HTTP header")?;
            let value = value.trim();
            if name.eq_ignore_ascii_case("content-length") {
                if length.is_some() {
                    return Err("Duplicate Content-Length".into());
                }
                length = Some(
                    value
                        .parse::<usize>()
                        .map_err(|_| "Invalid Content-Length")?,
                );
            } else if name.eq_ignore_ascii_case("transfer-encoding") {
                if transfer.is_some() {
                    return Err("Duplicate Transfer-Encoding".into());
                }
                transfer = Some(value.to_ascii_lowercase());
            } else if name.eq_ignore_ascii_case("content-encoding")
                && !value.eq_ignore_ascii_case("identity")
            {
                return Err("Compressed provider responses are unsupported".into());
            } else if name.eq_ignore_ascii_case("content-type") {
                body.content_type = value.into();
            }
        }
        if transfer.is_some() && length.is_some() {
            return Err("Conflicting HTTP body framing".into());
        }
        if let Some(transfer) = transfer {
            if transfer != "chunked" {
                return Err("Unsupported HTTP transfer encoding".into());
            }
            body.chunked = true;
            body.remaining = Some(0);
        } else {
            body.remaining = length;
        }
        Ok(body)
    }
    fn byte(&mut self) -> Result<Option<u8>, String> {
        if self.offset == self.length {
            self.length = self.source.read(&mut self.buffer)?;
            self.offset = 0;
            if self.length > self.buffer.len() {
                return Err("Invalid reader byte count".into());
            }
            if self.length == 0 {
                return Ok(None);
            }
        }
        let byte = self.buffer[self.offset];
        self.offset += 1;
        Ok(Some(byte))
    }
    fn line(&mut self, limit: usize) -> Result<String, String> {
        let mut line = Vec::new();
        loop {
            let byte = self.byte()?.ok_or("Truncated HTTP line")?;
            if byte == b'\n' {
                if line.pop() != Some(b'\r') {
                    return Err("HTTP line requires CRLF".into());
                }
                return String::from_utf8(line).map_err(|_| "HTTP header is not UTF-8".into());
            }
            line.push(byte);
            if line.len() > limit {
                return Err("HTTP line exceeds limit".into());
            }
        }
    }
}
impl<R: Read> Read for Body<R> {
    fn read(&mut self, output: &mut [u8]) -> Result<usize, String> {
        if output.is_empty() || self.done {
            return Ok(0);
        }
        if self.remaining == Some(0) {
            if !self.chunked {
                self.done = true;
                return Ok(0);
            }
            if self.chunk_end && !self.line(2)?.is_empty() {
                return Err("Invalid HTTP chunk terminator".into());
            }
            let size = self.line(1024)?;
            let size = size.split(';').next().unwrap_or_default();
            let count = usize::from_str_radix(size, 16).map_err(|_| "Invalid HTTP chunk size")?;
            if count > 8 * crate::protocol::MAX_MESSAGE_BYTES {
                return Err("HTTP chunk exceeds limit".into());
            }
            self.remaining = Some(count);
            self.chunk_end = true;
            if count == 0 {
                let mut trailer_bytes = 0;
                loop {
                    let line = self.line(8192)?;
                    trailer_bytes += line.len() + 2;
                    if trailer_bytes > 32 * 1024 {
                        return Err("HTTP trailers exceed limit".into());
                    }
                    if line.is_empty() {
                        break;
                    }
                }
                self.done = true;
                return Ok(0);
            }
        }
        // Return available bytes promptly so progress appears before the next
        // provider chunk arrives. Never wait to fill the caller's whole buffer.
        let first = match self.byte()? {
            Some(byte) => byte,
            None if self.remaining.is_none() => {
                self.done = true;
                return Ok(0);
            }
            None => return Err("Truncated HTTP body".into()),
        };
        output[0] = first;
        let available = self.length - self.offset;
        let count = (available + 1)
            .min(output.len())
            .min(self.remaining.unwrap_or(usize::MAX));
        output[1..count].copy_from_slice(&self.buffer[self.offset..self.offset + count - 1]);
        self.offset += count - 1;
        if let Some(remaining) = &mut self.remaining {
            *remaining -= count;
        }
        self.bytes += count;
        if self.bytes > 8 * crate::protocol::MAX_MESSAGE_BYTES {
            return Err("Provider response exceeds wire limit".into());
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fragmented<'a>(&'a [u8], usize);
    impl Read for Fragmented<'_> {
        fn read(&mut self, out: &mut [u8]) -> Result<usize, String> {
            let count = out.len().min(self.0.len()).min(self.1);
            out[..count].copy_from_slice(&self.0[..count]);
            self.0 = &self.0[count..];
            Ok(count)
        }
    }
    #[test]
    fn chunks_and_lengths_have_identical_content_at_every_fragment_size() {
        let cases: &[&[u8]]=&[b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nab\r\n3;ext=x\r\ncde\r\n0\r\nX: y\r\n\r\n", b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nabcde"];
        for wire in cases {
            for size in 1..=wire.len() {
                let mut body = Body::new(Fragmented(wire, size)).unwrap();
                let mut actual = Vec::new();
                let mut buf = [0; 3];
                loop {
                    let n = body.read(&mut buf).unwrap();
                    if n == 0 {
                        break;
                    }
                    actual.extend_from_slice(&buf[..n]);
                }
                assert_eq!(actual, b"abcde");
            }
        }
    }
    #[test]
    fn truncation_and_ambiguous_headers_are_errors() {
        for wire in [
            "HTTP/1.1 200 OK\r\nContent-Length: 6\r\n\r\nabcde",
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n6\r\nabcde",
        ] {
            let mut body = Body::new(wire.as_bytes()).unwrap();
            let mut buf = [0; 9];
            assert!(loop {
                match body.read(&mut buf) {
                    Ok(0) => break false,
                    Ok(_) => {}
                    Err(_) => break true,
                }
            });
        }
        assert!(
            Body::new(
                b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\nTransfer-Encoding: chunked\r\n\r\nx"
                    .as_slice()
            )
            .is_err()
        );
    }
}
