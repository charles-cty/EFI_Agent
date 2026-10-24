use crate::installation::HostLinker;
use crate::network::Connection;
use crate::{
    Context, MAX_OUTPUT_BYTES, Pid,
    clock::{ClockError, ClockId},
    io,
};
use alloc::{rc::Rc, string::String, vec, vec::Vec};
use core::{
    cell::{Cell, RefCell},
    fmt,
    ops::Range,
};
use wasmi::{AsContextMut, Caller, Engine};

const BADF: i32 = io::BADF;
const FAULT: i32 = 21;
const INVAL: i32 = 28;

#[derive(Clone, Debug)]
pub(crate) enum Action {
    Control {
        fd: u32,
        bytes: Vec<u8>,
        written: u32,
    },
    Yield,
    ProgramExists(String),
    Spawn {
        args: Vec<String>,
        environment: Option<Vec<String>>,
        descriptors: Option<Vec<(u32, u32)>>,
    },
    Wait {
        child: Pid,
        status_ptr: u32,
        nohang: bool,
    },
    Io(IoRequest),
}

#[derive(Clone, Debug)]
pub(crate) enum IoRequest {
    Resolve {
        result_ptr: u32,
    },
    CloseSocket {
        fd: u32,
    },
    Connect {
        fd: u32,
        result_ptr: u32,
    },
    ConnectSocket {
        fd: u32,
    },
    Read {
        fd: u32,
        vectors: Vec<Range<usize>>,
        result_ptr: u32,
    },
    Write {
        fd: u32,
        bytes: Vec<u8>,
        result_ptr: u32,
    },
    Poll {
        subscriptions: Vec<Subscription>,
        events_ptr: u32,
        result_ptr: u32,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct Subscription {
    userdata: u64,
    kind: u8,
    fd: u32,
    deadline: Option<(ClockId, u64)>,
}

/// Complete a suspended syscall without calling its Wasm import a second time.
/// Its memory ranges were validated while the caller was running. Guest memory
/// cannot change while its continuation is blocked.
pub(crate) fn perform_io(
    mut context: impl AsContextMut<Data = Context>,
    memory: wasmi::Memory,
    request: &IoRequest,
) -> Result<Option<()>, i32> {
    let (result_ptr, count) = match request {
        IoRequest::Resolve { result_ptr } => {
            let result = (|| {
                let clock = context.as_context().data().clock.clone().ok_or(58)?;
                let now = clock.now(ClockId::Realtime).map_err(ClockError::errno)?;
                let mut context = context.as_context_mut();
                let query = context
                    .data_mut()
                    .dns_query
                    .as_mut()
                    .expect("Pending DNS query");
                if now >= query.deadline {
                    return Err(73);
                }
                query.poll()
            })();
            if matches!(result, Ok(None)) {
                return Ok(None);
            }
            context.as_context_mut().data_mut().dns_query = None;
            let address = result?.expect("Completed DNS query");
            memory
                .write(&mut context, *result_ptr as usize, &address)
                .map_err(|_| FAULT)?;
            return Ok(Some(()));
        }
        IoRequest::CloseSocket { fd } => {
            let connection = match context.as_context().data().descriptors.get(fd) {
                Some(io::Descriptor::Socket { connection, .. }) => connection.clone(),
                _ => return Err(BADF),
            };
            let result = connection.borrow_mut().poll_close();
            if matches!(result, Ok(None)) {
                return Ok(None);
            }
            context.as_context_mut().data_mut().descriptors.remove(fd);
            context.as_context_mut().data_mut().close_on_exec.remove(fd);
            return result
                .map(|_| Some(()))
                .map_err(crate::network::NetworkError::errno);
        }
        IoRequest::Connect { fd, result_ptr } => {
            let connection = match context.as_context().data().descriptors.get(fd) {
                Some(io::Descriptor::Socket { connection, .. }) => connection.clone(),
                _ => return Err(BADF),
            };
            let result = connection.borrow_mut().poll_connect();
            match result {
                Ok(None) => return Ok(None),
                Ok(Some(())) => (*result_ptr, *fd as usize),
                Err(error) => {
                    context.as_context_mut().data_mut().descriptors.remove(fd);
                    context.as_context_mut().data_mut().close_on_exec.remove(fd);
                    return Err(error.errno());
                }
            }
        }
        IoRequest::ConnectSocket { fd } => {
            let socket = match context.as_context().data().descriptors.get(fd) {
                Some(io::Descriptor::Socket { connection, .. }) => connection.clone(),
                _ => return Err(BADF),
            };
            return socket
                .borrow_mut()
                .poll_connect()
                .map(|result| result.map(|_| ()))
                .map_err(crate::network::NetworkError::errno);
        }
        IoRequest::Poll {
            subscriptions,
            events_ptr,
            result_ptr,
        } => {
            let mut events = Vec::new();
            for subscription in subscriptions {
                let readiness = if subscription.kind == 0 {
                    if let Some((id, deadline)) = subscription.deadline {
                        let clock = context.as_context().data().clock.clone().ok_or(58)?;
                        let now = clock.now(id).map_err(ClockError::errno)?;
                        if now < deadline {
                            continue;
                        }
                    }
                    Some(io::Readiness {
                        bytes: 0,
                        hangup: false,
                        error: 0,
                    })
                } else {
                    match context
                        .as_context()
                        .data()
                        .descriptors
                        .get(&subscription.fd)
                    {
                        Some(descriptor) => descriptor.readiness(subscription.kind == 2),
                        None => Some(io::Readiness {
                            bytes: 0,
                            hangup: false,
                            error: BADF,
                        }),
                    }
                };
                let Some(readiness) = readiness else {
                    continue;
                };
                let mut event = [0u8; 32];
                event[..8].copy_from_slice(&subscription.userdata.to_le_bytes());
                event[8..10].copy_from_slice(&(readiness.error as u16).to_le_bytes());
                event[10] = subscription.kind;
                event[16..24].copy_from_slice(&(readiness.bytes as u64).to_le_bytes());
                event[24..26].copy_from_slice(&u16::from(readiness.hangup).to_le_bytes());
                events.extend_from_slice(&event);
            }
            if events.is_empty() {
                return Ok(None);
            }
            memory
                .write(&mut context, *events_ptr as usize, &events)
                .map_err(|_| FAULT)?;
            (*result_ptr, events.len() / 32)
        }
        IoRequest::Read {
            fd,
            vectors,
            result_ptr,
        } => {
            let maximum = vectors
                .iter()
                .map(|range| range.len())
                .sum::<usize>()
                .min(MAX_OUTPUT_BYTES);
            let descriptor = context
                .as_context()
                .data()
                .descriptors
                .get(fd)
                .ok_or(BADF)?
                .clone();
            let Some(bytes) = descriptor.read(maximum)? else {
                return Ok(None);
            };
            let mut cursor = 0;
            for vector in vectors {
                let count = vector.len().min(bytes.len() - cursor);
                memory
                    .write(&mut context, vector.start, &bytes[cursor..cursor + count])
                    .map_err(|_| FAULT)?;
                cursor += count;
            }
            (*result_ptr, bytes.len())
        }
        IoRequest::Write {
            fd,
            bytes,
            result_ptr,
        } => {
            let descriptor = context
                .as_context()
                .data()
                .descriptors
                .get(fd)
                .ok_or(BADF)?
                .clone();
            let Some(count) = descriptor.write(bytes)? else {
                return Ok(None);
            };
            (*result_ptr, count)
        }
    };
    memory
        .write(
            &mut context,
            result_ptr as usize,
            &(count as u32).to_le_bytes(),
        )
        .map_err(|_| FAULT)?;
    Ok(Some(()))
}
impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WASM process suspended")
    }
}
impl core::error::Error for Action {}
impl wasmi::errors::HostError for Action {}

