//! Cooperative TCP4 operations over firmware asynchronous tokens.
//!
//! No queued token, event context, or packet buffer can outlive its Rust storage.
use crate::event_loop::{self, Notification};
use alloc::{boxed::Box, format, string::String};
use core::{
    ffi::c_void,
    ptr::{self, NonNull},
    time::Duration,
};
use uefi::{
    Event, Handle, Status,
    boot::{self, EventType, ScopedProtocol, TimerTrigger, Tpl},
    proto::{network::ip4config2::Ip4Config2, unsafe_protocol},
};
use uefi_raw::{
    Ipv4Address,
    protocol::{
        driver::ServiceBindingProtocol,
        network::{ip4_config2::Ip4Config2Policy, tcp4::*},
    },
};

#[derive(Debug)]
#[repr(transparent)]
#[unsafe_protocol(Tcp4Protocol::GUID)]
struct TcpProtocol(Tcp4Protocol);

#[derive(Debug)]
#[repr(transparent)]
#[unsafe_protocol(Tcp4Protocol::SERVICE_BINDING_GUID)]
struct TcpBinding(ServiceBindingProtocol);

fn status(result: Status, action: &str) -> Result<(), String> {
    if result == Status::SUCCESS {
        Ok(())
    } else {
        Err(format!("TCP4 {action}: {result}"))
    }
}

/// The notification context stays heap allocated until its event is closed.
struct Completion {
    event: Option<Event>,
    done: Box<Notification>,
}

unsafe extern "efiapi" fn completed(_: Event, context: Option<NonNull<c_void>>) {
    if let Some(context) = context {
        event_loop::enqueue(context.cast());
    }
}

impl Completion {
    fn new() -> Result<Self, String> {
        let mut done = Notification::new();
        // SAFETY: context is stable and valid for the event lifetime. The app
        // never exits boot services; the callback only marks and queues completion.
        let event = unsafe {
            boot::create_event(
                EventType::NOTIFY_SIGNAL,
                Tpl::CALLBACK,
                Some(completed),
                Some(NonNull::from(done.as_mut()).cast()),
            )
        }
        .map_err(|e| format!("TCP4 event: {e}"))?;
        Ok(Self {
            event: Some(event),
            done,
        })
    }

    fn token(&self) -> Tcp4CompletionToken {
        Tcp4CompletionToken {
            event: self.event.as_ref().expect("Live event").as_ptr(),
            status: Status::NOT_READY,
        }
    }
}

impl Drop for Completion {
    fn drop(&mut self) {
        if let Some(event) = self.event.take() {
            // Failure would leave firmware with a pointer to freed context.
            boot::close_event(event).expect("Cannot close TCP4 completion event");
            event_loop::dispatch();
        }
    }
}

pub(crate) struct Deadline(Option<Event>);
impl Deadline {
    pub(crate) fn new(duration: Duration) -> Result<Self, String> {
        // SAFETY: timer has no callback or context and is closed by Drop.
        let event = unsafe { boot::create_event(EventType::TIMER, Tpl::APPLICATION, None, None) }
            .map_err(|e| format!("TCP4 timer: {e}"))?;
        let timer = Self(Some(event));
        boot::set_timer(
            timer.0.as_ref().expect("Live timer"),
            TimerTrigger::Relative(duration),
        )
        .map_err(|e| format!("TCP4 timer: {e}"))?;
        Ok(timer)
    }
    pub(crate) fn expired(&self) -> bool {
        // Timer errors terminate the wait, which will cancel the queued token.
        boot::check_event(self.0.as_ref().expect("Live timer")).unwrap_or(true)
    }
}
impl Drop for Deadline {
    fn drop(&mut self) {
        if let Some(event) = self.0.take() {
            let _ = boot::close_event(event);
        }
    }
}

pub struct Tcp {
    protocol: Option<ScopedProtocol<TcpProtocol>>,
    binding: ScopedProtocol<TcpBinding>,
    child: Handle,
}

pub(crate) fn interfaces() -> uefi::Result<usize> {
    match boot::find_handles::<TcpBinding>() {
        Ok(handles) => Ok(handles.len()),
        Err(error) if error.status() == Status::NOT_FOUND => Ok(0),
        Err(error) => Err(error),
    }
}

