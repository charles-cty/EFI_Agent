//! Native allocation and process-lifetime checks for real guest images.
use efi_agent_wasm::{Status, World};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};

pub struct CountingAllocator;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn added(size: usize) {
    let live = LIVE.fetch_add(size, Ordering::Relaxed) + size;
    PEAK.fetch_max(live, Ordering::Relaxed);
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            added(layout.size());
        }
        pointer
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            added(layout.size());
        }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // Match firmware allocate-copy-free behavior, including its transient peak.
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

pub fn run(world: &mut World, arguments: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let cycles: usize = arguments.first().ok_or("Expected cycle count")?.parse()?;
    let cancel_ms: u64 = arguments
        .get(1)
        .ok_or("Expected cancellation delay (0 disables)")?
        .parse()?;
    let command = arguments
        .get(2..)
        .filter(|args| !args.is_empty())
        .ok_or("Expected guest command")?;
    if command == ["session-js"] {
        return js_sessions(world, cycles);
    }
    let baseline = LIVE.load(Ordering::Relaxed);
    println!("cycle\tpeak_bytes\tterminal_bytes\treaped_bytes\tprocesses\tstatus");
    for cycle in 0..cycles {
        PEAK.store(LIVE.load(Ordering::Relaxed), Ordering::Relaxed);
        let pid = world.spawn(command.to_vec(), vec![])?;
        let started = Instant::now();
        while world.status(pid)?.is_live() {
            if started.elapsed() > Duration::from_secs(120) {
                return Err("Guest deadline exceeded".into());
            }
            if cancel_ms != 0 && started.elapsed() >= Duration::from_millis(cancel_ms) {
                // Cancel the entire ownership tree, as the tool frontend does.
                let children: Vec<_> = world
                    .process_ids()
                    .filter(|child| *child != pid && world.root(*child).unwrap() == pid)
                    .collect();
                world.cancel(pid)?;
                for child in children {
                    world.cancel(child)?;
                }
                break;
            }
            if world.step().is_none() {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        let status = world.status(pid)?.clone();
        let (stdout, stderr) = world.take_output(pid)?;
        let output_size = stdout.len() + stderr.len();
        drop((stdout, stderr));
        let terminal = LIVE.load(Ordering::Relaxed);
        let pids: Vec<_> = world.process_ids().collect();
        if cancel_ms == 0 && pids.len() != 1 {
            return Err(format!("Root left {} unreaped descendants", pids.len() - 1).into());
        }
        for child in pids {
            if world.status(child)?.is_live() {
                return Err(format!("Live descendant {child} after root exit").into());
            }
            world.reap(child)?;
        }
        let retained = LIVE.load(Ordering::Relaxed).saturating_sub(baseline);
        println!(
            "{cycle}\t{}\t{}\t{retained}\t{}\t{status:?} output={output_size}",
            PEAK.load(Ordering::Relaxed).saturating_sub(baseline),
            terminal.saturating_sub(baseline),
            world.process_ids().count()
        );
        if retained > 1024 * 1024 {
            return Err(
                format!("Cycle {cycle} retained more than 1 MiB after reap: {retained}").into(),
            );
        }
        let expected = if cancel_ms == 0 { 0 } else { 130 };
        if status != Status::Exited(expected) {
            return Err(format!("Unexpected guest status: {status:?}").into());
        }
    }
    Ok(())
}

fn js_sessions(world: &mut World, cycles: usize) -> Result<(), Box<dyn std::error::Error>> {
    let baseline = LIVE.load(Ordering::Relaxed);
    println!("cycle\tinitialized_bytes\tterminal_bytes\tstatus");
    for cycle in 0..cycles {
        let mut session = world.start_session(efi_agent_wasm::session::SessionKind::JavaScript)?;
        session.submit("let retained = 731;")?;
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if Instant::now() > deadline {
                return Err("JavaScript session initialization timed out".into());
            }
            world.step();
            if let Some(status) = session.take_completion() {
                if status != 0 {
                    return Err("JavaScript session initialization failed".into());
                }
                break;
            }
            if !world.status(session.pid())?.is_live() {
                return Err("JavaScript session exited early".into());
            }
        }
        let initialized = LIVE.load(Ordering::Relaxed).saturating_sub(baseline);
        if !(1024 * 1024..=20 * 1024 * 1024).contains(&initialized) {
            return Err("Expected initialized QuickJS below 20 MiB".into());
        }
        let pid = session.pid();
        if cycle.is_multiple_of(2) {
            session.submit(r#"if (retained !== 731) throw Error("Lost state"); quit(0);"#)?;
            while world.status(pid)?.is_live() {
                if Instant::now() > deadline {
                    return Err("JavaScript session exit timed out".into());
                }
                world.step();
            }
        } else {
            world.cancel(pid)?;
        }
        let expected = Status::Exited(if cycle.is_multiple_of(2) { 0 } else { 130 });
        if world.status(pid)? != &expected {
            return Err("Unexpected JavaScript session exit status".into());
        }
        drop(session);
        drop(world.take_output(pid)?);
        let terminal = LIVE.load(Ordering::Relaxed).saturating_sub(baseline);
        if terminal > 1024 * 1024 {
            return Err(format!("JavaScript session kept {terminal} bytes").into());
        }
        world.reap(pid)?;
        if world.process_ids().next().is_some() {
            return Err("JavaScript session left descendants".into());
        }
        println!("{cycle}\t{initialized}\t{terminal}\t{expected:?}");
    }
    Ok(())
}