pub(crate) fn memory(caller: &Caller<'_, Context>) -> Result<wasmi::Memory, i32> {
    caller
        .get_export("memory")
        .and_then(wasmi::Extern::into_memory)
        .ok_or(FAULT)
}

pub(crate) fn range(ptr: u32, len: usize, size: usize) -> Result<Range<usize>, i32> {
    let start = ptr as usize;
    let end = start.checked_add(len).ok_or(FAULT)?;
    if end > size {
        return Err(FAULT);
    }
    Ok(start..end)
}

fn write_u32(caller: &mut Caller<'_, Context>, ptr: u32, value: u32) -> Result<(), i32> {
    memory(caller)?
        .write(caller, ptr as usize, &value.to_le_bytes())
        .map_err(|_| FAULT)
}

pub(crate) fn vectors(
    caller: &Caller<'_, Context>,
    ptr: u32,
    count: u32,
) -> Result<Vec<Range<usize>>, i32> {
    if count > 1024 {
        return Err(INVAL);
    }
    let memory = memory(caller)?;
    let data = memory.data(caller);
    let table = range(ptr, count as usize * 8, data.len())?;
    data[table]
        .chunks_exact(8)
        .map(|entry| {
            let ptr = u32::from_le_bytes(entry[..4].try_into().expect("Four bytes"));
            let len = u32::from_le_bytes(entry[4..].try_into().expect("Four bytes"));
            range(ptr, len as usize, data.len())
        })
        .collect()
}

fn fd_write(
    mut caller: Caller<'_, Context>,
    fd: u32,
    ptr: u32,
    count: u32,
    written: u32,
) -> Result<i32, wasmi::Error> {
    if let Some(io::Descriptor::File {
        handle,
        write: true,
        ..
    }) = caller.data().descriptors.get(&fd)
    {
        let handle = handle.clone();
        let result = (|| {
            let memory = memory(&caller)?;
            range(written, 4, memory.data_size(&caller))?;
            // Validate every vector before the first file operation.
            let vectors = vectors(&caller, ptr, count)?;
            vectors.iter().try_fold(0u32, |sum, vector| {
                sum.checked_add(vector.len() as u32).ok_or(INVAL)
            })?;
            let mut total = 0u32;
            let mut handle = handle.borrow_mut();
            'vectors: for vector in vectors {
                for bytes in memory.data(&caller)[vector].chunks(io::PIPE_CAPACITY) {
                    match handle.write(bytes) {
                        Ok(count) => {
                            assert!(count <= bytes.len(), "Host file write exceeds buffer");
                            total += count as u32;
                            if count < bytes.len() {
                                break 'vectors;
                            }
                        }
                        Err(_) if total != 0 => break 'vectors,
                        Err(failure) => return Err(failure.errno()),
                    }
                }
            }
            memory
                .write(&mut caller, written as usize, &total.to_le_bytes())
                .map_err(|_| FAULT)
        })();
        return Ok(result.err().unwrap_or(0));
    }
    let result = (|| {
        if !caller
            .data()
            .descriptors
            .get(&fd)
            .is_some_and(io::Descriptor::writable)
        {
            return Err(
                if matches!(
                    caller.data().descriptors.get(&fd),
                    Some(io::Descriptor::File { .. })
                ) {
                    76
                } else {
                    BADF
                },
            );
        }
        let memory = memory(&caller)?;
        range(written, 4, memory.data_size(&caller))?;
        let vectors = vectors(&caller, ptr, count)?;
        let total = vectors.iter().try_fold(0usize, |sum, range| {
            sum.checked_add(range.len()).ok_or(INVAL)
        })?;
        if total > MAX_OUTPUT_BYTES
            && matches!(
                caller.data().descriptors.get(&fd),
                Some(io::Descriptor::Control(_))
            )
        {
            return Err(INVAL);
        }
        let mut output = Vec::with_capacity(total.min(MAX_OUTPUT_BYTES));
        for range in vectors {
            let count = range.len().min(MAX_OUTPUT_BYTES - output.len());
            output.extend_from_slice(&memory.data(&caller)[range.start..range.start + count]);
            if output.len() == MAX_OUTPUT_BYTES {
                break;
            }
        }
        Ok((
            memory,
            IoRequest::Write {
                fd,
                bytes: output,
                result_ptr: written,
            },
        ))
    })();
    if let Ok((_, IoRequest::Write { bytes, .. })) = &result
        && matches!(
            caller.data().descriptors.get(&fd),
            Some(io::Descriptor::Control(_))
        )
    {
        return Err(wasmi::Error::host(Action::Control {
            fd,
            bytes: bytes.clone(),
            written,
        }));
    }
    complete_or_suspend(&mut caller, result)
}

