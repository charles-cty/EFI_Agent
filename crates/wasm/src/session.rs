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
    pub closed: bool,
}

impl Channel {
    pub fn read(&mut self, maximum: usize) -> Option<alloc::vec::Vec<u8>> {
        if self.incoming.is_empty() {
            return self.closed.then(alloc::vec::Vec::new);
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
    terminal: Rc<crate::io::Terminal>,
}

impl Session {
    pub fn pid(&self) -> Pid {
        self.pid
    }

    pub fn submit(&mut self, source: &str) -> Result<(), Error> {
        self.submit_request(source, false)
    }

    /// Evaluate in the same interpreter and display its completion value.
    pub fn submit_repl(&mut self, source: &str) -> Result<(), Error> {
        self.submit_request(source, true)
    }

    /// Attach live terminal I/O for one direct evaluation. Tool calls stay finite.
    pub fn submit_terminal(&mut self, source: &str) -> Result<(), Error> {
        self.submit_repl(source)?;
        self.terminal.input.borrow_mut().open = true;
        self.terminal.input.borrow_mut().terminal = true;
        self.terminal.mode.set(7);
        self.terminal.active.set(true);
        Ok(())
    }

    pub fn write_stdin(&self, bytes: &[u8]) -> Result<(), Error> {
        let mut input = self.terminal.input.borrow_mut();
        if !self.terminal.active.get() || !input.open || input.bytes.len() + bytes.len() > 64 * 1024
        {
            return Err(Error::InvalidArguments);
        }
        input.bytes.extend(bytes);
        Ok(())
    }

    pub fn close_stdin(&self) {
        self.terminal.input.borrow_mut().open = false;
    }

    /// Deliver EOF to the source stream of an idle interpreter.
    pub fn close_source(&mut self) -> Result<(), Error> {
        let mut channel = self.channel.borrow_mut();
        if channel.closed || channel.active.is_some() || channel.completed.is_some() {
            return Err(Error::InvalidArguments);
        }
        channel.closed = true;
        self.close_stdin();
        self.terminal.active.set(true);
        Ok(())
    }

    pub fn take_terminal_output(&self) -> alloc::vec::Vec<u8> {
        core::mem::take(&mut *self.terminal.output.borrow_mut())
    }

    /// Bits 0/1 enable line input/echo; bit 2 maps output LF to CRLF.
    pub fn terminal_mode(&self) -> u32 {
        self.terminal.mode.get()
    }

    pub fn set_terminal_size(&self, columns: u16, rows: u16) {
        self.terminal
            .size
            .set((columns.clamp(1, 300), rows.clamp(1, 120)));
    }

    pub fn detach_terminal(&self) {
        self.terminal.active.set(false);
        let mut input = self.terminal.input.borrow_mut();
        input.open = false;
        input.terminal = false;
        input.bytes.clear();
        self.terminal.mode.set(7);
        self.terminal.owner.set(None);
    }

    fn submit_request(&mut self, source: &str, display_result: bool) -> Result<(), Error> {
        if source.contains('\0') || source.len() > MAX_CODE_BYTES {
            return Err(Error::InvalidArguments);
        }
        let mut channel = self.channel.borrow_mut();
        if channel.closed || channel.active.is_some() || channel.completed.is_some() {
            return Err(Error::InvalidArguments);
        }
        let sequence = channel.next.checked_add(1).ok_or(Error::InvalidArguments)?;
        channel.next = sequence;
        channel.active = Some(sequence);
        channel.incoming.extend(sequence.to_le_bytes());
        channel.incoming.extend((source.len() as u32).to_le_bytes());
        channel
            .incoming
            .extend(u32::from(display_result).to_le_bytes());
        channel.incoming.extend(source.bytes());
        Ok(())
    }

    pub fn take_completion(&mut self) -> Option<i32> {
        self.channel.borrow_mut().completed.take()
    }
}

#[derive(Clone, Copy)]
pub enum SessionKind {
    Shell,
    JavaScript,
}

impl SessionKind {
    pub fn slot(self) -> usize {
        match self {
            Self::Shell => 0,
            Self::JavaScript => 1,
        }
    }
}

impl World {
    pub fn start_session(&mut self, kind: SessionKind) -> Result<Session, Error> {
        let channel = Rc::new(RefCell::new(Channel::default()));
        let program = match kind {
            SessionKind::Shell => "ash",
            SessionKind::JavaScript => "quickjs",
        };
        let args = vec![String::from(program), String::from("--efi-session")];
        let pid = self.spawn_child(
            0,
            args,
            Some(self.default_environment.clone()),
            vec![],
            None,
            SpawnSettings {
                session: Some(channel.clone()),
                ..SpawnSettings::default()
            },
        )?;
        let terminal = self.processes[&pid]
            .execution()
            .store
            .data()
            .terminal
            .clone();
        Ok(Session {
            pid,
            channel,
            terminal,
        })
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
            terminal: Rc::new(crate::io::Terminal::default()),
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
        assert_eq!(
            session.channel.borrow_mut().read(99).unwrap(),
            b"\0\0\0\0\0a\nb"
        );
        for malformed in [&response[..7], &[0; 8][..], &[0; 9][..]] {
            assert!(session.channel.borrow_mut().complete(malformed).is_err());
        }
        session.channel.borrow_mut().complete(&response).unwrap();
        assert!(session.channel.borrow_mut().complete(&response).is_err());
        assert_eq!(session.take_completion(), Some(7));
        assert!(session.channel.borrow_mut().read(1).is_none());
        session.submit(&"x".repeat(MAX_CODE_BYTES)).unwrap();
        assert_eq!(session.channel.borrow().readable(), MAX_CODE_BYTES + 12);
    }

    #[test]
    fn repl_and_tool_requests_share_the_channel_with_explicit_display_flags() {
        let mut session = Session {
            pid: 1,
            channel: Rc::new(RefCell::new(Channel::default())),
            terminal: Rc::new(crate::io::Terminal::default()),
        };
        session.submit_repl("1 + 1").unwrap();
        assert_eq!(
            session.channel.borrow_mut().read(12).unwrap(),
            [1, 0, 0, 0, 5, 0, 0, 0, 1, 0, 0, 0]
        );
        assert_eq!(session.channel.borrow_mut().read(5).unwrap(), b"1 + 1");
        session
            .channel
            .borrow_mut()
            .complete(&[1, 0, 0, 0, 0, 0, 0, 0])
            .unwrap();
        assert_eq!(session.take_completion(), Some(0));
        session.submit("value").unwrap();
        assert_eq!(
            session.channel.borrow_mut().read(12).unwrap(),
            [2, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0]
        );
    }
}
