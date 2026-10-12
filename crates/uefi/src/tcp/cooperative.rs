//! Persistent TCP4 tokens for the single-thread WASM dispatcher.
use super::*;
use alloc::vec::Vec;
use efi_agent_wasm::network::{Connection, Network, NetworkError, Readiness};

const CAPACITY: usize = 64 * 1024;
const FIN: Status = Status(Status::ERROR_BIT | 104);

fn error(value: Status) -> NetworkError {
    match value {
        Status::ACCESS_DENIED => NetworkError::Access,
        Status::NO_MAPPING => NetworkError::AddressUnavailable,
        Status::UNSUPPORTED | Status::NOT_FOUND => NetworkError::Unavailable,
        Status::INVALID_PARAMETER => NetworkError::Invalid,
        Status::TIMEOUT => NetworkError::TimedOut,
        Status::ABORTED => NetworkError::ConnectionAborted,
        _ if value == Status(Status::ERROR_BIT | 106) => NetworkError::ConnectionRefused,
        _ if value == Status(Status::ERROR_BIT | 105) => NetworkError::ConnectionReset,
        _ if value == Status(Status::ERROR_BIT | 100) => NetworkError::NetworkUnreachable,
        _ if value == Status(Status::ERROR_BIT | 101) => NetworkError::HostUnreachable,
        _ => NetworkError::Io,
    }
}

fn checked(value: Status) -> Result<(), NetworkError> {
    if value == Status::SUCCESS {
        Ok(())
    } else {
        Err(error(value))
    }
}

pub(crate) struct FirmwareNetwork(pub Option<StaticIpv4>);

struct ConnectToken {
    completion: Completion,
    token: Tcp4ConnectionToken,
}

struct ReceiveToken {
    completion: Completion,
    token: Tcp4IoToken,
    packet: RxPacket,
    bytes: Vec<u8>,
}

struct SendToken {
    completion: Completion,
    token: Tcp4IoToken,
    packet: TxPacket,
    bytes: Vec<u8>,
}

struct CloseToken {
    completion: Completion,
    token: Tcp4CloseToken,
}

struct Socket {
    protocol: Option<ScopedProtocol<TcpProtocol>>,
    binding: ScopedProtocol<TcpBinding>,
    child: Handle,
    config: Tcp4ConfigData,
    ipv4: Option<StaticIpv4>,
    configured: bool,
    connected: bool,
    deadline: Deadline,
    connect: Option<Box<ConnectToken>>,
    receive: Option<Box<ReceiveToken>>,
    send: Option<Box<SendToken>>,
    incoming: Vec<u8>,
    cursor: usize,
    eof: bool,
    close: Option<Box<CloseToken>>,
    close_deadline: Option<Deadline>,
    closed: bool,
}

impl Network for FirmwareNetwork {
    fn connect(&self, address: [u8; 4], port: u16) -> Result<Box<dyn Connection>, NetworkError> {
        let handles =
            boot::find_handles::<TcpBinding>().map_err(|failure| error(failure.status()))?;
        let mut last = NetworkError::Unavailable;
        for handle in handles {
            match Socket::new(handle, address, port, self.0.clone()) {
                Ok(socket) => return Ok(Box::new(socket)),
                Err(failure) => last = failure,
            }
        }
        Err(last)
    }
}