fn fd_read(
    mut caller: Caller<'_, Context>,
    fd: u32,
    ptr: u32,
    count: u32,
    read: u32,
) -> Result<i32, wasmi::Error> {
    let result = (|| {
        if !caller
            .data()
            .descriptors
            .get(&fd)
            .is_some_and(io::Descriptor::readable)
        {
            return Err(
                if matches!(
                    caller.data().descriptors.get(&fd),
                    Some(io::Descriptor::File { .. })
                ) {
                    76
                } else {
                    BADF
                },
            );
        }
        let memory = memory(&caller)?;
        range(read, 4, memory.data_size(&caller))?;
        let vectors = vectors(&caller, ptr, count)?;
        Ok((
            memory,
            IoRequest::Read {
                fd,
                vectors,
                result_ptr: read,
            },
        ))
    })();
    complete_or_suspend(&mut caller, result)
}

fn complete_or_suspend(
    caller: &mut Caller<'_, Context>,
    request: Result<(wasmi::Memory, IoRequest), i32>,
) -> Result<i32, wasmi::Error> {
    let (memory, request) = match request {
        Ok(request) => request,
        Err(errno) => return Ok(errno),
    };
    match perform_io(caller, memory, &request) {
        Ok(Some(())) => Ok(0),
        Ok(None) => Err(wasmi::Error::host(Action::Io(request))),
        Err(errno) => Ok(errno),
    }
}

fn tcp_connect(
    mut caller: Caller<'_, Context>,
    address: u32,
    port: u32,
    result_ptr: u32,
) -> Result<i32, wasmi::Error> {
    let result = (|| {
        let memory = memory(&caller)?;
        let bytes = range(address, 4, memory.data_size(&caller))?;
        range(result_ptr, 4, memory.data_size(&caller))?;
        if port == 0 || port > u16::MAX as u32 {
            return Err(INVAL);
        }
        let address = memory.data(&caller)[bytes].try_into().expect("Four octets");
        let fd = (0..io::MAX_DESCRIPTORS)
            .find(|fd| !caller.data().descriptors.contains_key(fd))
            .ok_or(io::MFILE)?;
        let network = caller.data().network.clone().ok_or(58)?;
        let connection = network
            .connect(address, port as u16)
            .map_err(crate::network::NetworkError::errno)?;
        caller.data_mut().descriptors.insert(
            fd,
            io::Descriptor::Socket {
                connection: Rc::new(RefCell::new(crate::socket::Socket::connecting(connection))),
                flags: Rc::new(Cell::new(0)),
            },
        );
        Ok((memory, IoRequest::Connect { fd, result_ptr }))
    })();
    complete_or_suspend(&mut caller, result)
}

fn tcp_open(mut caller: Caller<'_, Context>, flags: u32, result_ptr: u32) -> i32 {
    let result = (|| {
        let memory = memory(&caller)?;
        range(result_ptr, 4, memory.data_size(&caller))?;
        if flags & !(4 | 0x10000) != 0 {
            return Err(INVAL);
        }
        let fd = (0..io::MAX_DESCRIPTORS)
            .find(|fd| !caller.data().descriptors.contains_key(fd))
            .ok_or(io::MFILE)?;
        caller.data_mut().descriptors.insert(
            fd,
            io::Descriptor::Socket {
                connection: Rc::new(RefCell::new(crate::socket::Socket::new())),
                flags: Rc::new(Cell::new((flags & 4) as u16)),
            },
        );
        if flags & 0x10000 != 0 {
            caller.data_mut().close_on_exec.insert(fd);
        }
        write_u32(&mut caller, result_ptr, fd)
    })();
    result.err().unwrap_or(0)
}

fn tcp_connect_fd(
    mut caller: Caller<'_, Context>,
    fd: u32,
    address: u32,
    port: u32,
) -> Result<i32, wasmi::Error> {
    let request = (|| {
        let memory = memory(&caller)?;
        let bytes = range(address, 4, memory.data_size(&caller))?;
        if port == 0 || port > u16::MAX as u32 {
            return Err(INVAL);
        }
        let address = memory.data(&caller)[bytes].try_into().expect("Four octets");
        let (socket, nonblocking) = match caller.data().descriptors.get(&fd) {
            Some(io::Descriptor::Socket {
                connection, flags, ..
            }) => (connection.clone(), flags.get() & 4 != 0),
            Some(_) => return Err(57),
            None => return Err(BADF),
        };
        let network = caller.data().network.clone().ok_or(58)?;
        socket
            .borrow_mut()
            .begin(network.as_ref(), address, port as u16)?;
        Ok((memory, socket, nonblocking))
    })();
    let (memory, socket, nonblocking) = match request {
        Ok(request) => request,
        Err(errno) => return Ok(errno),
    };
    if nonblocking {
        return Ok(match socket.borrow_mut().poll_connect() {
            Ok(Some(())) => 0,
            Ok(None) => 26, // INPROGRESS; descriptor remains live.
            Err(error) => error.errno(),
        });
    }
    complete_or_suspend(&mut caller, Ok((memory, IoRequest::ConnectSocket { fd })))
}

