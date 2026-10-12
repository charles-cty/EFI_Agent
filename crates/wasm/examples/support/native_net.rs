use efi_agent_wasm::network::{Connection, Network, NetworkError, Readiness};
use mio::{Events, Interest, Poll, Token, net::TcpStream};
use std::{
    io::{self, Read, Write},
    net::{Ipv4Addr, SocketAddrV4},
    time::Duration,
};

pub struct NativeNetwork;

pub fn error(error: io::Error) -> NetworkError {
    match error.kind() {
        io::ErrorKind::PermissionDenied => NetworkError::Access,
        io::ErrorKind::AddrNotAvailable => NetworkError::AddressUnavailable,
        io::ErrorKind::ConnectionAborted => NetworkError::ConnectionAborted,
        io::ErrorKind::ConnectionRefused => NetworkError::ConnectionRefused,
        io::ErrorKind::ConnectionReset => NetworkError::ConnectionReset,
        io::ErrorKind::HostUnreachable => NetworkError::HostUnreachable,
        io::ErrorKind::InvalidInput => NetworkError::Invalid,
        io::ErrorKind::NetworkUnreachable => NetworkError::NetworkUnreachable,
        io::ErrorKind::NotConnected => NetworkError::NotConnected,
        io::ErrorKind::TimedOut => NetworkError::TimedOut,
        io::ErrorKind::BrokenPipe => NetworkError::Pipe,
        _ => NetworkError::Io,
    }
}

struct NativeConnection {
    stream: TcpStream,
    poll: Poll,
    events: Events,
    writable: bool,
    connected: bool,
    closed: bool,
}

impl NativeConnection {
    fn progress(&mut self) -> Result<(), NetworkError> {
        self.poll
            .poll(&mut self.events, Some(Duration::ZERO))
            .map_err(error)?;
        for event in &self.events {
            self.writable |= event.is_writable() || event.is_error() || event.is_write_closed();
        }
        Ok(())
    }
}

impl Network for NativeNetwork {
    fn connect(&self, address: [u8; 4], port: u16) -> Result<Box<dyn Connection>, NetworkError> {
        let address = SocketAddrV4::new(Ipv4Addr::from(address), port).into();
        let mut stream = TcpStream::connect(address).map_err(error)?;
        let poll = Poll::new().map_err(error)?;
        poll.registry()
            .register(
                &mut stream,
                Token(0),
                Interest::READABLE | Interest::WRITABLE,
            )
            .map_err(error)?;
        Ok(Box::new(NativeConnection {
            stream,
            poll,
            events: Events::with_capacity(8),
            writable: false,
            connected: false,
            closed: false,
        }))
    }
}

impl Connection for NativeConnection {
    fn poll_close(&mut self) -> Result<Option<()>, NetworkError> {
        if !self.closed {
            self.stream
                .shutdown(std::net::Shutdown::Both)
                .map_err(error)?;
            self.closed = true;
        }
        Ok(Some(()))
    }
    fn poll_connect(&mut self) -> Result<Option<()>, NetworkError> {
        if self.connected {
            return Ok(Some(()));
        }
        self.progress()?;
        if !self.writable {
            return Ok(None);
        }
        if let Some(failure) = self.stream.take_error().map_err(error)? {
            return Err(error(failure));
        }
        match self.stream.peer_addr() {
            Ok(_) => {
                self.connected = true;
                Ok(Some(()))
            }
            Err(failure) if failure.kind() == io::ErrorKind::NotConnected => Ok(None),
            Err(failure) => Err(error(failure)),
        }
    }

    fn read(&mut self, maximum: usize) -> Result<Option<Vec<u8>>, NetworkError> {
        if self.closed {
            return Err(NetworkError::NotConnected);
        }
        self.progress()?;
        let mut bytes = vec![0; maximum];
        match self.stream.read(&mut bytes) {
            Ok(count) => {
                bytes.truncate(count);
                Ok(Some(bytes))
            }
            Err(failure) if failure.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(failure) => Err(error(failure)),
        }
    }

    fn write(&mut self, bytes: &[u8]) -> Result<Option<usize>, NetworkError> {
        if self.closed {
            return Err(NetworkError::NotConnected);
        }
        self.progress()?;
        match self.stream.write(bytes) {
            Ok(count) => Ok(Some(count)),
            Err(failure) if failure.kind() == io::ErrorKind::WouldBlock => {
                self.writable = false;
                Ok(None)
            }
            Err(failure) => Err(error(failure)),
        }
    }

    fn readiness(&mut self, writing: bool) -> Result<Option<Readiness>, NetworkError> {
        if self.closed {
            return Err(NetworkError::NotConnected);
        }
        self.progress()?;
        if writing {
            return Ok(self.writable.then_some(Readiness {
                bytes: 1,
                hangup: false,
            }));
        }
        let mut byte = [0];
        match self.stream.peek(&mut byte) {
            Ok(count) => Ok(Some(Readiness {
                bytes: count,
                hangup: count == 0,
            })),
            Err(failure) if failure.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(failure) => Err(error(failure)),
        }
    }
}
