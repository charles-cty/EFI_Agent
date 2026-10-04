//! Single-processor cooperative dispatcher. Only notifications run above APPLICATION.
use alloc::{format, string::String};
use core::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use efi_agent_core::completion::CompletionQueue;
pub use efi_agent_core::completion::Notification;
use uefi::{
    Event,
    boot::{self, EventType, TimerTrigger, Tpl},
};

static PENDING: CompletionQueue = CompletionQueue::new();
static DISPATCHING: AtomicBool = AtomicBool::new(false);

pub fn enqueue(context: core::ptr::NonNull<Notification>) {
    // SAFETY: Completion retains its boxed node until CloseEvent and dispatch.
    unsafe { Notification::enqueue(context, &PENDING) };
}

struct DispatchGuard;
impl DispatchGuard {
    fn enter() -> Self {
        assert!(
            !DISPATCHING.swap(true, Ordering::AcqRel),
            "UEFI dispatcher must not be reentered"
        );
        Self
    }
}
impl Drop for DispatchGuard {
    fn drop(&mut self) {
        DISPATCHING.store(false, Ordering::Release);
    }
}

/// Dispatch notifications at APPLICATION, outside all EFI callbacks.
pub fn dispatch() {
    let _guard = DispatchGuard::enter();
    // SAFETY: the guarded APPLICATION consumer drains before any node is freed.
    unsafe { PENDING.drain() };
}

/// Yield the application processor to firmware while awaiting the next tick.
/// The entry point stays at APPLICATION; no application code raises TPL.
pub fn idle(duration: Duration) -> Result<(), String> {
    let _guard = DispatchGuard::enter();
    // SAFETY: the guarded APPLICATION consumer drains before any node is freed.
    unsafe { PENDING.drain() };
    let tick = Tick::new(duration)?;
    // NOTIFY_SIGNAL completion events are deliberately not passed to WaitForEvent.
    boot::wait_for_event(core::slice::from_ref(tick.0.as_ref().expect("Live tick")))
        .map_err(|e| format!("UEFI event wait: {e}"))?;
    // SAFETY: the guarded APPLICATION consumer drains before any node is freed.
    unsafe { PENDING.drain() };
    Ok(())
}

struct Tick(Option<Event>);
impl Tick {
    fn new(duration: Duration) -> Result<Self, String> {
        // SAFETY: this timer has no notification function or context.
        let event = unsafe { boot::create_event(EventType::TIMER, Tpl::APPLICATION, None, None) }
            .map_err(|e| format!("UEFI tick event: {e}"))?;
        let tick = Self(Some(event));
        boot::set_timer(
            tick.0.as_ref().expect("Live tick"),
            TimerTrigger::Relative(duration),
        )
        .map_err(|e| format!("UEFI tick timer: {e}"))?;
        Ok(tick)
    }
}
impl Drop for Tick {
    fn drop(&mut self) {
        if let Some(event) = self.0.take() {
            boot::close_event(event).expect("Cannot close UEFI tick event");
        }
    }
}