impl Socket {
    fn new(
        handle: Handle,
        address: [u8; 4],
        port: u16,
        ipv4: Option<StaticIpv4>,
    ) -> Result<Self, NetworkError> {
        if ipv4.is_none() {
            match Ip4Config2::new(handle) {
                Ok(mut ip) => {
                    let info = ip
                        .get_interface_info()
                        .map_err(|failure| error(failure.status()))?;
                    if info.station_addr == Ipv4Address::from([0; 4]) {
                        ip.set_policy(Ip4Config2Policy::DHCP)
                            .map_err(|failure| error(failure.status()))?;
                    }
                }
                Err(failure)
                    if matches!(failure.status(), Status::NOT_FOUND | Status::UNSUPPORTED) => {}
                Err(failure) => return Err(error(failure.status())),
            }
        }
        // GET_PROTOCOL permits multiple independent TCP children on one NIC.
        // No controller disconnect or driver unload occurs during this runtime.
        let mut binding = unsafe {
            boot::open_protocol::<TcpBinding>(
                boot::OpenProtocolParams {
                    handle,
                    agent: boot::image_handle(),
                    controller: None,
                },
                boot::OpenProtocolAttributes::GetProtocol,
            )
        }
        .map_err(|failure| error(failure.status()))?;
        let mut raw_child = ptr::null_mut();
        // SAFETY: binding is live and raw_child is a writable out parameter.
        checked(unsafe { (binding.0.create_child)(&mut binding.0, &mut raw_child) })?;
        let child = unsafe { Handle::from_ptr(raw_child) }.expect("TCP4 created child handle");
        let protocol = match boot::open_protocol_exclusive::<TcpProtocol>(child) {
            Ok(protocol) => protocol,
            Err(failure) => {
                checked(unsafe { (binding.0.destroy_child)(&mut binding.0, child.as_ptr()) })
                    .expect("Destroy unopened TCP child");
                return Err(error(failure.status()));
            }
        };
        let config = Tcp4ConfigData {
            type_of_service: 0,
            time_to_live: 64,
            access_point: Tcp4AccessPoint {
                use_default_address: ipv4.is_none().into(),
                station_address: Ipv4Address::from(ipv4.as_ref().map_or([0; 4], |ip| ip.address)),
                subnet_mask: Ipv4Address::from(ipv4.as_ref().map_or([0; 4], |ip| ip.subnet_mask)),
                station_port: 0,
                remote_address: Ipv4Address::from(address),
                remote_port: port,
                active_flag: true.into(),
            },
            control_option: ptr::null_mut(),
        };
        // Own the child before allocating further resources: any subsequent
        // failure resets and destroys it through Socket::drop.
        let deadline = match Deadline::new(Duration::from_secs(30)) {
            Ok(deadline) => deadline,
            Err(_) => {
                drop(protocol);
                checked(unsafe { (binding.0.destroy_child)(&mut binding.0, child.as_ptr()) })
                    .expect("Destroy idle TCP child");
                return Err(NetworkError::Io);
            }
        };
        Ok(Self {
            protocol: Some(protocol),
            binding,
            child,
            config,
            ipv4,
            configured: false,
            connected: false,
            deadline,
            connect: None,
            receive: None,
            send: None,
            incoming: Vec::new(),
            cursor: 0,
            eof: false,
            close: None,
            close_deadline: None,
            closed: false,
        })
    }

    fn raw(&mut self) -> *mut Tcp4Protocol {
        &mut self.protocol.as_mut().expect("Open TCP4 socket").0
    }

    fn progress(&mut self) {
        let protocol = self.raw();
        // SAFETY: live child, all outstanding operations own stable storage.
        unsafe {
            let _ = ((*protocol).poll)(protocol);
        }
        event_loop::dispatch();
    }

    fn receive_ready(&mut self) -> Result<bool, NetworkError> {
        self.progress();
        if self.cursor < self.incoming.len() || self.eof {
            return Ok(true);
        }
        if let Some(receive) = &self.receive {
            if !receive.completion.done.ready() {
                return Ok(false);
            }
            let mut receive = self.receive.take().expect("Completed receive");
            let status = unsafe { ptr::read_volatile(&receive.token.completion_token.status) };
            if status == FIN {
                self.eof = true;
                return Ok(true);
            }
            checked(status)?;
            let count = receive.packet.header.data_length as usize;
            if count == 0 || count > receive.bytes.len() {
                return Err(NetworkError::Io);
            }
            self.incoming = core::mem::take(&mut receive.bytes);
            self.incoming.truncate(count);
            self.cursor = 0;
            return Ok(true);
        }
        let completion = Completion::new().map_err(|_| NetworkError::Io)?;
        let mut receive = Box::new(ReceiveToken {
            token: Tcp4IoToken {
                completion_token: completion.token(),
                packet: Tcp4Packet {
                    rx_data: ptr::null_mut(),
                },
            },
            completion,
            packet: RxPacket {
                header: Tcp4ReceiveData {
                    urgent: false.into(),
                    data_length: CAPACITY as u32,
                    fragment_count: 1,
                    fragment_table: [],
                },
                fragment: Tcp4FragmentData {
                    fragment_length: CAPACITY as u32,
                    fragment_buf: ptr::null_mut(),
                },
            },
            bytes: alloc::vec![0; CAPACITY],
        });
        receive.packet.fragment.fragment_buf = receive.bytes.as_mut_ptr();
        receive.token.packet.rx_data = &mut receive.packet.header;
        let protocol = self.raw();
        // SAFETY: boxed token, packet and vector allocation remain owned until
        // completion or synchronous reset; moving Box does not move storage.
        let status = unsafe { ((*protocol).receive)(protocol, &mut receive.token) };
        if status == FIN {
            self.eof = true;
            return Ok(true);
        }
        checked(status)?;
        self.receive = Some(receive);
        Ok(false)
    }
}

