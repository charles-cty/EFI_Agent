use efi_agent_wasm::{Status, World};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

// This test binary has one test. Count live allocation bytes so status-only
// success cannot conceal linear memory retained until wait/reap.
struct CountingAllocator;
static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            LIVE_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            LIVE_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
        LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let replacement = unsafe { System.realloc(pointer, layout, size) };
        if !replacement.is_null() {
            if size >= layout.size() {
                LIVE_BYTES.fetch_add(size - layout.size(), Ordering::Relaxed);
            } else {
                LIVE_BYTES.fetch_sub(layout.size() - size, Ordering::Relaxed);
            }
        }
        replacement
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[test]
fn all_terminal_paths_release_execution_memory_before_reap_and_keep_output() {
    let mut world = World::new();
    for (name, ending) in [
        ("return", ""),
        ("exit", "(call $exit (i32.const 29))"),
        ("trap", "unreachable"),
        ("cancel", "(loop $forever (br $forever))"),
    ] {
        let source = format!(
            r#"(module
              (import "wasi_snapshot_preview1" "sched_yield" (func $yield (result i32)))
              (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
              (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
              (memory (export "memory") 1024)
              (data (i32.const 32) "saved")
              (func (export "_start")
                (i32.store8 (i32.const 67108863) (i32.const 73))
                (i32.store (i32.const 0) (i32.const 32))
                (i32.store (i32.const 4) (i32.const 5))
                (drop (call $write (i32.const 1) (i32.const 0) (i32.const 1) (i32.const 8)))
                (drop (call $write (i32.const 2) (i32.const 0) (i32.const 1) (i32.const 8)))
                (drop (call $yield))
                {ending}))"#
        );
        world
            .register(name, &wat::parse_str(source).unwrap())
            .unwrap();
    }
    let mut zombies = Vec::new();
    for name in ["return", "exit", "trap", "cancel"] {
        let baseline = LIVE_BYTES.load(Ordering::Relaxed);
        let pid = world.spawn(vec![name.into()], vec![]).unwrap();
        world.step();
        assert_eq!(world.status(pid).unwrap(), &Status::Ready);
        let running = LIVE_BYTES.load(Ordering::Relaxed);
        assert!(running > baseline + 60 * 1024 * 1024);
        if name == "cancel" {
            world.cancel(pid).unwrap();
        } else {
            for _ in 0..1000 {
                if !world.status(pid).unwrap().is_live() {
                    break;
                }
                world.step();
            }
        }
        assert!(!world.status(pid).unwrap().is_live());
        let terminal = LIVE_BYTES.load(Ordering::Relaxed);
        assert!(
            terminal + 60 * 1024 * 1024 < running,
            "{name} kept execution memory"
        );
        match name {
            "return" => assert_eq!(world.status(pid).unwrap(), &Status::Exited(0)),
            "exit" => assert_eq!(world.status(pid).unwrap(), &Status::Exited(29)),
            "cancel" => assert_eq!(world.status(pid).unwrap(), &Status::Exited(130)),
            "trap" => assert!(matches!(world.status(pid).unwrap(), Status::Trapped(_))),
            _ => unreachable!(),
        }
        assert_eq!(
            world.take_output(pid).unwrap(),
            (b"saved".to_vec(), b"saved".to_vec())
        );
        assert_eq!(world.take_output(pid).unwrap(), (vec![], vec![]));
        // Cancellation of an already terminal record is idempotent.
        let status = world.status(pid).unwrap().clone();
        world.cancel(pid).unwrap();
        assert_eq!(world.status(pid).unwrap(), &status);
        zombies.push((pid, status));
    }
    assert_eq!(world.step(), None);
    assert_eq!(world.process_ids().count(), zombies.len());
    for (pid, status) in zombies {
        assert_eq!(world.reap(pid).unwrap(), status);
    }
    assert_eq!(world.process_ids().count(), 0);
}