impl Tcp {
    pub fn connect(
        address: [u8; 4],
        port: u16,
        poll: &mut dyn FnMut() -> bool,
    ) -> Result<Self, String> {
        let handles =
            boot::find_handles::<TcpBinding>().map_err(|e| format!("TCP4 service binding: {e}"))?;
        let mut last = String::from("No TCP4-capable network interface");
        for handle in handles {
            match Self::on_interface(handle, address, port, poll) {
                Ok(connection) => return Ok(connection),
                Err(error) if error == "Request cancelled" => return Err(error),
                Err(error) => last = error,
            }
        }
        Err(last)
    }

    fn on_interface(
        handle: Handle,
        address: [u8; 4],
        port: u16,
        poll: &mut dyn FnMut() -> bool,
    ) -> Result<Self, String> {
        if poll() {
            return Err("Request cancelled".into());
        }
        // Configure an available IPv4 policy before creating a TCP child. Some
        // firmware retains an unresolved default mapping once a child exists.
        // Absence of Config2 is allowed when TCP4 supplies a configured address.
        let address_configurable = Self::configure_address(handle, poll)?;
        let mut binding = boot::open_protocol_exclusive::<TcpBinding>(handle)
            .map_err(|e| format!("TCP4 binding: {e}"))?;
        let mut raw_child = ptr::null_mut();
        // SAFETY: binding is open and raw_child is a writable out parameter.
        status(
            unsafe { (binding.0.create_child)(&mut binding.0, &mut raw_child) },
            "create child",
        )?;
        // SAFETY: successful CreateChild returns a valid firmware handle.
        let child = unsafe { Handle::from_ptr(raw_child) }.ok_or("TCP4 returned a null child")?;
        let protocol = match boot::open_protocol_exclusive::<TcpProtocol>(child) {
            Ok(protocol) => protocol,
            Err(e) => {
                // SAFETY: no protocol references or pending operations exist.
                let _ = unsafe { (binding.0.destroy_child)(&mut binding.0, child.as_ptr()) };
                return Err(format!("TCP4 open child: {e}"));
            }
        };
        let mut connection = Self {
            protocol: Some(protocol),
            binding,
            child,
        };
        let config = Tcp4ConfigData {
            type_of_service: 0,
            time_to_live: 64,
            access_point: Tcp4AccessPoint {
                use_default_address: true.into(),
                station_address: Ipv4Address::from([0; 4]),
                subnet_mask: Ipv4Address::from([0; 4]),
                station_port: 0,
                remote_address: Ipv4Address::from(address),
                remote_port: port,
                active_flag: true.into(),
            },
            control_option: ptr::null_mut(),
        };
        let protocol = connection.raw();
        // SAFETY: Configure reads config only for this call.
        let configured = unsafe { ((*protocol).configure)(protocol, &config) };
        if configured == Status::NO_MAPPING {
            if !address_configurable {
                return Err("TCP4 has no address and IPv4 Config2 is unavailable; configure the interface in firmware".into());
            }
            let deadline = Deadline::new(Duration::from_secs(30))?;
            loop {
                if poll() {
                    return Err("Request cancelled".into());
                }
                let configured = unsafe { ((*protocol).configure)(protocol, &config) };
                if configured != Status::NO_MAPPING {
                    status(configured, "configure after address setup")?;
                    break;
                }
                if deadline.expired() {
                    return Err("TCP4 address mapping timed out".into());
                }
                // Advance the firmware's child IP state while mapping settles.
                unsafe {
                    let _ = ((*protocol).poll)(protocol);
                }
                event_loop::idle(Duration::from_millis(10))?;
            }
        } else {
            status(configured, "configure")?;
        }

        let completion = Completion::new()?;
        // Allocate the deadline before queuing the token so an allocation
        // failure cannot return with firmware still retaining stack pointers.
        let deadline = Deadline::new(Duration::from_secs(15))?;
        let mut token = Tcp4ConnectionToken {
            completion_token: completion.token(),
        };
        // SAFETY: token remains in this stack frame until completion or cancel.
        status(
            unsafe { ((*protocol).connect)(protocol, &mut token) },
            "connect queue",
        )?;
        connection.wait(&mut token.completion_token, &completion, &deadline, poll)?;
        Ok(connection)
    }