fn tcp_error(mut caller: Caller<'_, Context>, fd: u32, result_ptr: u32) -> i32 {
    let result = (|| {
        let memory = memory(&caller)?;
        range(result_ptr, 4, memory.data_size(&caller))?;
        let socket = match caller.data().descriptors.get(&fd) {
            Some(io::Descriptor::Socket { connection, .. }) => connection.clone(),
            Some(_) => return Err(57),
            None => return Err(BADF),
        };
        let error = socket.borrow_mut().take_error();
        write_u32(&mut caller, result_ptr, error as u32)
    })();
    result.err().unwrap_or(0)
}

fn tcp_close(mut caller: Caller<'_, Context>, fd: u32) -> Result<i32, wasmi::Error> {
    let request = (|| {
        match caller.data().descriptors.get(&fd) {
            Some(io::Descriptor::Socket { .. }) => {}
            Some(_) => return Err(57), // NOTSOCK
            None => return Err(BADF),
        }
        Ok((memory(&caller)?, IoRequest::CloseSocket { fd }))
    })();
    complete_or_suspend(&mut caller, request)
}

fn resolve_ipv4(
    mut caller: Caller<'_, Context>,
    ptr: u32,
    len: u32,
    result_ptr: u32,
) -> Result<i32, wasmi::Error> {
    let request = (|| {
        let memory = memory(&caller)?;
        let input = range(ptr, len as usize, memory.data_size(&caller))?;
        range(result_ptr, 4, memory.data_size(&caller))?;
        let name = core::str::from_utf8(&memory.data(&caller)[input]).map_err(|_| INVAL)?;
        let name = crate::dns::hostname(name)?;
        let server = caller.data().dns_server.ok_or(58)?;
        let clock = caller.data().clock.clone().ok_or(58)?;
        let deadline = clock
            .now(ClockId::Realtime)
            .map_err(ClockError::errno)?
            .checked_add(10_000_000_000)
            .ok_or(61)?;
        let mut id = [0; 2];
        caller
            .data()
            .random
            .as_ref()
            .ok_or(58)?
            .fill(&mut id)
            .map_err(crate::random::RandomError::errno)?;
        let network = caller.data().network.clone().ok_or(58)?;
        let connection = network
            .connect(server.0, server.1)
            .map_err(crate::network::NetworkError::errno)?;
        assert!(
            caller.data().dns_query.is_none(),
            "Process has one outstanding DNS query"
        );
        caller.data_mut().dns_query = Some(crate::dns::Query::new(connection, name, id, deadline));
        Ok((memory, IoRequest::Resolve { result_ptr }))
    })();
    complete_or_suspend(&mut caller, request)
}

fn args_sizes(mut caller: Caller<'_, Context>, count: u32, bytes: u32) -> i32 {
    strings_sizes(&mut caller, count, bytes, false)
}

fn strings_sizes(
    caller: &mut Caller<'_, Context>,
    count: u32,
    bytes: u32,
    environment: bool,
) -> i32 {
    let result = (|| {
        let memory = memory(caller)?;
        range(count, 4, memory.data_size(&*caller))?;
        range(bytes, 4, memory.data_size(&*caller))?;
        let strings = if environment {
            &caller.data().environment
        } else {
            &caller.data().args
        };
        let count_value = strings.len() as u32;
        let size = strings.iter().map(|entry| entry.len() + 1).sum::<usize>() as u32;
        write_u32(caller, count, count_value)?;
        write_u32(caller, bytes, size)
    })();
    result.err().unwrap_or(0)
}

fn poll_oneoff(
    mut caller: Caller<'_, Context>,
    input: u32,
    output: u32,
    count: u32,
    result_ptr: u32,
) -> Result<i32, wasmi::Error> {
    let request = (|| {
        if count == 0 || count > 1024 {
            return Err(INVAL);
        }
        let memory = memory(&caller)?;
        let size = memory.data_size(&caller);
        let input = range(input, count as usize * 48, size)?;
        range(output, count as usize * 32, size)?;
        range(result_ptr, 4, size)?;
        let mut subscriptions = Vec::with_capacity(count as usize);
        for bytes in memory.data(&caller)[input].chunks_exact(48) {
            let userdata = u64::from_le_bytes(bytes[..8].try_into().expect("Eight bytes"));
            let kind = bytes[8];
            let fd = u32::from_le_bytes(bytes[16..20].try_into().expect("Four bytes"));
            let mut deadline = None;
            match kind {
                0 => {
                    let timeout =
                        u64::from_le_bytes(bytes[24..32].try_into().expect("Eight bytes"));
                    let flags = u16::from_le_bytes(bytes[40..42].try_into().expect("Two bytes"));
                    if fd > 3 || flags & !1 != 0 {
                        return Err(INVAL);
                    }
                    if timeout != 0 || flags != 0 {
                        let mut id = ClockId::decode(fd).map_err(ClockError::errno)?;
                        // Relative wall-clock waits measure elapsed time. Use
                        // the monotonic source so RTC granularity and wall-time
                        // corrections cannot shorten or extend a duration.
                        if flags == 0 && id == ClockId::Realtime {
                            id = ClockId::Monotonic;
                        }
                        let clock = caller.data().clock.as_ref().ok_or(58)?;
                        let now = clock.now(id).map_err(ClockError::errno)?;
                        let target = if flags == 1 {
                            timeout
                        } else {
                            now.checked_add(timeout).ok_or(61)?
                        };
                        deadline = Some((id, target));
                    }
                }
                1 | 2 => {}
                _ => return Err(INVAL),
            }
            subscriptions.push(Subscription {
                userdata,
                kind,
                fd,
                deadline,
            });
        }
        Ok((
            memory,
            IoRequest::Poll {
                subscriptions,
                events_ptr: output,
                result_ptr,
            },
        ))
    })();
    complete_or_suspend(&mut caller, request)
}

