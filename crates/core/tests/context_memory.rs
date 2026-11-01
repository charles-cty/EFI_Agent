//! Measure requested live bytes with firmware-style allocate/copy/free growth.
use efi_agent_core::{
    agent::{Agent, Environment},
    budget::{Limits, Purpose},
    model::ApiFormat,
    protocol::{ChatMessage, Operation},
    request,
    ui::App,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicUsize, Ordering},
};

struct Counter;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
#[global_allocator]
static ALLOCATOR: Counter = Counter;
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(live, Ordering::Relaxed);
        }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let replacement =
            unsafe { self.alloc(Layout::from_size_align(size, layout.align()).unwrap()) };
        if !replacement.is_null() {
            unsafe {
                std::ptr::copy_nonoverlapping(pointer, replacement, layout.size().min(size));
                self.dealloc(pointer, layout);
            }
        }
        replacement
    }
}
struct Model {
    api: ApiFormat,
    summaries: usize,
}
impl Environment for Model {
    fn request_bytes(&self, messages: &[ChatMessage], purpose: Purpose) -> Result<usize, String> {
        request::bytes(self.api, "memory", messages, "medium", purpose)
    }
    fn complete(
        &mut self,
        messages: &[ChatMessage],
        purpose: Purpose,
        _: &mut dyn FnMut(&str),
    ) -> Result<ChatMessage, String> {
        let body = request::build(self.api, "memory", messages, "medium", purpose)?;
        let encoded = request::encode(&body, &Limits::default())?;
        assert!(encoded.len() <= Limits::default().request_bytes);
        if purpose == Purpose::Summary {
            self.summaries += 1;
            Ok(ChatMessage::text(
                "assistant",
                "Completed earlier work; preserve current state.".into(),
            ))
        } else {
            Ok(ChatMessage::text("assistant", "text 中\n".repeat(65536)))
        }
    }
    fn execute(&mut self, _: Operation) -> Result<String, String> {
        panic!("No tool effects in memory measurement")
    }
}

#[test]
fn repeated_turns_compaction_and_clear_release_history_allocations() {
    for api in [ApiFormat::ChatCompletions, ApiFormat::Responses] {
        let mut agent = Agent::default();
        let mut ui = App::default();
        let mut model = Model { api, summaries: 0 };
        let baseline = LIVE.load(Ordering::Relaxed);
        PEAK.store(baseline, Ordering::Relaxed);
        for _ in 0..12 {
            agent
                .turn("continue".into(), &mut model, |event| {
                    if let efi_agent_core::agent::Event::Assistant(text) = event {
                        ui.message("assistant", text.into());
                    }
                })
                .unwrap();
            ui.prune_history();
        }
        let retained = LIVE.load(Ordering::Relaxed);
        agent.compact(&mut model, |_| {}).unwrap();
        let compacted = LIVE.load(Ordering::Relaxed);
        assert!(compacted < retained);
        assert!(model.summaries > 0);
        agent.clear();
        ui.clear_messages();
        let cleared = LIVE.load(Ordering::Relaxed);
        assert!(
            cleared <= baseline + 16384,
            "Retained allocation after clear: {cleared} vs {baseline}"
        );
        println!(
            "CONTEXT_MEMORY api={} baseline={baseline} retained={retained} compacted={compacted} cleared={cleared} peak={}",
            api.label(),
            PEAK.load(Ordering::Relaxed)
        );
    }
}