impl Connection for Socket {
    fn poll_close(&mut self) -> Result<Option<()>, NetworkError> {
        self.progress();
        if self.closed {
            return Ok(Some(()));
        }
        if self.close_deadline.is_none() {
            self.close_deadline =
                Some(Deadline::new(Duration::from_secs(30)).map_err(|_| NetworkError::Io)?);
        }
        if let Some(close) = &self.close
            && close.completion.done.ready()
        {
            checked(unsafe { ptr::read_volatile(&close.token.completion_token.status) })?;
            self.close = None;
            self.closed = true;
            return Ok(Some(()));
        }
        if self
            .close_deadline
            .as_ref()
            .expect("Close deadline")
            .expired()
        {
            return Err(NetworkError::TimedOut);
        }
        if let Some(send) = &self.send {
            if !send.completion.done.ready() {
                return Ok(None);
            }
            let send = self.send.take().expect("Completed transmit");
            checked(unsafe { ptr::read_volatile(&send.token.completion_token.status) })?;
        }
        if self.close.is_none() {
            let completion = Completion::new().map_err(|_| NetworkError::Io)?;
            let mut close = Box::new(CloseToken {
                token: Tcp4CloseToken {
                    completion_token: completion.token(),
                    abort_on_close: false.into(),
                },
                completion,
            });
            let protocol = self.raw();
            // SAFETY: close token and notification stay boxed until completion
            // or confirmed reset, including cancellation of a suspended guest.
            checked(unsafe { ((*protocol).close)(protocol, &mut close.token) })?;
            self.close = Some(close);
        }
        Ok(None)
    }
    fn poll_connect(&mut self) -> Result<Option<()>, NetworkError> {
        self.progress();
        if self.connected {
            return Ok(Some(()));
        }
        if let Some(connect) = &self.connect
            && connect.completion.done.ready()
        {
            checked(unsafe { ptr::read_volatile(&connect.token.completion_token.status) })?;
            self.connect = None;
            self.connected = true;
            return Ok(Some(()));
        }
        if self.deadline.expired() {
            return Err(NetworkError::TimedOut);
        }
        if !self.configured {
            let protocol = self.raw();
            let configured = unsafe { ((*protocol).configure)(protocol, &self.config) };
            if configured == Status::NO_MAPPING {
                return Ok(None);
            }
            checked(configured)?;
            self.configured = true;
            if let Some(ip) = &self.ipv4 {
                let gateway = Ipv4Address::from(ip.gateway);
                let zero = Ipv4Address::from([0; 4]);
                checked(unsafe {
                    ((*protocol).routes)(protocol, false.into(), &zero, &zero, &gateway)
                })?;
            }
        }
        if self.connect.is_none() {
            let completion = Completion::new().map_err(|_| NetworkError::Io)?;
            let mut connect = Box::new(ConnectToken {
                token: Tcp4ConnectionToken {
                    completion_token: completion.token(),
                },
                completion,
            });
            let protocol = self.raw();
            checked(unsafe { ((*protocol).connect)(protocol, &mut connect.token) })?;
            self.connect = Some(connect);
        }
        Ok(None)
    }

    fn read(&mut self, maximum: usize) -> Result<Option<Vec<u8>>, NetworkError> {
        if self.closed || self.close_deadline.is_some() {
            return Err(NetworkError::NotConnected);
        }
        if !self.receive_ready()? {
            return Ok(None);
        }
        let count = maximum.min(self.incoming.len() - self.cursor);
        let bytes = self.incoming[self.cursor..self.cursor + count].to_vec();
        self.cursor += count;
        if self.cursor == self.incoming.len() {
            self.incoming.clear();
            self.cursor = 0;
        }
        Ok(Some(bytes))
    }

