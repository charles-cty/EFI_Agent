use alloc::{format, string::String};
use efi_agent_wasm::clock::{Clock, ClockError, ClockId, CounterClock};
use uefi::{Status, StatusExt, boot, boot::ScopedProtocol, proto::misc::Timestamp};

/// PI 1.9, CPU Architectural Protocol. Unused function slots retain ABI order.
#[repr(C)]
#[derive(Debug)]
#[uefi::proto::unsafe_protocol("26baccb1-6f42-11d4-bce7-0080c73c8881")]
struct CpuArch {
    flush_data_cache: usize,
    enable_interrupt: usize,
    disable_interrupt: usize,
    get_interrupt_state: usize,
    init: usize,
    register_interrupt_handler: usize,
    get_timer_value: unsafe extern "efiapi" fn(*const CpuArch, u32, *mut u64, *mut u64) -> Status,
    set_memory_attributes: usize,
    number_of_timers: u32,
    dma_buffer_alignment: u32,
}

impl CpuArch {
    fn timer(&self) -> Result<(u64, u64), ClockError> {
        let mut value = 0;
        let mut period = 0;
        // SAFETY: The protocol guard retains the firmware interface. Timer 0
        // exists, and both output pointers refer to live, aligned u64 values.
        unsafe { (self.get_timer_value)(self, 0, &mut value, &mut period) }
            .to_result()
            .map_err(protocol_error)?;
        Ok((value, period))
    }
}

fn protocol_error(error: uefi::Error) -> ClockError {
    match error.status() {
        Status::NOT_FOUND | Status::UNSUPPORTED => ClockError::Unavailable,
        _ => ClockError::Io,
    }
}

enum CounterSource {
    Timestamp(ScopedProtocol<Timestamp>),
    Cpu(ScopedProtocol<CpuArch>),
}

struct Monotonic {
    source: CounterSource,
    elapsed: CounterClock,
}

impl Monotonic {
    fn new() -> Result<Self, ClockError> {
        match boot::get_handle_for_protocol::<Timestamp>() {
            Ok(handle) => {
                let source =
                    boot::open_protocol_exclusive::<Timestamp>(handle).map_err(protocol_error)?;
                let properties = source.get_properties().map_err(protocol_error)?;
                // Narrow counters can wrap repeatedly during synchronous file
                // operations or idle UI time. Require a full 64-bit counter.
                if properties.end_value != u64::MAX {
                    return Err(ClockError::Unavailable);
                }
                let elapsed =
                    CounterClock::new(source.get_timestamp(), 1_000_000_000, properties.frequency)?;
                Ok(Self {
                    source: CounterSource::Timestamp(source),
                    elapsed,
                })
            }
            Err(error) if error.status() == Status::NOT_FOUND => {
                let handle = boot::get_handle_for_protocol::<CpuArch>().map_err(protocol_error)?;
                let source =
                    boot::open_protocol_exclusive::<CpuArch>(handle).map_err(protocol_error)?;
                if source.number_of_timers == 0 {
                    return Err(ClockError::Unavailable);
                }
                let (initial, femtoseconds) = source.timer()?;
                // PI specifies zero for a timer without a predictable rate.
                let elapsed = CounterClock::new(initial, femtoseconds, 1_000_000)?;
                Ok(Self {
                    source: CounterSource::Cpu(source),
                    elapsed,
                })
            }
            Err(error) => Err(protocol_error(error)),
        }
    }

    fn now(&self) -> Result<u64, ClockError> {
        let value = match &self.source {
            CounterSource::Timestamp(source) => source.get_timestamp(),
            CounterSource::Cpu(source) => source.timer()?.0,
        };
        self.elapsed.sample(value)
    }
}

/// Wall time comes from RTC; elapsed time comes from a constant-rate firmware
/// hardware counter. Neither clock depends on dispatcher progress.
pub struct FirmwareClock {
    monotonic: Result<Monotonic, ClockError>,
}

impl FirmwareClock {
    pub fn new() -> Self {
        Self {
            monotonic: Monotonic::new(),
        }
    }

    pub fn description(&self) -> String {
        match &self.monotonic {
            Ok(clock) => {
                let source = match &clock.source {
                    CounterSource::Timestamp(_) => "UEFI Timestamp",
                    CounterSource::Cpu(_) => "PI CPU timer 0",
                };
                format!(
                    "Monotonic clock: {source}; nominal resolution {} ns",
                    clock.elapsed.resolution()
                )
            }
            Err(error) => format!("Monotonic clock: {error:?}"),
        }
    }
}

impl Clock for FirmwareClock {
    fn now(&self, id: ClockId) -> Result<u64, ClockError> {
        if id == ClockId::Monotonic {
            return self.monotonic.as_ref().map_err(|error| *error)?.now();
        }
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
        if id == ClockId::Monotonic {
            return Ok(self
                .monotonic
                .as_ref()
                .map_err(|error| *error)?
                .elapsed
                .resolution());
        }
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
