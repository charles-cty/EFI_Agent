use efi_agent_wasm::clock::{Clock, ClockError, ClockId};

/// Firmware RTC readings are exposed as realtime. No elapsed-time or CPU-time
/// value is derived from the number of dispatcher calls.
pub struct FirmwareClock;

impl Clock for FirmwareClock {
    fn now(&self, id: ClockId) -> Result<u64, ClockError> {
        if id != ClockId::Realtime {
            return Err(ClockError::Unavailable);
        }
        let time = uefi::runtime::get_time().map_err(|_| ClockError::Io)?;
        let seconds = efi_agent_core::clock::unix_seconds(
            time.year(),
            time.month(),
            time.day(),
            time.hour(),
            time.minute(),
            time.second(),
            time.time_zone().unwrap_or(0),
        )
        .ok_or(ClockError::Overflow)?;
        seconds
            .checked_mul(1_000_000_000)
            .and_then(|ns| ns.checked_add(u64::from(time.nanosecond())))
            .ok_or(ClockError::Overflow)
    }

    fn resolution(&self, id: ClockId) -> Result<u64, ClockError> {
        if id != ClockId::Realtime {
            return Err(ClockError::Unavailable);
        }
        let (_, caps) = uefi::runtime::get_time_and_caps().map_err(|_| ClockError::Io)?;
        if caps.resolution == 0 {
            return Err(ClockError::Unavailable);
        }
        Ok(1_000_000_000u64.div_ceil(u64::from(caps.resolution)))
    }
}
