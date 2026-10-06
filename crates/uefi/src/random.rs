//! One application-thread generator; callbacks must not request randomness.
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, Ordering},
};
use efi_agent_core::random::{Error, Generator};

struct Shared {
    busy: AtomicBool,
    generator: UnsafeCell<Generator>,
}
// SAFETY: the atomic guard protects all generator access, including reentry.
unsafe impl Sync for Shared {}

static RANDOM: Shared = Shared {
    busy: AtomicBool::new(false),
    generator: UnsafeCell::new(Generator::new()),
};

struct Guard;
impl Drop for Guard {
    fn drop(&mut self) {
        RANDOM.busy.store(false, Ordering::Release);
    }
}

pub fn fill(bytes: &mut [u8]) -> Result<(), Error> {
    if RANDOM
        .busy
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        return Err(Error::Busy);
    }
    let _guard = Guard;
    // SAFETY: the guard grants exclusive access until this call returns.
    unsafe { &mut *RANDOM.generator.get() }.fill(bytes)
}
