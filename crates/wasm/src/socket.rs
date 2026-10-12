//! Shared stream socket state, separate from descriptor allocation.
use crate::network::{Connection, Network, NetworkError, Readiness};
use alloc::{boxed::Box, vec::Vec};

enum State {
    New,
    Connecting(Box<dyn Connection>),
    Connected(Box<dyn Connection>),
    Failed(NetworkError),
    Closed,
}

pub(crate) struct Socket {
    state: State,
    error: Option<NetworkError>,
}

impl Socket {
    pub fn new() -> Self {
        Self {
            state: State::New,
            error: None,
        }
    }

    pub fn connecting(connection: Box<dyn Connection>) -> Self {
        Self {
            state: State::Connecting(connection),
            error: None,
        }
    }

    pub fn begin(&mut self, network: &dyn Network, address: [u8; 4], port: u16) -> Result<(), i32> {
        match self.state {
            State::Connecting(_) => return Err(7), // ALREADY
            State::Connected(_) => return Err(30), // ISCONN
            State::Closed => return Err(53),
            State::New | State::Failed(_) => {}
        }
        self.error = None;
        match network.connect(address, port) {
            Ok(connection) => {
                self.state = State::Connecting(connection);
                Ok(())
            }
            Err(error) => {
                self.state = State::Failed(error);
                self.error = Some(error);
                Err(error.errno())
            }
        }
    }

    pub fn take_error(&mut self) -> i32 {
        if matches!(self.state, State::Connecting(_)) {
            let _ = self.poll_connect();
        }
        self.error.take().map_or(0, NetworkError::errno)
    }

    fn connected(&mut self) -> Result<&mut dyn Connection, NetworkError> {
        match &mut self.state {
            State::Connected(connection) => Ok(connection.as_mut()),
            _ => Err(NetworkError::NotConnected),
        }
    }
}

impl Connection for Socket {
    fn poll_connect(&mut self) -> Result<Option<()>, NetworkError> {
        let result = match &mut self.state {
            State::Connecting(connection) => connection.poll_connect(),
            State::Connected(_) => return Ok(Some(())),
            State::Failed(error) => return Err(*error),
            State::New | State::Closed => return Err(NetworkError::NotConnected),
        };
        match result {
            Ok(Some(())) => {
                let State::Connecting(connection) =
                    core::mem::replace(&mut self.state, State::Closed)
                else {
                    unreachable!("Connecting state polled")
                };
                self.state = State::Connected(connection);
            }
            Err(error) => {
                self.state = State::Failed(error);
                self.error = Some(error);
            }
            Ok(None) => {}
        }
        result
    }

    fn poll_close(&mut self) -> Result<Option<()>, NetworkError> {
        let result = match &mut self.state {
            State::Connected(connection) => connection.poll_close(),
            _ => Ok(Some(())),
        };
        if !matches!(result, Ok(None)) {
            self.state = State::Closed;
        }
        result
    }

    fn read(&mut self, maximum: usize) -> Result<Option<Vec<u8>>, NetworkError> {
        self.connected()?.read(maximum)
    }

    fn write(&mut self, bytes: &[u8]) -> Result<Option<usize>, NetworkError> {
        self.connected()?.write(bytes)
    }

    fn readiness(&mut self, writing: bool) -> Result<Option<Readiness>, NetworkError> {
        if matches!(self.state, State::Connecting(_)) {
            match self.poll_connect()? {
                None => return Ok(None),
                Some(()) if writing => {
                    return Ok(Some(Readiness {
                        bytes: 0,
                        hangup: false,
                    }));
                }
                Some(()) => {}
            }
        }
        if let State::Failed(error) = self.state {
            return Err(error);
        }
        self.connected()?.readiness(writing)
    }
}
