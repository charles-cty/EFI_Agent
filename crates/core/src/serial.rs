//! Private guest/launcher readiness marker carried in the serial byte stream.
use alloc::vec::Vec;

pub const READY: &[u8] = b"\x1b]777;efi-agent;ready\x07";

#[derive(Default)]
pub struct Decoder {
    pending: Vec<u8>,
}

impl Decoder {
    /// Remove readiness markers while preserving all other terminal bytes.
    /// A prefix may be split across arbitrary socket reads.
    pub fn push(&mut self, bytes: &[u8]) -> (Vec<u8>, usize) {
        let mut output = Vec::with_capacity(bytes.len());
        let mut ready = 0;
        for byte in bytes {
            self.pending.push(*byte);
            while !READY.starts_with(&self.pending) {
                output.push(self.pending.remove(0));
            }
            if self.pending.len() == READY.len() {
                self.pending.clear();
                ready += 1;
            }
        }
        (output, ready)
    }

    pub fn finish(&mut self) -> Vec<u8> {
        core::mem::take(&mut self.pending)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_split_preserves_terminal_bytes_and_removes_only_marker() {
        let mut input = Vec::from(b"text\x1b[2J\x1b".as_slice());
        input.extend_from_slice(READY);
        input.extend_from_slice(b"\x1b]777;efi-agent;other\x07\xe4\xb8\xad");
        let expected = b"text\x1b[2J\x1b\x1b]777;efi-agent;other\x07\xe4\xb8\xad";
        for split in 0..=input.len() {
            let mut decoder = Decoder::default();
            let (mut first, count1) = decoder.push(&input[..split]);
            let (second, count2) = decoder.push(&input[split..]);
            first.extend_from_slice(&second);
            first.extend_from_slice(&decoder.finish());
            assert_eq!(first, expected);
            assert_eq!(count1 + count2, 1);
        }
    }

    #[test]
    fn one_byte_reads_repeated_marker_and_incomplete_suffix() {
        let mut decoder = Decoder::default();
        let mut input = Vec::from(READY);
        input.extend_from_slice(READY);
        input.extend_from_slice(b"\x1b]777;");
        let mut output = Vec::new();
        let mut ready = 0;
        for byte in input {
            let (bytes, count) = decoder.push(&[byte]);
            output.extend(bytes);
            ready += count;
        }
        assert_eq!(ready, 2);
        assert!(output.is_empty());
        assert_eq!(decoder.finish(), b"\x1b]777;");
    }
}