    fn configure_address(handle: Handle, poll: &mut dyn FnMut() -> bool) -> Result<bool, String> {
        let mut ip = match Ip4Config2::new(handle) {
            Ok(ip) => ip,
            Err(error) if matches!(error.status(), Status::UNSUPPORTED | Status::NOT_FOUND) => {
                return Ok(false);
            }
            Err(error) => return Err(format!("IPv4 configuration: {error}")),
        };
        if ip
            .get_interface_info()
            .map_err(|e| format!("IPv4 configuration: {e}"))?
            .station_addr
            == Ipv4Address::from([0; 4])
        {
            ip.set_policy(Ip4Config2Policy::DHCP)
                .map_err(|e| format!("IPv4 DHCP: {e}"))?;
            let deadline = Deadline::new(Duration::from_secs(30))?;
            loop {
                if poll() {
                    return Err("Request cancelled".into());
                }
                if ip
                    .get_interface_info()
                    .map_err(|e| format!("IPv4 configuration: {e}"))?
                    .station_addr
                    != Ipv4Address::from([0; 4])
                {
                    break;
                }
                if deadline.expired() {
                    return Err("IPv4 DHCP timed out".into());
                }
                event_loop::idle(Duration::from_millis(10))?;
            }
        }
        drop(ip);
        if poll() {
            return Err("Request cancelled".into());
        }
        Ok(true)
    }

    fn raw(&mut self) -> *mut Tcp4Protocol {
        &mut self.protocol.as_mut().expect("Open TCP4 protocol").0
    }

    fn wait(
        &mut self,
        token: &mut Tcp4CompletionToken,
        completion: &Completion,
        deadline: &Deadline,
        poll: &mut dyn FnMut() -> bool,
    ) -> Result<(), String> {
        let protocol = self.raw();
        loop {
            event_loop::dispatch();
            if completion.done.ready() {
                // SAFETY: event signals after firmware writes the token status.
                return status(unsafe { ptr::read_volatile(&token.status) }, "completion");
            }
            let interrupted = poll();
            if interrupted || deadline.expired() {
                // SAFETY: token is still queued or already complete. Cancel
                // synchronously signals queued tokens as specified by TCP4.
                let cancelled = unsafe { ((*protocol).cancel)(protocol, token) };
                if cancelled != Status::SUCCESS && cancelled != Status::NOT_FOUND {
                    // A reset flushes all queued operations before it returns.
                    status(
                        unsafe { ((*protocol).configure)(protocol, ptr::null()) },
                        "reset after cancel failure",
                    )
                    .expect("Firmware failed to release pending TCP4 token");
                }
                // Pump completion notifications at APPLICATION before retiring storage.
                for _ in 0..1000 {
                    event_loop::dispatch();
                    if completion.done.ready() {
                        // Cancellation can race a successful receive. Preserve
                        // completed bytes so the framed stream stays aligned.
                        if interrupted && token.status == Status::SUCCESS {
                            return Ok(());
                        }
                        return Err(String::from(if interrupted {
                            "Request cancelled"
                        } else {
                            "TCP4 operation timed out"
                        }));
                    }
                    unsafe {
                        let _ = ((*protocol).poll)(protocol);
                    }
                    // Keep token storage live even if the event scheduler fails.
                    event_loop::idle(Duration::from_millis(1))
                        .expect("Cannot drain cancelled TCP4 token");
                }
                // Do not unwind and free storage still owned by broken firmware.
                panic!("Firmware did not signal cancelled TCP4 token");
            }
            // SAFETY: live configured protocol; buffers remain valid while polling.
            unsafe {
                let _ = ((*protocol).poll)(protocol);
            }
            // A scheduler failure must cancel the live token before unwinding.
            if event_loop::idle(Duration::from_millis(1)).is_err() {
                status(
                    unsafe { ((*protocol).configure)(protocol, ptr::null()) },
                    "reset after event loop failure",
                )
                .expect("Firmware failed to release TCP4 token");
                event_loop::dispatch();
                assert!(
                    completion.done.ready(),
                    "Firmware did not retire TCP4 token after reset"
                );
                return Err("UEFI event loop failed".into());
            }
        }
    }