fn clock_value(mut caller: Caller<'_, Context>, id: u32, result_ptr: u32, resolution: bool) -> i32 {
    let result = (|| {
        let memory = memory(&caller)?;
        range(result_ptr, 8, memory.data_size(&caller))?;
        let id = ClockId::decode(id).map_err(ClockError::errno)?;
        let clock = caller.data().clock.as_ref().ok_or(58)?;
        let value = if resolution {
            clock.resolution(id)
        } else {
            clock.now(id)
        }
        .map_err(ClockError::errno)?;
        if resolution {
            assert_ne!(value, 0, "Clock resolution must be positive");
        }
        memory
            .write(&mut caller, result_ptr as usize, &value.to_le_bytes())
            .map_err(|_| FAULT)
    })();
    result.err().unwrap_or(0)
}

fn args_get(mut caller: Caller<'_, Context>, argv: u32, buffer: u32) -> i32 {
    strings_get(&mut caller, argv, buffer, false)
}

fn strings_get(caller: &mut Caller<'_, Context>, argv: u32, buffer: u32, environment: bool) -> i32 {
    let result = (|| {
        let memory = memory(caller)?;
        let args = if environment {
            caller.data().environment.clone()
        } else {
            caller.data().args.clone()
        };
        range(argv, args.len() * 4, memory.data_size(&*caller))?;
        range(
            buffer,
            args.iter().map(|arg| arg.len() + 1).sum(),
            memory.data_size(&*caller),
        )?;
        let mut cursor = buffer;
        for (index, arg) in args.iter().enumerate() {
            write_u32(caller, argv + index as u32 * 4, cursor)?;
            memory
                .write(&mut *caller, cursor as usize, arg.as_bytes())
                .map_err(|_| FAULT)?;
            cursor += arg.len() as u32;
            memory
                .write(&mut *caller, cursor as usize, &[0])
                .map_err(|_| FAULT)?;
            cursor += 1;
        }
        Ok(())
    })();
    result.err().unwrap_or(0)
}

fn spawn(caller: Caller<'_, Context>, ptr: u32, len: u32) -> Result<i32, wasmi::Error> {
    spawn_with_stdio(caller, ptr, len, None, None)
}

fn spawn_with_stdio(
    caller: Caller<'_, Context>,
    ptr: u32,
    len: u32,
    descriptors: Option<Vec<(u32, u32)>>,
    environment_range: Option<(u32, u32)>,
) -> Result<i32, wasmi::Error> {
    let parse = (|| {
        if descriptors.as_ref().is_some_and(|fds| {
            fds.iter()
                .any(|(_, fd)| !caller.data().descriptors.contains_key(fd))
        }) {
            return Err(-9);
        }
        if len == 0 {
            return Err(-22);
        }
        if len > 65536 {
            return Err(-7);
        }
        let memory = memory(&caller).map_err(|_| -14)?;
        let data = memory.data(&caller);
        let bytes = &data[range(ptr, len as usize, data.len()).map_err(|_| -14)?];
        if bytes.last() != Some(&0) {
            return Err(-22);
        }
        let args = bytes[..bytes.len() - 1]
            .split(|byte| *byte == 0)
            .map(|arg| core::str::from_utf8(arg).map(String::from).map_err(|_| -22))
            .collect::<Result<Vec<_>, _>>()?;
        if args[0].is_empty() {
            return Err(-22);
        }
        let environment = if let Some((environment_ptr, size)) = environment_range {
            if size > 65536 - len {
                return Err(-7);
            }
            let bytes =
                &data[range(environment_ptr, size as usize, data.len()).map_err(|_| -14)?];
            let entries = if bytes.is_empty() {
                Vec::new()
            } else {
                if bytes.last() != Some(&0) {
                    return Err(-22);
                }
                bytes[..bytes.len() - 1]
                    .split(|byte| *byte == 0)
                    .map(|entry| {
                        core::str::from_utf8(entry)
                            .map(String::from)
                            .map_err(|_| -22)
                    })
                    .collect::<Result<Vec<_>, _>>()?
            };
            if !crate::valid_environment(&entries) {
                return Err(-22);
            }
            Some(entries)
        } else {
            None
        };
        Ok((args, environment))
    })();
    match parse {
        Ok((args, environment)) => Err(wasmi::Error::host(Action::Spawn {
            args,
            environment,
            descriptors,
        })),
        Err(errno) => Ok(errno),
    }
}

