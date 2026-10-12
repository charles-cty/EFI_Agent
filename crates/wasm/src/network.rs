//! Nonblocking IPv4 TCP provider for the cooperative process dispatcher.
use alloc::{boxed::Box, vec::Vec};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkError {
    Access,
    AddressUnavailable,
    ConnectionAborted,
    ConnectionRefused,
    ConnectionReset,
    HostUnreachable,
    Invalid,
    Io,
    NetworkUnreachable,
    NotConnected,
    Unavailable,
    TimedOut,
    Pipe,
}

impl NetworkError {
    pub fn errno(self) -> i32 {
        match self {
            Self::Access => 2,
            Self::AddressUnavailable => 4,
            Self::ConnectionAborted => 13,
            Self::ConnectionRefused => 14,
            Self::ConnectionReset => 15,
            Self::HostUnreachable => 23,
            Self::Invalid => 28,
            Self::Io => 29,
            Self::NetworkUnreachable => 40,
            Self::NotConnected => 53,
            Self::Unavailable => 58,
            Self::Pipe => 64,
            Self::TimedOut => 73,
        }
    }
}

pub struct Readiness {
    pub bytes: usize,
    pub hangup: bool,
}

/// All methods must return without waiting for network or timer progress.
/// Providers own pending tokens/buffers; dropping the last connection must
/// close it and retire those operations before releasing their storage.
pub trait Connection {
    /// None means the connection attempt is still pending.
    fn poll_connect(&mut self) -> Result<Option<()>, NetworkError>;
    /// Drain accepted writes and close normally; None means still pending.
    /// This closes the shared connection, including duplicate descriptors.
    fn poll_close(&mut self) -> Result<Option<()>, NetworkError>;
    /// None means pending, empty bytes mean peer EOF.
    fn read(&mut self, maximum: usize) -> Result<Option<Vec<u8>>, NetworkError>;
    /// None means backpressure; a successful write may accept a partial prefix.
    fn write(&mut self, bytes: &[u8]) -> Result<Option<usize>, NetworkError>;
    /// Inspect readiness without consuming bytes or starting a write.
    fn readiness(&mut self, writing: bool) -> Result<Option<Readiness>, NetworkError>;
}

pub trait Network {
    /// Start one attempt. IPv4 octets are in address order; port is host order.
    fn connect(&self, address: [u8; 4], port: u16) -> Result<Box<dyn Connection>, NetworkError>;
}
