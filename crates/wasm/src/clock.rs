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

/// Converts a constant-rate, wrapping 64-bit hardware counter to elapsed time.
/// Read at least once per full counter period. Fractions are retained across
/// samples, so frequent polling does not accumulate rounding error.
pub struct CounterClock {
    numerator: u64,
    denominator: u64,
    state: core::cell::Cell<(u64, u128)>,
}

impl CounterClock {
    pub fn new(initial: u64, numerator: u64, denominator: u64) -> Result<Self, ClockError> {
        if numerator == 0 || denominator == 0 {
            return Err(ClockError::Unavailable);
        }
        Ok(Self {
            numerator,
            denominator,
            state: core::cell::Cell::new((initial, 0)),
        })
    }

    pub fn sample(&self, value: u64) -> Result<u64, ClockError> {
        let (previous, elapsed) = self.state.get();
        let elapsed = elapsed
            .checked_add(u128::from(value.wrapping_sub(previous)))
            .ok_or(ClockError::Overflow)?;
        let ns = elapsed
            .checked_mul(u128::from(self.numerator))
            .ok_or(ClockError::Overflow)?
            / u128::from(self.denominator);
        let ns = u64::try_from(ns).map_err(|_| ClockError::Overflow)?;
        self.state.set((value, elapsed));
        Ok(ns)
    }

    pub fn resolution(&self) -> u64 {
        self.numerator.div_ceil(self.denominator)
    }
}

#[cfg(test)]
mod counter_tests {
    use super::*;

    #[test]
    fn hardware_counter_wrap_and_fractional_samples_match_total_elapsed_time() {
        let clock = CounterClock::new(u64::MAX - 2, 1_000_000_000, 3).unwrap();
        assert_eq!(clock.sample(u64::MAX - 2), Ok(0));
        assert_eq!(clock.sample(u64::MAX - 1), Ok(333_333_333));
        assert_eq!(clock.sample(u64::MAX), Ok(666_666_666));
        assert_eq!(clock.sample(0), Ok(1_000_000_000));
        assert_eq!(clock.sample(1), Ok(1_333_333_333));
        assert_eq!(clock.resolution(), 333_333_334);

        let clock = CounterClock::new(71, 333_333, 1_000_000).unwrap();
        for ticks in 0..10001 {
            assert_eq!(clock.sample(71 + ticks), Ok(ticks * 333_333 / 1_000_000));
        }
        assert_eq!(clock.resolution(), 1);
    }

    #[test]
    fn invalid_rates_and_elapsed_overflow_fail_without_resetting_the_epoch() {
        assert!(matches!(
            CounterClock::new(0, 0, 1),
            Err(ClockError::Unavailable)
        ));
        assert!(matches!(
            CounterClock::new(0, 1, 0),
            Err(ClockError::Unavailable)
        ));
        let clock = CounterClock::new(0, 2, 1).unwrap();
        assert_eq!(clock.sample(u64::MAX / 2), Ok(u64::MAX - 1));
        assert_eq!(clock.sample(u64::MAX / 2 + 1), Err(ClockError::Overflow));
        assert_eq!(clock.sample(u64::MAX / 2), Ok(u64::MAX - 1));
    }
}