pub(crate) fn linker(engine: &Engine) -> HostLinker {
    let mut linker = HostLinker::new(engine);
    crate::filesystem_abi::link(&mut linker);
    linker
        .func_wrap("efi_net", "tcp_open", tcp_open)
        .expect("Unique import");
    linker
        .func_wrap("efi_net", "tcp_connect_fd", tcp_connect_fd)
        .expect("Unique import");
    linker
        .func_wrap("efi_net", "tcp_error", tcp_error)
        .expect("Unique import");
    linker
        .func_wrap("efi_net", "tcp_connect", tcp_connect)
        .expect("Unique import");
    linker
        .func_wrap("efi_net", "tcp_close", tcp_close)
        .expect("Unique import");
    linker
        .func_wrap("efi_net", "resolve_ipv4", resolve_ipv4)
        .expect("Unique import");
    let wasi = "wasi_snapshot_preview1";
    linker
        .func_wrap(
            wasi,
            "sock_accept",
            |caller: Caller<'_, Context>, fd: u32, flags: u32, output: u32| -> i32 {
                let result: Result<(), i32> = (|| {
                    let memory = memory(&caller)?;
                    range(output, 4, memory.data_size(&caller))?;
                    if flags & !4 != 0 {
                        return Err(INVAL);
                    }
                    match caller.data().descriptors.get(&fd) {
                        Some(io::Descriptor::Socket { .. }) => Err(58),
                        Some(_) => Err(57),
                        None => Err(BADF),
                    }
                })();
                result.err().unwrap_or(0)
            },
        )
        .expect("Unique import");
    linker
        .func_wrap(
            wasi,
            "random_get",
            |mut caller: Caller<'_, Context>, ptr: u32, len: u32| -> i32 {
                let result = (|| {
                    let memory = memory(&caller)?;
                    let output = range(ptr, len as usize, memory.data_size(&caller))?;
                    if len as usize > crate::random::MAX_RANDOM_BYTES {
                        return Err(INVAL);
                    }
                    if len == 0 {
                        return Ok(());
                    }
                    let random = caller.data().random.clone().ok_or(58)?;
                    // Publish bytes only after complete generation. A provider
                    // error must not expose partial output in guest memory.
                    let mut bytes = vec![0; len as usize];
                    random
                        .fill(&mut bytes)
                        .map_err(crate::random::RandomError::errno)?;
                    memory.data_mut(&mut caller)[output].copy_from_slice(&bytes);
                    Ok(())
                })();
                result.err().unwrap_or(0)
            },
        )
        .expect("Unique import");
    linker
        .func_wrap(wasi, "fd_write", fd_write)
        .expect("Unique import");
    linker
        .func_wrap(wasi, "fd_read", fd_read)
        .expect("Unique import");
    linker
        .func_wrap(
            wasi,
            "sock_recv",
            |caller: Caller<'_, Context>,
             fd: u32,
             ptr: u32,
             count: u32,
             flags: u32,
             used: u32,
             output_flags: u32| {
                let result = (|| {
                    match caller.data().descriptors.get(&fd) {
                        Some(io::Descriptor::Socket { .. }) => (),
                        Some(_) => return Err(57),
                        None => return Err(BADF),
                    }
                    if flags & !3 != 0 {
                        return Err(INVAL);
                    }
                    vectors(&caller, ptr, count)?;
                    let size = memory(&caller)?.data_size(&caller);
                    range(used, 4, size)?;
                    range(output_flags, 2, size)?;
                    Err::<(), i32>(58)
                })();
                result.err().unwrap_or(0)
            },
        )
        .expect("Unique import");
    linker
        .func_wrap(
            wasi,
            "sock_send",
            |caller: Caller<'_, Context>, fd: u32, ptr: u32, count: u32, flags: u32, used: u32| {
                let result = (|| {
                    match caller.data().descriptors.get(&fd) {
                        Some(io::Descriptor::Socket { .. }) => (),
                        Some(_) => return Err(57),
                        None => return Err(BADF),
                    }
                    if flags != 0 {
                        return Err(INVAL);
                    }
                    vectors(&caller, ptr, count)?;
                    range(used, 4, memory(&caller)?.data_size(&caller))?;
                    Err::<(), i32>(58)
                })();
                result.err().unwrap_or(0)
            },
        )
        .expect("Unique import");
    linker
        .func_wrap(
            wasi,
            "sock_shutdown",
            |caller: Caller<'_, Context>, fd: u32, flags: u32| match caller
                .data()
                .descriptors
                .get(&fd)
            {
                None => BADF,
                Some(io::Descriptor::Socket { .. }) if flags == 0 || flags & !3 != 0 => INVAL,
                Some(io::Descriptor::Socket { .. }) => 58,
                Some(_) => 57,
            },
        )
        .expect("Unique import");
    linker
        .func_wrap(wasi, "poll_oneoff", poll_oneoff)
        .expect("Unique import");
    linker
        .func_wrap(
            wasi,
            "fd_seek",
            |mut caller: Caller<'_, Context>, fd: u32, offset: i64, whence: u32, ptr: u32| -> i32 {
                if !caller.data().descriptors.contains_key(&fd) {
                    return BADF;
                }
                if whence > 2 {
                    return INVAL;
                }
                if memory(&caller)
                    .and_then(|memory| range(ptr, 8, memory.data_size(&caller)))
                    .is_err()
                {
                    return FAULT;
                }
                let result = (|| {
                    let position = caller.data().descriptors[&fd].seek(offset, whence)?;
                    memory(&caller)?
                        .write(&mut caller, ptr as usize, &position.to_le_bytes())
                        .map_err(|_| FAULT)
                })();
                result.err().unwrap_or(0)
            },
        )
        .expect("Unique import");
    linker
        .func_wrap(
            wasi,
            "clock_time_get",
            |caller: Caller<'_, Context>, id: u32, _precision: u64, ptr: u32| {
                clock_value(caller, id, ptr, false)
            },
        )
        .expect("Unique import");
    linker
        .func_wrap(
            wasi,
            "clock_res_get",
            |caller: Caller<'_, Context>, id: u32, ptr: u32| clock_value(caller, id, ptr, true),
        )
        .expect("Unique import");
    linker
        .func_wrap(
            wasi,
            "fd_fdstat_get",
            |mut caller: Caller<'_, Context>, fd: u32, ptr: u32| -> i32 {
                let result = (|| {
                    let descriptor = caller.data().descriptors.get(&fd).ok_or(BADF)?;
                    let stat = descriptor.fdstat()?;
                    memory(&caller)?
                        .write(&mut caller, ptr as usize, &stat)
                        .map_err(|_| FAULT)
                })();
                result.err().unwrap_or(0)
            },
        )
        .expect("Unique import");
    linker
        .func_wrap(
            wasi,
            "fd_fdstat_set_flags",
            |caller: Caller<'_, Context>, fd: u32, flags: u32| -> i32 {
                let Some(descriptor) = caller.data().descriptors.get(&fd) else {
                    return BADF;
                };
                descriptor.set_flags(flags).err().unwrap_or(0)
            },
        )
        .expect("Unique import");
    linker
        .func_wrap(
            wasi,
            "fd_filestat_get",
            |mut caller: Caller<'_, Context>, fd: u32, ptr: u32| -> i32 {
                let result = (|| {
                    let descriptor = caller.data().descriptors.get(&fd).ok_or(BADF)?;
                    let stat = descriptor.filestat()?;
                    memory(&caller)?
                        .write(&mut caller, ptr as usize, &stat)
                        .map_err(|_| FAULT)
                })();
                result.err().unwrap_or(0)
            },
        )
        .expect("Unique import");
    linker
        .func_wrap(wasi, "args_sizes_get", args_sizes)
        .expect("Unique import");
    linker
        .func_wrap(wasi, "args_get", args_get)
        .expect("Unique import");
    linker
        .func_wrap(
            wasi,
            "environ_sizes_get",
            |mut caller: Caller<'_, Context>, count: u32, bytes: u32| -> i32 {
                strings_sizes(&mut caller, count, bytes, true)
            },
        )
        .expect("Unique import");
    linker
        .func_wrap(
            wasi,
            "environ_get",
            |mut caller: Caller<'_, Context>, pointers: u32, bytes: u32| -> i32 {
                strings_get(&mut caller, pointers, bytes, true)
            },
        )
        .expect("Unique import");
    linker
        .func_wrap(wasi, "proc_exit", |code: i32| -> Result<(), wasmi::Error> {
            Err(wasmi::Error::i32_exit(code))
        })
        .expect("Unique import");
    linker
        .func_wrap(wasi, "sched_yield", || -> Result<i32, wasmi::Error> {
            Err(wasmi::Error::host(Action::Yield))
        })
        .expect("Unique import");
    linker
        .func_wrap(
            "efi_process",
            "getpid",
            |caller: Caller<'_, Context>| -> i32 { caller.data().pid as i32 },
        )
        .expect("Unique import");
    linker
        .func_wrap(
            "efi_process",
            "getppid",
            |caller: Caller<'_, Context>| -> i32 { caller.data().parent as i32 },
        )
        .expect("Unique import");
    linker
        .func_wrap("efi_process", "spawn", spawn)
        .expect("Unique import");
    linker
        .func_wrap(
            "efi_process",
            "program_exists",
            |caller: Caller<'_, Context>, ptr: u32, len: u32| -> Result<i32, wasmi::Error> {
                let parse = (|| {
                    if len == 0 {
                        return Err(-22);
                    }
                    if len > 65536 {
                        return Err(-7);
                    }
                    let memory = memory(&caller).map_err(|_| -14)?;
                    let data = memory.data(&caller);
                    let bytes = &data[range(ptr, len as usize, data.len()).map_err(|_| -14)?];
                    let name = core::str::from_utf8(bytes).map_err(|_| -22)?;
                    if name.contains('\0') {
                        return Err(-22);
                    }
                    Ok(String::from(name))
                })();
                match parse {
                    Ok(name) => Err(wasmi::Error::host(Action::ProgramExists(name))),
                    Err(errno) => Ok(errno),
                }
            },
        )
        .expect("Unique import");
    linker
        .func_wrap(
            "efi_process",
            "spawn_fds",
            |caller: Caller<'_, Context>,
             ptr: u32,
             len: u32,
             input: u32,
             output: u32,
             error: u32| {
                spawn_with_stdio(
                    caller,
                    ptr,
                    len,
                    Some(vec![(0, input), (1, output), (2, error)]),
                    None,
                )
            },
        )
        .expect("Unique import");
    linker
        .func_wrap(
            "efi_process",
            "spawn_env_fds",
            |caller: Caller<'_, Context>,
             ptr: u32,
             len: u32,
             environment_ptr: u32,
             environment_len: u32,
             input: u32,
             output: u32,
             error: u32| {
                spawn_with_stdio(
                    caller,
                    ptr,
                    len,
                    Some(vec![(0, input), (1, output), (2, error)]),
                    Some((environment_ptr, environment_len)),
                )
            },
        )
        .expect("Unique import");
    linker
        .func_wrap(
            "efi_process",
            "spawn_mapped_fds",
            |caller: Caller<'_, Context>,
             ptr: u32,
             len: u32,
             environment_ptr: u32,
             environment_len: u32,
             mappings_ptr: u32,
             count: u32|
             -> Result<i32, wasmi::Error> {
                let parse = (|| {
                    if count > io::MAX_DESCRIPTORS - 1 {
                        return Err(-7);
                    }
                    let memory = memory(&caller).map_err(|_| -14)?;
                    let data = memory.data(&caller);
                    let bytes = &data
                        [range(mappings_ptr, count as usize * 8, data.len()).map_err(|_| -14)?];
                    let mut descriptors = Vec::with_capacity(count as usize);
                    for entry in bytes.chunks_exact(8) {
                        let target = u32::from_le_bytes(entry[..4].try_into().unwrap());
                        let source = u32::from_le_bytes(entry[4..].try_into().unwrap());
                        // fd 3 is the world's WASI preopened workspace.
                        if target == 3
                            || target >= io::MAX_DESCRIPTORS
                            || descriptors.iter().any(|(fd, _)| *fd == target)
                        {
                            return Err(-22);
                        }
                        descriptors.push((target, source));
                    }
                    Ok(descriptors)
                })();
                match parse {
                    Ok(descriptors) => spawn_with_stdio(
                        caller,
                        ptr,
                        len,
                        Some(descriptors),
                        Some((environment_ptr, environment_len)),
                    ),
                    Err(errno) => Ok(errno),
                }
            },
        )
        .expect("Unique import");
    linker
        .func_wrap(
            "efi_process",
            "input_snapshot",
            |mut caller: Caller<'_, Context>, ptr: u32, size: u32| -> i32 {
                let result = (|| {
                    if size as usize > MAX_OUTPUT_BYTES {
                        return Err(-7);
                    }
                    let memory = memory(&caller).map_err(|_| -14)?;
                    let data = memory.data(&caller);
                    let bytes = range(ptr, size as usize, data.len()).map_err(|_| -14)?;
                    let fd = (0..io::MAX_DESCRIPTORS)
                        .find(|fd| !caller.data().descriptors.contains_key(fd))
                        .ok_or(-24)?;
                    let input = io::Descriptor::Input(Rc::new(RefCell::new(
                        data[bytes].iter().copied().collect(),
                    )));
                    caller.data_mut().descriptors.insert(fd, input);
                    Ok(fd as i32)
                })();
                result.unwrap_or_else(|error| error)
            },
        )
        .expect("Unique import");
    linker
        .func_wrap(
            "efi_process",
            "pipe",
            |mut caller: Caller<'_, Context>, ptr: u32| -> i32 {
                let result = (|| {
                    let memory = memory(&caller).map_err(|_| -14)?;
                    range(ptr, 8, memory.data_size(&caller)).map_err(|_| -14)?;
                    let fds = io::pipe(&mut caller.data_mut().descriptors).map_err(|_| -24)?;
                    write_u32(&mut caller, ptr, fds[0]).expect("Validated pipe result");
                    write_u32(&mut caller, ptr + 4, fds[1]).expect("Validated pipe result");
                    Ok(())
                })();
                result.err().unwrap_or(0)
            },
        )
        .expect("Unique import");
    linker
        .func_wrap(
            "efi_process",
            "get_fd_flags",
            |caller: Caller<'_, Context>, fd: u32| -> i32 {
                if !caller.data().descriptors.contains_key(&fd) {
                    return -9;
                }
                i32::from(caller.data().close_on_exec.contains(&fd))
            },
        )
        .expect("Unique import");
    linker
        .func_wrap(
            "efi_process",
            "set_fd_flags",
            |mut caller: Caller<'_, Context>, fd: u32, flags: u32| -> i32 {
                if !caller.data().descriptors.contains_key(&fd) {
                    return -9;
                }
                if flags & !1 != 0 {
                    return -22;
                }
                if flags == 0 && caller.data().descriptors[&fd].is_private() {
                    return -22;
                }
                if flags == 0 {
                    caller.data_mut().close_on_exec.remove(&fd);
                } else {
                    caller.data_mut().close_on_exec.insert(fd);
                }
                0
            },
        )
        .expect("Unique import");
    linker
        .func_wrap(
            "efi_process",
            "dupfd",
            |mut caller: Caller<'_, Context>, old: u32, minimum: u32| -> i32 {
                if minimum >= io::MAX_DESCRIPTORS {
                    return -22;
                }
                let table = &mut caller.data_mut().descriptors;
                if table.get(&old).is_some_and(|fd| fd.is_private()) {
                    return -9;
                }
                let Some(descriptor) = table.get(&old).map(io::Descriptor::duplicate) else {
                    return -9;
                };
                let Some(fd) = (minimum..io::MAX_DESCRIPTORS).find(|fd| !table.contains_key(fd))
                else {
                    return -24;
                };
                table.insert(fd, descriptor);
                fd as i32
            },
        )
        .expect("Unique import");
    linker
        .func_wrap(
            "efi_process",
            "dup2",
            |mut caller: Caller<'_, Context>, old: u32, new: u32| -> i32 {
                match io::dup2(&mut caller.data_mut().descriptors, old, new) {
                    Ok(()) => {
                        if old != new {
                            caller.data_mut().close_on_exec.remove(&new);
                        }
                        new as i32
                    }
                    Err(_) => -9,
                }
            },
        )
        .expect("Unique import");
    linker
        .func_wrap(
            wasi,
            "fd_close",
            |mut caller: Caller<'_, Context>, fd: u32| -> i32 {
                if caller.data_mut().descriptors.remove(&fd).is_some() {
                    caller.data_mut().close_on_exec.remove(&fd);
                    0
                } else {
                    BADF
                }
            },
        )
        .expect("Unique import");
    linker
        .func_wrap(
            wasi,
            "fd_renumber",
            |mut caller: Caller<'_, Context>, old: u32, new: u32| -> i32 {
                let table = &mut caller.data_mut().descriptors;
                if new >= io::MAX_DESCRIPTORS || !table.contains_key(&old) {
                    return BADF;
                }
                if table[&old].is_private() || table.get(&new).is_some_and(|fd| fd.is_private()) {
                    return BADF;
                }
                if old != new {
                    let descriptor = table.remove(&old).expect("Descriptor checked");
                    table.insert(new, descriptor);
                    let marked = caller.data_mut().close_on_exec.remove(&old);
                    caller.data_mut().close_on_exec.remove(&new);
                    if marked {
                        caller.data_mut().close_on_exec.insert(new);
                    }
                }
                0
            },
        )
        .expect("Unique import");
    for (name, nohang) in [("waitpid", false), ("waitpid_nohang", true)] {
        linker
            .func_wrap(
                "efi_process",
                name,
                move |caller: Caller<'_, Context>,
                      child: u32,
                      status_ptr: u32|
                      -> Result<i32, wasmi::Error> {
                    let valid = memory(&caller)
                        .and_then(|memory| range(status_ptr, 4, memory.data_size(&caller)));
                    if valid.is_err() {
                        return Ok(-14);
                    }
                    Err(wasmi::Error::host(Action::Wait {
                        child,
                        status_ptr,
                        nohang,
                    }))
                },
            )
            .expect("Unique import");
    }
    linker
}
