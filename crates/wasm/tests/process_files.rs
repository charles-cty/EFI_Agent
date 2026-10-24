use efi_agent_wasm::clock::{Clock, ClockError, ClockId};
use efi_agent_wasm::{
    Status, World,
    filesystem::{FileError, FileSystem, OpenOptions, namespace::Namespace},
};
use std::cell::Cell;
use std::rc::Rc;

struct Counter(Cell<u64>);

impl Clock for Counter {
    fn now(&self, id: ClockId) -> Result<u64, ClockError> {
        if self.0.get() == u64::MAX {
            return Err(ClockError::Io);
        }
        match id {
            ClockId::Realtime | ClockId::Monotonic => Ok(self.0.get()),
            _ => Err(ClockError::Unavailable),
        }
    }
    fn resolution(&self, _id: ClockId) -> Result<u64, ClockError> {
        Ok(1)
    }
}

#[test]
fn uptime_rejects_missing_clocks_and_failed_transition_accounting() {
    let mut world = World::new();
    world
        .set_filesystem(Rc::new(Namespace::temporary(1024 * 1024, 128, 0, None)))
        .unwrap();
    let root = world.filesystem().unwrap();
    let options = OpenOptions {
        read: true,
        write: false,
        create: false,
        exclusive: false,
        truncate: false,
        append: false,
    };
    assert_eq!(
        root.open("proc/uptime", options).err(),
        Some(FileError::Unsupported)
    );
    let clock = Rc::new(Counter(Cell::new(1_000_000_000)));
    world.set_clock(clock.clone());
    let wasm = wat::parse_str(r#"(module (memory (export "memory") 1) (func (export "_start")))"#)
        .unwrap();
    world.register("probe", &wasm).unwrap();
    clock.0.set(u64::MAX);
    world.spawn(vec!["probe".into()], Vec::new()).unwrap();
    assert_eq!(root.open("proc/uptime", options).err(), Some(FileError::Io));
    clock.0.set(2_000_000_000);
    assert_eq!(root.open("proc/uptime", options).err(), Some(FileError::Io));
}

fn read(filesystem: &dyn FileSystem, path: &str) -> Vec<u8> {
    let mut file = filesystem
        .open(
            path,
            OpenOptions {
                read: true,
                write: false,
                create: false,
                exclusive: false,
                truncate: false,
                append: false,
            },
        )
        .unwrap();
    let mut bytes = Vec::new();
    let mut buffer = [0; 256];
    loop {
        let count = file.read(&mut buffer).unwrap();
        if count == 0 {
            return bytes;
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
}

#[test]
fn uptime_measures_elapsed_and_scheduler_idle_time_from_a_nonzero_epoch() {
    let clock = Rc::new(Counter(Cell::new(9_000_000_000)));
    let mut world = World::new();
    world.set_clock(clock.clone());
    world
        .set_filesystem(Rc::new(Namespace::temporary(1024 * 1024, 128, 0, None)))
        .unwrap();
    let root = world.filesystem().unwrap();
    assert_eq!(read(root.as_ref(), "proc/uptime"), b"0.00 0.00\n");
    clock.0.set(9_125_000_000);
    assert_eq!(read(root.as_ref(), "proc/uptime"), b"0.12 0.12\n");
    let wasm = wat::parse_str(
        r#"(module (memory (export "memory") 1) (func (export "_start") (loop $loop (br $loop))))"#,
    )
    .unwrap();
    world.register("busy", &wasm).unwrap();
    let pid = world.spawn(vec!["busy".into()], Vec::new()).unwrap();
    clock.0.set(9_325_000_000);
    world.step();
    assert_eq!(read(root.as_ref(), "proc/uptime"), b"0.32 0.12\n");
    world.cancel(pid).unwrap();
    clock.0.set(9_425_000_000);
    assert_eq!(read(root.as_ref(), "proc/uptime"), b"0.42 0.22\n");
}

#[test]
fn process_records_follow_spawn_exit_and_reap_without_retaining_descriptors() {
    let mut world = World::new();
    world
        .set_filesystem(Rc::new(Namespace::temporary(1024 * 1024, 128, 0, None)))
        .unwrap();
    let wasm = wat::parse_str(
        r#"(module
        (import "wasi_snapshot_preview1" "sched_yield" (func $yield (result i32)))
        (memory (export "memory") 1)
        (func (export "_start") (drop (call $yield))))"#,
    )
    .unwrap();
    world.register("probe", &wasm).unwrap();
    let pid = world
        .spawn_with_environment(
            vec!["probe".into(), "alpha β".into()],
            vec!["PATH=/bin".into(), "KEY=a b".into()],
            Vec::new(),
        )
        .unwrap();
    let root = world.filesystem().unwrap();
    let base = format!("proc/{pid}");
    assert_eq!(
        read(root.as_ref(), &format!("{base}/cmdline")),
        b"probe\0alpha \xce\xb2\0"
    );
    assert_eq!(
        read(root.as_ref(), &format!("{base}/environ")),
        b"PATH=/bin\0KEY=a b\0"
    );
    let status = String::from_utf8(read(root.as_ref(), &format!("{base}/status"))).unwrap();
    assert!(status.contains(&format!("Pid:\t{pid}\nPPid:\t0\n")));
    assert!(status.contains("State:\tR (running)\n"));
    let mut file = root
        .open(
            &format!("{base}/status"),
            OpenOptions {
                read: false,
                write: false,
                create: false,
                exclusive: false,
                truncate: false,
                append: false,
            },
        )
        .unwrap();
    assert_eq!(file.read(&mut [0; 1]), Err(FileError::Access));
    assert_eq!(file.sync(false), Err(FileError::Invalid));
    file.flush().unwrap();
    assert_eq!(root.read_link(&format!("{base}/cwd")).unwrap(), "/");
    assert!(root.metadata(&format!("{base}/fd/1")).is_ok());
    assert_eq!(root.metadata("proc/self").err(), Some(FileError::NotFound));
    assert_eq!(world.step(), Some(pid));
    assert_eq!(world.status(pid).unwrap(), &Status::Ready);
    assert_eq!(world.step(), Some(pid));
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
    let status = String::from_utf8(read(root.as_ref(), &format!("{base}/status"))).unwrap();
    assert!(status.contains("State:\tZ (zombie)\n"));
    assert!(read(root.as_ref(), &format!("{base}/cmdline")).is_empty());
    assert_eq!(
        root.metadata(&format!("{base}/fd/1")).err(),
        Some(FileError::NotFound)
    );
    world.reap(pid).unwrap();
    assert_eq!(root.metadata(&base).err(), Some(FileError::NotFound));
}
