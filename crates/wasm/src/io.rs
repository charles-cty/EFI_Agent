use alloc::{
    boxed::Box,
    collections::{BTreeMap, VecDeque},
    rc::Rc,
    vec::Vec,
};
use core::cell::{Cell, RefCell};

use crate::MAX_OUTPUT_BYTES;
use crate::filesystem::{Directory, File, FileError};
use crate::network::Connection;

pub const PIPE_CAPACITY: usize = 64 * 1024;
pub const MAX_DESCRIPTORS: u32 = 128;
pub(crate) const BADF: i32 = 8;
pub(crate) const MFILE: i32 = 33;
pub(crate) const PIPE: i32 = 64;

pub(crate) struct Readiness {
    pub bytes: usize,
    pub hangup: bool,
    pub error: i32,
}

struct Pipe {
    bytes: VecDeque<u8>,
    readers: usize,
    writers: usize,
}

#[derive(Clone, Copy)]
enum Direction {
    Read,
    Write,
}

pub(crate) struct Endpoint {
    pipe: Rc<RefCell<Pipe>>,
    direction: Direction,
}

impl Endpoint {
    fn new(pipe: Rc<RefCell<Pipe>>, direction: Direction) -> Self {
        match direction {
            Direction::Read => pipe.borrow_mut().readers += 1,
            Direction::Write => pipe.borrow_mut().writers += 1,
        }
        Self { pipe, direction }
    }
}

impl Clone for Endpoint {
    fn clone(&self) -> Self {
        Self::new(self.pipe.clone(), self.direction)
    }
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        match self.direction {
            Direction::Read => self.pipe.borrow_mut().readers -= 1,
            Direction::Write => self.pipe.borrow_mut().writers -= 1,
        }
    }
}

#[derive(Clone)]
pub(crate) enum Descriptor {
    Control(Rc<RefCell<VecDeque<u8>>>),
    Input(Rc<RefCell<VecDeque<u8>>>),
    Output(Rc<RefCell<Vec<u8>>>),
    Pipe(Endpoint),
    File {
        handle: Rc<RefCell<Box<dyn File>>>,
        read: bool,
        write: bool,
        rights: u64,
        append: bool,
    },
    Directory(Directory),
    Socket {
        connection: Rc<RefCell<crate::socket::Socket>>,
        flags: Rc<Cell<u16>>,
    },
}

impl Descriptor {
    /// Descriptor duplication shares the underlying stream and socket status.
    pub(crate) fn duplicate(&self) -> Self {
        self.clone()
    }

    /// None means that an open writer can supply bytes later. Empty bytes mean EOF.
    pub(crate) fn read(&self, maximum: usize) -> Result<Option<Vec<u8>>, i32> {
        if !self.readable() {
            return Err(if matches!(self, Self::File { .. }) {
                76
            } else {
                BADF
            });
        }
        if maximum == 0 {
            return Ok(Some(Vec::new()));
        }
        match self {
            Self::Control(reply) => {
                let mut reply = reply.borrow_mut();
                let count = maximum.min(reply.len());
                Ok(Some(reply.drain(..count).collect()))
            }
            Self::Socket {
                connection, flags, ..
            } => {
                let bytes = connection
                    .borrow_mut()
                    .read(maximum)
                    .map_err(crate::network::NetworkError::errno)?;
                assert!(
                    bytes.as_ref().is_none_or(|bytes| bytes.len() <= maximum),
                    "Host socket read exceeds buffer"
                );
                if bytes.is_none() && flags.get() & 4 != 0 {
                    Err(6) // AGAIN
                } else {
                    Ok(bytes)
                }
            }
            Self::File { handle, .. } => {
                let mut bytes = alloc::vec![0; maximum];
                let count = handle
                    .borrow_mut()
                    .read(&mut bytes)
                    .map_err(FileError::errno)?;
                assert!(count <= maximum, "Host file read exceeds buffer");
                bytes.truncate(count);
                Ok(Some(bytes))
            }
            Self::Input(input) => {
                let mut input = input.borrow_mut();
                let count = maximum.min(input.len());
                Ok(Some(input.drain(..count).collect()))
            }
            Self::Pipe(endpoint) => {
                let mut pipe = endpoint.pipe.borrow_mut();
                if pipe.bytes.is_empty() && pipe.writers != 0 {
                    return Ok(None);
                }
                let count = maximum.min(pipe.bytes.len());
                Ok(Some(pipe.bytes.drain(..count).collect()))
            }
            Self::Output(_) | Self::Directory(_) => unreachable!("Readability checked"),
        }
    }

