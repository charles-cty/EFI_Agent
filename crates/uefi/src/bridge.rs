use crate::tcp::Tcp;
use alloc::{string::String, vec};
use efi_agent_core::protocol::{self, Operation, Request, Response};

pub struct Bridge {
    address: [u8; 4],
    port: u16,
    next_id: u64,
    connection: Option<Tcp>,
}

impl Bridge {
    pub fn vm() -> Self {
        Self {
            address: [10, 0, 2, 100],
            port: protocol::BRIDGE_PORT,
            next_id: 1,
            connection: None,
        }
    }

    pub fn call(&mut self, operation: Operation) -> Result<String, String> {
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or("RPC request ID exhausted")?;
        let frame = protocol::encode(&Request { id, operation }).map_err(String::from)?;
        if self.connection.is_none() {
            self.connection = Some(Tcp::connect(self.address, self.port)?);
        }
        let result = self.exchange(id, &frame);
        if result.is_err() {
            self.connection = None;
        }
        // Never retry a write or a model request automatically after a partial
        // exchange: the peer may already have performed the operation.
        result
    }

    fn exchange(&mut self, id: u64, frame: &[u8]) -> Result<String, String> {
        let connection = self.connection.as_mut().ok_or("HostBridge disconnected")?;
        connection.send(frame)?;
        let mut header = [0; 4];
        connection.read_exact(&mut header)?;
        let mut body = vec![0; protocol::frame_length(header).map_err(String::from)?];
        connection.read_exact(&mut body)?;
        let response: Response = serde_json::from_slice(&body)
            .map_err(|_| String::from("HostBridge invalid response JSON"))?;
        if response.id != id {
            return Err(String::from("HostBridge response ID mismatch"));
        }
        response.result
    }
}
