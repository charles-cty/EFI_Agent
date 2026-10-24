//! Host-owned code sessions over a bounded, non-inheritable control stream.
use crate::{Error, Pid, SpawnSettings, World};
use alloc::{collections::VecDeque, rc::Rc, string::String, vec};
use core::cell::RefCell;

pub const MAX_CODE_BYTES: usize = 60 * 1024;

#[derive(Default)]
pub(crate) struct Channel {
    incoming: VecDeque<u8>,
    active: Option<u32>,
    completed: Option<i32>,
    next: u32,
    pub opened: bool,
}

impl Channel {
    pub fn read(&mut self, maximum: usize) -> Option<alloc::vec::Vec<u8>> {
        if self.incoming.is_empty() {
            return None;
        }
        let count = maximum.min(self.incoming.len());
        Some(self.incoming.drain(..count).collect())
    }

    pub fn readable(&self) -> usize {
        self.incoming.len()
    }

    pub fn complete(&mut self, bytes: &[u8]) -> Result<(), i32> {
        if bytes.len() != 8 || !self.incoming.is_empty() || self.completed.is_some() {
            return Err(28);
        }
        let sequence = u32::from_le_bytes(bytes[..4].try_into().unwrap());
        if self.active != Some(sequence) {
            return Err(28);
        }
        let status = i32::from_le_bytes(bytes[4..].try_into().unwrap());
        self.active = None;
        self.completed = Some(status);
        Ok(())
    }
}

/// A handle belongs to one host conversation, rather than a guest child.
pub struct Session {
    pid: Pid,
    channel: Rc<RefCell<Channel>>,
}

impl Session {
    pub fn pid(&self) -> Pid {
        self.pid
    }

    pub fn submit(&mut self, source: &str) -> Result<(), Error> {
        if source.contains('\0') || source.len() > MAX_CODE_BYTES {
            return Err(Error::InvalidArguments);
        }
        let mut channel = self.channel.borrow_mut();
        if channel.active.is_some() || channel.completed.is_some() {
            return Err(Error::InvalidArguments);
        }
        let sequence = channel.next.checked_add(1).ok_or(Error::InvalidArguments)?;
        channel.next = sequence;
        channel.active = Some(sequence);
        channel.incoming.extend(sequence.to_le_bytes());
        channel.incoming.extend((source.len() as u32).to_le_bytes());
        channel.incoming.extend(source.bytes());
        Ok(())
    }

    pub fn take_completion(&mut self) -> Option<i32> {
        self.channel.borrow_mut().completed.take()
    }
}

impl World {
    pub fn start_session(&mut self, python: bool) -> Result<Session, Error> {
        let channel = Rc::new(RefCell::new(Channel::default()));
        let args = if python {
            vec![
                String::from("python"),
                String::from("-c"),
                String::from("import _efi_session; _efi_session.run()"),
            ]
        } else {
            vec![String::from("ash"), String::from("--efi-session")]
        };
        let pid = self.spawn_child(
            0,
            args,
            Some(vec![]),
            vec![],
            None,
            SpawnSettings {
                session: Some(channel.clone()),
                ..SpawnSettings::default()
            },
        )?;
        Ok(Session { pid, channel })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn framing_rejects_stale_partial_and_early_replies_before_effects() {
        let mut session = Session {
            pid: 1,
            channel: Rc::new(RefCell::new(Channel::default())),
        };
        for invalid in [String::from("a\0b"), "é".repeat(MAX_CODE_BYTES / 2 + 1)] {
            assert!(session.submit(&invalid).is_err());
            assert_eq!(session.channel.borrow().readable(), 0);
        }
        session.submit("a\nb").unwrap();
        assert!(session.submit("overlap").is_err());
        let response = [1u32.to_le_bytes(), 7i32.to_le_bytes()].concat();
        assert!(session.channel.borrow_mut().complete(&response).is_err());
        let first = session.channel.borrow_mut().read(7).unwrap();
        assert_eq!(first, [1, 0, 0, 0, 3, 0, 0]);
        assert!(session.channel.borrow_mut().complete(&response).is_err());
        assert_eq!(session.channel.borrow_mut().read(99).unwrap(), b"\0a\nb");
        for malformed in [&response[..7], &[0; 8][..], &[0; 9][..]] {
            assert!(session.channel.borrow_mut().complete(malformed).is_err());
        }
        session.channel.borrow_mut().complete(&response).unwrap();
        assert!(session.channel.borrow_mut().complete(&response).is_err());
        assert_eq!(session.take_completion(), Some(7));
        assert!(session.channel.borrow_mut().read(1).is_none());
        session.submit(&"x".repeat(MAX_CODE_BYTES)).unwrap();
        assert_eq!(session.channel.borrow().readable(), MAX_CODE_BYTES + 8);
    }
}