    /// None means a full output queue or pipe. Available space permits a short write.
    pub(crate) fn write(&self, bytes: &[u8]) -> Result<Option<usize>, i32> {
        if !self.writable() {
            return Err(if matches!(self, Self::File { .. }) {
                76
            } else {
                BADF
            });
        }
        if bytes.is_empty() {
            return Ok(Some(0));
        }
        match self {
            Self::Control(_) => unreachable!("Control writes are dispatched by the world"),
            Self::Socket {
                connection, flags, ..
            } => {
                let count = connection
                    .borrow_mut()
                    .write(bytes)
                    .map_err(crate::network::NetworkError::errno)?;
                assert!(
                    count.is_none_or(|count| count <= bytes.len()),
                    "Host socket write exceeds buffer"
                );
                if count.is_none() && flags.get() & 4 != 0 {
                    Err(6) // AGAIN
                } else {
                    Ok(count)
                }
            }
            Self::File { handle, .. } => {
                let count = handle.borrow_mut().write(bytes).map_err(FileError::errno)?;
                assert!(count <= bytes.len(), "Host file write exceeds buffer");
                Ok(Some(count))
            }
            Self::Output(output) => {
                let mut output = output.borrow_mut();
                let count = bytes.len().min(MAX_OUTPUT_BYTES - output.len());
                if count == 0 {
                    return Ok(None);
                }
                output.extend_from_slice(&bytes[..count]);
                Ok(Some(count))
            }
            Self::Pipe(endpoint) => {
                let mut pipe = endpoint.pipe.borrow_mut();
                if pipe.readers == 0 {
                    return Err(PIPE);
                }
                let count = bytes.len().min(PIPE_CAPACITY - pipe.bytes.len());
                if count == 0 {
                    return Ok(None);
                }
                pipe.bytes.extend(&bytes[..count]);
                Ok(Some(count))
            }
            Self::Input(_) | Self::Directory(_) => unreachable!("Writability checked"),
        }
    }

    pub(crate) fn readable(&self) -> bool {
        matches!(
            self,
            Self::Control(_)
                | Self::Socket { .. }
                | Self::File { read: true, .. }
                | Self::Input(_)
                | Self::Pipe(Endpoint {
                    direction: Direction::Read,
                    ..
                })
        )
    }

    pub(crate) fn writable(&self) -> bool {
        matches!(
            self,
            Self::Control(_)
                | Self::Socket { .. }
                | Self::File { write: true, .. }
                | Self::Output(_)
                | Self::Pipe(Endpoint {
                    direction: Direction::Write,
                    ..
                })
        )
    }

    pub(crate) fn fdstat(&self) -> [u8; 24] {
        let mut stat = [0u8; 24];
        match self {
            Self::File { rights, append, .. } => {
                stat[0] = 4;
                stat[2] = u8::from(*append);
                stat[8..16].copy_from_slice(&rights.to_le_bytes());
                return stat;
            }
            Self::Directory(directory) => {
                stat[0] = 3;
                stat[2..4].copy_from_slice(&directory.flags.to_le_bytes());
                stat[8..16].copy_from_slice(&directory.rights.to_le_bytes());
                stat[16..24].copy_from_slice(&directory.inheriting.to_le_bytes());
                return stat;
            }
            _ => {}
        }
        // WASI has no pipe type. Finite input and captured output are streams,
        // not terminals. libc uses CHARACTER_DEVICE to implement isatty.
        if let Self::Socket { flags, .. } = self {
            stat[0] = 6; // SOCKET_STREAM
            stat[2..4].copy_from_slice(&flags.get().to_le_bytes());
        }
        let mut rights = (1u64 << 27) | (1u64 << 21); // POLL_FD_READWRITE, FD_FILESTAT_GET
        if self.readable() {
            rights |= 1 << 1;
        } // FD_READ
        if self.writable() {
            rights |= 1 << 6;
        } // FD_WRITE
        if matches!(self, Self::Socket { .. }) {
            rights |= 1 << 3; // FD_FDSTAT_SET_FLAGS
        }
        stat[8..16].copy_from_slice(&rights.to_le_bytes());
        stat
    }

    pub(crate) fn set_flags(&self, requested: u32) -> Result<(), i32> {
        if requested & !0x1f != 0 {
            return Err(28);
        }
        if let Self::Socket { flags, .. } = self {
            if requested & !4 != 0 {
                return Err(58);
            }
            flags.set(requested as u16);
            return Ok(());
        }
        let stat = self.fdstat();
        if requested == u16::from_le_bytes([stat[2], stat[3]]) as u32 {
            Ok(())
        } else {
            Err(58)
        }
    }