    fn write(&mut self, bytes: &[u8]) -> Result<Option<usize>, NetworkError> {
        if self.closed || self.close_deadline.is_some() {
            return Err(NetworkError::NotConnected);
        }
        self.progress();
        if let Some(send) = &self.send {
            if !send.completion.done.ready() {
                return Ok(None);
            }
            let send = self.send.take().expect("Completed transmit");
            checked(unsafe { ptr::read_volatile(&send.token.completion_token.status) })?;
        }
        let completion = Completion::new().map_err(|_| NetworkError::Io)?;
        let mut send = Box::new(SendToken {
            token: Tcp4IoToken {
                completion_token: completion.token(),
                packet: Tcp4Packet {
                    tx_data: ptr::null_mut(),
                },
            },
            completion,
            packet: TxPacket {
                header: Tcp4TransmitData {
                    push: true.into(),
                    urgent: false.into(),
                    data_length: 0,
                    fragment_count: 1,
                    fragment_table: [],
                },
                fragment: Tcp4FragmentData {
                    fragment_length: 0,
                    fragment_buf: ptr::null_mut(),
                },
            },
            bytes: bytes[..bytes.len().min(CAPACITY)].to_vec(),
        });
        send.packet.header.data_length = send.bytes.len() as u32;
        send.packet.fragment.fragment_length = send.bytes.len() as u32;
        send.packet.fragment.fragment_buf = send.bytes.as_mut_ptr();
        send.token.packet.tx_data = &mut send.packet.header;
        let protocol = self.raw();
        checked(unsafe { ((*protocol).transmit)(protocol, &mut send.token) })?;
        let accepted = send.bytes.len();
        self.send = Some(send);
        // Bytes are now owned by the backend. Report acceptance once, even
        // when another descriptor submits the next write on this connection.
        Ok(Some(accepted))
    }

    fn readiness(&mut self, writing: bool) -> Result<Option<Readiness>, NetworkError> {
        if self.closed || self.close_deadline.is_some() {
            return Err(NetworkError::NotConnected);
        }
        if writing {
            self.progress();
            if self
                .send
                .as_ref()
                .is_some_and(|send| send.completion.done.ready())
            {
                let send = self.send.take().expect("Completed transmit");
                checked(unsafe { ptr::read_volatile(&send.token.completion_token.status) })?;
            }
            return Ok(if self.send.is_none() {
                Some(Readiness {
                    bytes: CAPACITY,
                    hangup: false,
                })
            } else {
                None
            });
        }
        Ok(self.receive_ready()?.then_some(Readiness {
            bytes: self.incoming.len() - self.cursor,
            hangup: self.eof,
        }))
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        let protocol = self.raw();
        // Request an abortive close so the remote peer observes cancellation.
        // Reset below also retires this boxed close token before storage dies.
        let mut close = if self.closed || self.close.is_some() {
            None
        } else {
            Completion::new().ok().map(|completion| {
                let token = Box::new(Tcp4CloseToken {
                    completion_token: completion.token(),
                    abort_on_close: true.into(),
                });
                (completion, token)
            })
        };
        let close_queued = close.as_mut().is_some_and(|(_, token)| {
            (unsafe { ((*protocol).close)(protocol, token.as_mut()) }) == Status::SUCCESS
        });
        self.progress();
        // TCP4 Configure(NULL) synchronously flushes queued operations. Keep all
        // token/packet/buffer allocations live until reset and dispatch finish.
        checked(unsafe { ((*protocol).configure)(protocol, ptr::null()) })
            .expect("Reset TCP4 socket before freeing tokens");
        event_loop::dispatch();
        if close_queued {
            assert!(
                close.as_ref().expect("Queued close").0.done.ready(),
                "Reset did not retire TCP4 close token"
            );
        }
        for ready in [
            self.connect
                .as_ref()
                .map(|token| token.completion.done.ready()),
            self.receive
                .as_ref()
                .map(|token| token.completion.done.ready()),
            self.send
                .as_ref()
                .map(|token| token.completion.done.ready()),
            self.close
                .as_ref()
                .map(|token| token.completion.done.ready()),
        ]
        .into_iter()
        .flatten()
        {
            assert!(ready, "Reset did not retire TCP4 token");
        }
        self.connect = None;
        self.receive = None;
        self.send = None;
        self.close = None;
        drop(self.protocol.take());
        checked(unsafe {
            (self.binding.0.destroy_child)(&mut self.binding.0, self.child.as_ptr())
        })
        .expect("Destroy TCP4 socket child");
    }
}
