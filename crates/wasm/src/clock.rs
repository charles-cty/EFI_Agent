//! Clock readings come from the host. The dispatcher never invents elapsed time.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockId {
    Realtime,
    Monotonic,
    ProcessCpu,
    ThreadCpu,
}

impl ClockId {
    pub(crate) fn decode(value: u32) -> Result<Self, ClockError> {
        match value {
            0 => Ok(Self::Realtime),
            1 => Ok(Self::Monotonic),
            2 => Ok(Self::ProcessCpu),
            3 => Ok(Self::ThreadCpu),
            _ => Err(ClockError::InvalidClock),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockError {
    InvalidClock,
    Unavailable,
    Io,
    Overflow,
}

impl ClockError {
    pub(crate) fn errno(self) -> i32 {
        match self {
            Self::InvalidClock => 28,
            Self::Unavailable => 58,
            Self::Io => 29,
            Self::Overflow => 61,
        }
    }
}

/// Realtime values use the Unix epoch. Other clock epochs are host-defined.
/// All timestamps and resolutions are nanoseconds. A monotonic provider must
/// never decrease. Unsupported clock types return `Unavailable`.
pub trait Clock {
    fn now(&self, id: ClockId) -> Result<u64, ClockError>;
    fn resolution(&self, id: ClockId) -> Result<u64, ClockError>;
}