    /// Inspect readiness without consuming bytes or retaining a pipe endpoint.
    pub(crate) fn readiness(&self, writing: bool) -> Option<Readiness> {
        let ready = |bytes, hangup, error| {
            Some(Readiness {
                bytes,
                hangup,
                error,
            })
        };
        if (writing && !self.writable()) || (!writing && !self.readable()) {
            return ready(
                0,
                false,
                if matches!(self, Self::File { .. }) {
                    76
                } else {
                    BADF
                },
            );
        }
        match self {
            Self::Control(reply) => ready(reply.borrow().len(), false, 0),
            Self::Socket { connection, .. } => match connection.borrow_mut().readiness(writing) {
                Ok(Some(state)) => ready(state.bytes, state.hangup, 0),
                Ok(None) => None,
                Err(error) => ready(0, false, error.errno()),
            },
            Self::File { handle, rights, .. } => {
                if rights & (1 << 27) == 0 {
                    return ready(0, false, 76);
                }
                let mut handle = handle.borrow_mut();
                let result = (|| {
                    let size = handle.metadata()?.size;
                    let position = handle.seek(0, 1)?;
                    Ok::<_, FileError>(
                        size.saturating_sub(position)
                            .try_into()
                            .unwrap_or(usize::MAX),
                    )
                })();
                match result {
                    Ok(bytes) => ready(if writing { 0 } else { bytes }, false, 0),
                    Err(error) => ready(0, false, error.errno()),
                }
            }
            Self::Directory(_) => unreachable!("Directory is not readable or writable"),
            Self::Input(input) => ready(input.borrow().len(), false, 0),
            Self::Output(output) => {
                let available = MAX_OUTPUT_BYTES - output.borrow().len();
                if available == 0 {
                    None
                } else {
                    ready(available, false, 0)
                }
            }
            Self::Pipe(endpoint) => {
                let pipe = endpoint.pipe.borrow();
                if writing {
                    if pipe.readers == 0 {
                        return ready(0, true, PIPE);
                    }
                    let available = PIPE_CAPACITY - pipe.bytes.len();
                    if available == 0 {
                        None
                    } else {
                        ready(available, false, 0)
                    }
                } else if pipe.bytes.is_empty() && pipe.writers != 0 {
                    None
                } else {
                    ready(pipe.bytes.len(), pipe.writers == 0, 0)
                }
            }
        }
    }

    pub(crate) fn filestat(&self) -> Result<[u8; 64], i32> {
        match self {
            Self::File { rights, .. } if rights & (1 << 21) == 0 => Err(76),
            Self::File { handle, .. } => Ok(handle
                .borrow_mut()
                .metadata()
                .map_err(FileError::errno)?
                .wasi()),
            Self::Directory(directory) if directory.rights & (1 << 21) == 0 => Err(76),
            Self::Directory(directory) => Ok(directory
                .filesystem
                .metadata(&directory.path)
                .map_err(FileError::errno)?
                .wasi()),
            _ => {
                let mut stat = [0; 64];
                stat[16] = self.fdstat()[0];
                stat[24..32].copy_from_slice(&1u64.to_le_bytes());
                Ok(stat)
            }
        }
    }

    pub(crate) fn seek(&self, offset: i64, whence: u32) -> Result<u64, i32> {
        match self {
            Self::File { handle, rights, .. } => {
                let required = if whence == 1 && offset == 0 {
                    1 << 5
                } else {
                    1 << 2
                };
                if rights & required == 0 {
                    return Err(76);
                }
                handle
                    .borrow_mut()
                    .seek(offset, whence)
                    .map_err(FileError::errno)
            }
            _ => Err(70),
        }
    }
}

pub(crate) const FILE_RIGHTS: u64 = (1 << 0)
    | (1 << 1)
    | (1 << 2)
    | (1 << 4)
    | (1 << 5)
    | (1 << 6)
    | (1 << 21)
    | (1 << 22)
    | (1 << 23)
    | (1 << 27);
pub(crate) const DIRECTORY_RIGHTS: u64 = (1 << 9)
    | (1 << 10)
    | (1 << 11)
    | (1 << 12)
    | (1 << 13)
    | (1 << 14)
    | (1 << 15)
    | (1 << 16)
    | (1 << 17)
    | (1 << 18)
    | (1 << 19)
    | (1 << 20)
    | (1 << 21)
    | (1 << 23)
    | (1 << 24)
    | (1 << 25)
    | (1 << 26);

pub(crate) type Table = BTreeMap<u32, Descriptor>;

pub(crate) fn pipe(table: &mut Table) -> Result<[u32; 2], i32> {
    let mut available = (0..MAX_DESCRIPTORS).filter(|fd| !table.contains_key(fd));
    let read = available.next().ok_or(MFILE)?;
    let write = available.next().ok_or(MFILE)?;
    let pipe = Rc::new(RefCell::new(Pipe {
        bytes: VecDeque::new(),
        readers: 0,
        writers: 0,
    }));
    table.insert(
        read,
        Descriptor::Pipe(Endpoint::new(pipe.clone(), Direction::Read)),
    );
    table.insert(
        write,
        Descriptor::Pipe(Endpoint::new(pipe, Direction::Write)),
    );
    Ok([read, write])
}

pub(crate) fn dup2(table: &mut Table, old: u32, new: u32) -> Result<(), i32> {
    if new >= MAX_DESCRIPTORS {
        return Err(BADF);
    }
    let descriptor = table.get(&old).ok_or(BADF)?;
    if old == new {
        return Ok(());
    }
    let descriptor = descriptor.duplicate();
    table.insert(new, descriptor);
    Ok(())
}