    pub fn send(&mut self, bytes: &[u8], poll: &mut dyn FnMut() -> bool) -> Result<(), String> {
        if bytes.is_empty() {
            return Ok(());
        }
        let completion = Completion::new()?;
        let deadline = Deadline::new(Duration::from_secs(30))?;
        // repr(C) places the fragment directly after the flexible-array header.
        let mut packet = TxPacket {
            header: Tcp4TransmitData {
                push: true.into(),
                urgent: false.into(),
                data_length: bytes.len() as u32,
                fragment_count: 1,
                fragment_table: [],
            },
            fragment: Tcp4FragmentData {
                fragment_length: bytes.len() as u32,
                fragment_buf: bytes.as_ptr().cast_mut(),
            },
        };
        let mut token = Tcp4IoToken {
            completion_token: completion.token(),
            packet: Tcp4Packet {
                tx_data: &mut packet.header,
            },
        };
        let protocol = self.raw();
        // SAFETY: firmware reads (does not modify) the transmit buffer. Packet,
        // token, and bytes stay valid through wait including timeout cancellation.
        status(
            unsafe { ((*protocol).transmit)(protocol, &mut token) },
            "transmit queue",
        )?;
        self.wait(&mut token.completion_token, &completion, &deadline, poll)
    }

    pub fn read_some(
        &mut self,
        bytes: &mut [u8],
        deadline: &Deadline,
        poll: &mut dyn FnMut() -> bool,
    ) -> Result<usize, String> {
        // The bridge retains successfully received fragments across cancellation.
        if bytes.is_empty() {
            return Ok(0);
        }
        {
            let completion = Completion::new()?;
            let capacity = bytes.len().min(64 * 1024);
            let mut packet = RxPacket {
                header: Tcp4ReceiveData {
                    urgent: false.into(),
                    data_length: capacity as u32,
                    fragment_count: 1,
                    fragment_table: [],
                },
                fragment: Tcp4FragmentData {
                    fragment_length: capacity as u32,
                    fragment_buf: bytes.as_mut_ptr(),
                },
            };
            let mut token = Tcp4IoToken {
                completion_token: completion.token(),
                packet: Tcp4Packet {
                    rx_data: &mut packet.header,
                },
            };
            let protocol = self.raw();
            // SAFETY: output buffer, descriptor, and token are live until wait
            // confirms completion, including cancellation on timeout.
            status(
                unsafe { ((*protocol).receive)(protocol, &mut token) },
                "receive queue",
            )?;
            self.wait(&mut token.completion_token, &completion, deadline, poll)?;
            let count = packet.header.data_length as usize;
            if count == 0 || count > capacity {
                return Err(String::from("TCP4 invalid receive length"));
            }
            Ok(count)
        }
    }
}

#[repr(C)]
struct TxPacket {
    header: Tcp4TransmitData,
    fragment: Tcp4FragmentData,
}
#[repr(C)]
struct RxPacket {
    header: Tcp4ReceiveData,
    fragment: Tcp4FragmentData,
}

// Check offsets against the ABI's flexible-array positions at compile time.
const _: () = assert!(
    core::mem::offset_of!(TxPacket, fragment)
        == core::mem::offset_of!(Tcp4TransmitData, fragment_table)
);
const _: () = assert!(
    core::mem::offset_of!(RxPacket, fragment)
        == core::mem::offset_of!(Tcp4ReceiveData, fragment_table)
);

impl Drop for Tcp {
    fn drop(&mut self) {
        if self.protocol.is_some() {
            let protocol = self.raw();
            // Configure(NULL) only resets the local state machine. Send an
            // abortive close first so peers and QEMU retire the old connection.
            if let (Ok(completion), Ok(deadline)) =
                (Completion::new(), Deadline::new(Duration::from_secs(2)))
            {
                let mut token = Tcp4CloseToken {
                    completion_token: completion.token(),
                    abort_on_close: true.into(),
                };
                // SAFETY: Drop runs at APPLICATION with no other queued tokens;
                // close token stays live through completion or confirmed reset.
                if unsafe { ((*protocol).close)(protocol, &mut token) } == Status::SUCCESS {
                    let _ = self.wait(
                        &mut token.completion_token,
                        &completion,
                        &deadline,
                        &mut || false,
                    );
                }
            }
            // SAFETY: all methods have retired their tokens before returning.
            let _ = unsafe { ((*protocol).configure)(protocol, ptr::null()) };
            // CloseProtocol must precede DestroyChild.
            drop(self.protocol.take());
        }
        // SAFETY: no open child protocol or pending token references remain.
        let _ = unsafe { (self.binding.0.destroy_child)(&mut self.binding.0, self.child.as_ptr()) };
    }
}
