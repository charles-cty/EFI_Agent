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
    exact_growth_and_file_image_lifetime();
    translated_file_code_is_released_before_reap();
    custom_sections_do_not_enter_the_runtime_heap();
    let mut world = World::new();
    world
        .set_filesystem(std::rc::Rc::new(
            efi_agent_wasm::filesystem::namespace::Namespace::temporary(1024 * 1024, 128, 0, None),
        ))
        .unwrap();
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
        let mut arguments = if name == "return" {
            vec![String::new(); 32_769]
        } else {
            vec![String::new()]
        };
        arguments[0] = name.into();
        let environment = if name == "exit" {
            (0..8_000).map(|index| format!("V{index}=")).collect()
        } else {
            Vec::new()
        };
        let pid = world
            .spawn_with_environment(arguments, environment, vec![])
            .unwrap();
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
        if matches!(name, "return" | "exit") {
            assert!(
                terminal < baseline + 128 * 1024,
                "{name} kept startup-vector storage: {} bytes",
                terminal.saturating_sub(baseline)
            );
        }
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

fn custom_sections_do_not_enter_the_runtime_heap() {
    let mut bytes =
        wat::parse_str(r#"(module (memory (export "memory") 1) (func (export "_start")))"#)
            .unwrap();
    bytes.push(0);
    let mut size = 8 * 1024 * 1024 + 2;
    loop {
        let mut byte = (size & 127) as u8;
        size >>= 7;
        if size != 0 {
            byte |= 128;
        }
        bytes.push(byte);
        if size == 0 {
            break;
        }
    }
    bytes.extend_from_slice(&[1, b'd']);
    bytes.resize(bytes.len() + 8 * 1024 * 1024, 0);
    let mut world = World::new();
    let baseline = LIVE_BYTES.load(Ordering::Relaxed);
    world.register("debug", &bytes).unwrap();
    assert!(
        LIVE_BYTES.load(Ordering::Relaxed) < baseline + 1024 * 1024,
        "Unused custom section was retained in the runtime image"
    );
}

fn translated_file_code_is_released_before_reap() {
    use efi_agent_wasm::filesystem::{FileSystem, OpenOptions, namespace::Namespace};
    let fs = std::rc::Rc::new(Namespace::temporary(8 * 1024 * 1024, 128, 0, None));
    // Many unused functions make translated-code retention observable separately
    // from the single-page instance. Eager compilation must release them too.
    let body = "local.get 0 i32.const 1 i32.add local.set 0 ".repeat(64);
    let source = format!(
        "(module (memory (export \"memory\") 1) (func (export \"_start\")) {})",
        format!("(func (param i32) {body})").repeat(2048)
    );
    let bytes = wat::parse_str(source).unwrap();
    let mut file = fs
        .open(
            "code.wasm",
            OpenOptions {
                read: false,
                write: true,
                create: true,
                exclusive: false,
                truncate: true,
                append: false,
            },
        )
        .unwrap();
    file.write(&bytes).unwrap();
    drop(file);
    let mut world = World::new();
    world.set_filesystem(fs).unwrap();
    world.discover_programs().unwrap();
    let baseline = LIVE_BYTES.load(Ordering::Relaxed);
    for _ in 0..3 {
        let pid = world.spawn(vec!["code".into()], vec![]).unwrap();
        assert!(LIVE_BYTES.load(Ordering::Relaxed) > baseline + 1024 * 1024);
        world.step();
        assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
        assert!(
            LIVE_BYTES.load(Ordering::Relaxed) < baseline + 256 * 1024,
            "Translated code or Engine outlived its final execution"
        );
        world.reap(pid).unwrap();
    }
}

fn exact_growth_and_file_image_lifetime() {
    use efi_agent_wasm::filesystem::{FileSystem, OpenOptions, namespace::Namespace};
    let fs = std::rc::Rc::new(Namespace::temporary(1024 * 1024, 128, 0, None));
    let bytes = wat::parse_str(
        r#"(module
        (import "wasi_snapshot_preview1" "sched_yield" (func $yield (result i32)))
        (memory (export "memory") 640 1024)
        (func (export "_start")
            (if (i32.ne (memory.grow (i32.const 1)) (i32.const 640)) (then unreachable))
            (drop (call $yield))
            (if (i32.ne (memory.grow (i32.const 383)) (i32.const 641)) (then unreachable))
            (if (i32.ne (memory.grow (i32.const 1)) (i32.const -1)) (then unreachable))
            (if (i32.ne (memory.size) (i32.const 1024)) (then unreachable))))"#,
    )
    .unwrap();
    let mut file = fs
        .open(
            "grow.wasm",
            OpenOptions {
                read: false,
                write: true,
                create: true,
                exclusive: false,
                truncate: true,
                append: false,
            },
        )
        .unwrap();
    file.write(&bytes).unwrap();
    drop(file);
    let mut world = World::new();
    world.set_filesystem(fs).unwrap();
    world.discover_programs().unwrap();
    let baseline = LIVE_BYTES.load(Ordering::Relaxed);
    for cycle in 0..8 {
        let pid = world.spawn(vec!["grow".into()], vec![]).unwrap();
        let initial = LIVE_BYTES.load(Ordering::Relaxed);
        world.step();
        let grown = LIVE_BYTES.load(Ordering::Relaxed);
        assert_eq!(world.status(pid).unwrap(), &Status::Ready);
        assert!(
            grown < initial + 256 * 1024,
            "One page caused excess allocation"
        );
        for _ in 0..100 {
            if !world.status(pid).unwrap().is_live() {
                break;
            }
            world.step();
        }
        assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
        let terminal = LIVE_BYTES.load(Ordering::Relaxed);
        assert!(
            terminal < baseline + 256 * 1024,
            "Cycle {cycle} retained image or memory: {}",
            terminal.saturating_sub(baseline)
        );
        world.reap(pid).unwrap();
    }
}
