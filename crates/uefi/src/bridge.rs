use crate::tcp::{Deadline, Tcp};
use alloc::{string::String, vec::Vec};
use core::time::Duration;
use efi_agent_core::protocol::ChatMessage;
use efi_agent_core::protocol::{self, Operation, Request, Response};

pub struct Bridge {
    address: [u8; 4],
    port: u16,
    next_id: u64,
    connection: Option<Tcp>,
    incoming: Vec<u8>,
    body_length: Option<usize>,
    heartbeat: Option<Deadline>,
    connected_before: bool,
}

impl Bridge {
    pub fn vm() -> Self {
        Self::new([10, 0, 2, 100], protocol::BRIDGE_PORT)
    }

    pub fn new(address: [u8; 4], port: u16) -> Self {
        Self {
            address,
            port,
            next_id: 1,
            connection: None,
            incoming: Vec::new(),
            body_length: None,
            heartbeat: None,
            connected_before: false,
        }
    }

    /// Only the harmless ping is retried. File and model operations never are.
    pub fn keep_alive(&mut self, poll: &mut dyn FnMut() -> bool) -> Result<bool, String> {
        if self
            .heartbeat
            .as_ref()
            .is_some_and(|timer| !timer.expired())
        {
            return Ok(false);
        }
        self.heartbeat = Some(Deadline::new(Duration::from_secs(30))?);
        if !self.connected_before {
            return Ok(false);
        }
        match self.call(Operation::Ping, poll) {
            Ok(reply) if reply == "pong" => Ok(true),
            Err(error) if error == "Request cancelled" => Err(error),
            _ => {
                self.connection = None;
                self.incoming.clear();
                self.body_length = None;
                self.call(Operation::Ping, poll).and_then(|reply| {
                    if reply == "pong" {
                        Ok(true)
                    } else {
                        Err("HostBridge invalid heartbeat".into())
                    }
                })
            }
        }
    }

    pub fn complete(
        &mut self,
        messages: &[ChatMessage],
        progress: &mut dyn FnMut(&str),
        poll: &mut dyn FnMut() -> bool,
    ) -> Result<ChatMessage, String> {
        let response = self.call_stream(
            Operation::Complete {
                messages: messages.into(),
            },
            poll,
            progress,
        )?;
        serde_json::from_str(&response)
            .map_err(|_| String::from("HostBridge returned an invalid model message"))
    }

    pub fn call(
        &mut self,
        operation: Operation,
        poll: &mut dyn FnMut() -> bool,
    ) -> Result<String, String> {
        self.call_stream(operation, poll, &mut |_| {})
    }

    fn call_stream(
        &mut self,
        operation: Operation,
        poll: &mut dyn FnMut() -> bool,
        progress: &mut dyn FnMut(&str),
    ) -> Result<String, String> {
        if poll() {
            return Err("Request cancelled".into());
        }
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or("RPC request ID exhausted")?;
        let timeout = if matches!(operation, Operation::Ping | Operation::ModelConfig) {
            5
        } else {
            150
        };
        let frame = protocol::encode(&Request { id, operation }).map_err(String::from)?;
        self.heartbeat = Some(Deadline::new(Duration::from_secs(30))?);
        if self.connection.is_none() {
            self.connection = Some(Tcp::connect(self.address, self.port, poll)?);
            self.connected_before = true;
        }
        let result = self.exchange(id, &frame, poll, progress, timeout);
        if result
            .as_ref()
            .is_err_and(|error| error != "Request cancelled")
        {
            self.connection = None;
            self.incoming.clear();
            self.body_length = None;
        }
        // Never retry a write or a model request automatically after a partial
        // exchange: the peer may already have performed the operation.
        result?
    }

    fn exchange(
        &mut self,
        id: u64,
        frame: &[u8],
        poll: &mut dyn FnMut() -> bool,
        progress: &mut dyn FnMut(&str),
        timeout: u64,
    ) -> Result<Result<String, String>, String> {
        let connection = self.connection.as_mut().ok_or("HostBridge disconnected")?;
        // Once transmission starts, finish the frame. The host may execute it
        // even if the user cancels; cancellation cannot roll back file changes.
        connection.send(frame, &mut || {
            poll();
            false
        })?;
        // A single deadline covers fragmented and obsolete response frames.
        let deadline = Deadline::new(Duration::from_secs(timeout))?;
        loop {
            if poll() {
                return Err("Request cancelled".into());
            }
            let target = self.body_length.map_or(4, |length| length + 4);
            if self.incoming.len() < target {
                let mut bytes = [0; 4096];
                let capacity = (target - self.incoming.len()).min(bytes.len());
                let count = connection.read_some(&mut bytes[..capacity], &deadline, poll)?;
                self.incoming.extend_from_slice(&bytes[..count]);
                continue;
            }
            if self.body_length.is_none() {
                self.body_length = Some(
                    protocol::frame_length(self.incoming[..4].try_into().expect("Full header"))
                        .map_err(String::from)?,
                );
                continue;
            }
            let response: Response = serde_json::from_slice(&self.incoming[4..])
                .map_err(|_| String::from("HostBridge invalid response JSON"))?;
            self.incoming.clear();
            self.body_length = None;
            if response.id < id {
                // The cancelled request may still finish on the host. Consume
                // its complete frame, preserving alignment for the new reply.
                continue;
            }
            if response.id != id {
                return Err("HostBridge response ID mismatch".into());
            }
            if let Some(delta) = response.delta {
                progress(&delta);
                continue;
            }
            return Ok(response.result);
        }
    }
}
