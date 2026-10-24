use efi_agent_wasm::{
    Status, World,
    filesystem::{EntryType, FileError, FileSystem, OpenOptions, namespace::Namespace},
    random::{Random, RandomError},
};
use std::{cell::Cell, rc::Rc};

struct Sequence {
    next: Cell<u8>,
    calls: Cell<usize>,
    failure: Cell<Option<RandomError>>,
}

impl Random for Sequence {
    fn fill(&self, buffer: &mut [u8]) -> Result<(), RandomError> {
        self.calls.set(self.calls.get() + 1);
        for byte in buffer {
            *byte = self.next.get();
            self.next.set(self.next.get().wrapping_add(1));
        }
        self.failure.get().map_or(Ok(()), Err)
    }
}

fn namespace() -> Rc<Namespace> {
    Rc::new(Namespace::temporary(1024 * 1024, 128, 0, None))
}

fn options() -> OpenOptions {
    OpenOptions {
        read: true,
        write: true,
        create: false,
        exclusive: false,
        truncate: false,
        append: false,
    }
}

#[test]
fn null_zero_and_random_metadata_are_character_devices_with_distinct_identity() {
    let base = namespace();
    let mut world = World::new();
    world.set_filesystem(base.clone()).unwrap();
    let root = world.filesystem().unwrap();
    let mut inodes = Vec::new();
    for name in ["null", "zero", "random", "urandom"] {
        let path = format!("dev/{name}");
        let info = root.metadata(&path).unwrap();
        assert_eq!(
            (info.kind, info.mode, info.device, info.size, info.allocated),
            (EntryType::CharacterDevice, 0o666, 2, 0, Some(0))
        );
        inodes.push(info.inode);
        let mut file = root.open(&path, options()).unwrap();
        assert_eq!(file.metadata().unwrap().inode, info.inode);
        assert_eq!(file.write(&vec![19; 65537]).unwrap(), 65537);
        for whence in 0..=2 {
            assert_eq!(file.seek(-73, whence).unwrap(), 0);
        }
        assert_eq!(file.seek(0, 3), Err(FileError::Invalid));
        assert_eq!(file.resize(0), Err(FileError::Invalid));
        assert_eq!(file.sync(false), Err(FileError::Invalid));
        assert_eq!(file.sync(true), Err(FileError::Invalid));
        file.flush().unwrap();
        let mut exclusive = options();
        exclusive.create = true;
        exclusive.exclusive = true;
        assert_eq!(root.open(&path, exclusive).err(), Some(FileError::Exists));
    }
    inodes.sort();
    inodes.dedup();
    assert_eq!(inodes.len(), 4);
    let listing = root.read_directory("dev").unwrap();
    for (name, kind) in listing.iter() {
        assert_eq!(
            kind,
            if matches!(name, "." | "..") {
                EntryType::Directory
            } else {
                EntryType::CharacterDevice
            }
        );
    }
    let statfs = root.filesystem_statistics("dev/null").unwrap();
    assert_eq!(
        (statfs.files, statfs.read_only, statfs.capacity.total),
        (5, true, 0)
    );
    assert_eq!(base.metadata("dev").err(), Some(FileError::NotFound));
    let mut null = root.open("dev/null", options()).unwrap();
    let mut zero = root.open("dev/zero", options()).unwrap();
    let mut bytes = vec![73; 65537];
    assert_eq!(null.read(&mut bytes).unwrap(), 0);
    assert!(bytes.iter().all(|byte| *byte == 73));
    assert_eq!(zero.read(&mut bytes).unwrap(), bytes.len());
    assert!(bytes.iter().all(|byte| *byte == 0));
    let mut read_only = options();
    read_only.write = false;
    assert_eq!(
        root.open("dev/zero", read_only).unwrap().write(b"x"),
        Err(FileError::Access)
    );
    let mut write_only = options();
    write_only.read = false;
    assert_eq!(
        root.open("dev/null", write_only).unwrap().read(&mut bytes),
        Err(FileError::Access)
    );
}

#[test]
fn devices_share_continuous_entropy_and_configuration_in_either_install_order() {
    for random_first in [false, true] {
        let source = Rc::new(Sequence {
            next: Cell::new(73),
            calls: Cell::new(0),
            failure: Cell::new(None),
        });
        let mut world = World::new();
        if random_first {
            world.set_random(source.clone());
        }
        world.set_filesystem(namespace()).unwrap();
        let root = world.filesystem().unwrap();
        let mut first = root.open("dev/random", options()).unwrap();
        if !random_first {
            let mut before = [165; 3];
            assert_eq!(first.read(&mut before), Err(FileError::Unsupported));
            assert_eq!(before, [165; 3]);
            world.set_random(source.clone());
        }
        let dev = root.directory_view("dev").unwrap().unwrap();
        let mut second = dev.open("urandom", options()).unwrap();
        let mut bytes = [0; 5];
        assert_eq!(first.read(&mut bytes).unwrap(), 5);
        assert_eq!(bytes, [73, 74, 75, 76, 77]);
        assert_eq!(second.read(&mut bytes).unwrap(), 5);
        assert_eq!(bytes, [78, 79, 80, 81, 82]);
        assert_eq!(source.calls.get(), 2);
        first.write(b"user bytes do not seed entropy").unwrap();
        assert_eq!(source.calls.get(), 2);
        for error in [RandomError::Unavailable, RandomError::Io] {
            source.failure.set(Some(error));
            let mut unchanged = [165; 5];
            assert_eq!(
                first.read(&mut unchanged),
                Err(if error == RandomError::Io {
                    FileError::Io
                } else {
                    FileError::Unsupported
                })
            );
            assert_eq!(unchanged, [165; 5]);
        }
        source.failure.set(None);
        let mut large = vec![165; 65537];
        assert_eq!(first.read(&mut large).unwrap(), 65536);
        assert_eq!(large[65536], 165);
        let before = source.calls.get();
        assert_eq!(first.read(&mut []).unwrap(), 0);
        assert_eq!(source.calls.get(), before);
    }
}

#[test]
fn wasi_device_reads_validate_all_ranges_and_keep_failures_atomic() {
    for failure in [None, Some(RandomError::Unavailable), Some(RandomError::Io)] {
        let source = Rc::new(Sequence {
            next: Cell::new(73),
            calls: Cell::new(0),
            failure: Cell::new(failure),
        });
        let mut world = World::new();
        world.set_random(source.clone());
        world.set_filesystem(namespace()).unwrap();
        let expected_error = match failure {
            None => 0,
            Some(RandomError::Unavailable) => 58,
            Some(RandomError::Io) => 29,
        };
        let expected_data = if failure.is_none() {
            0xaa4b4a49u32
        } else {
            0xaabbccddu32
        };
        let expected_count = if failure.is_none() { 3 } else { 0x12345678 };
        let module = format!(
            r#"(module
          (import "wasi_snapshot_preview1" "path_open" (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
          (import "wasi_snapshot_preview1" "fd_read" (func $read (param i32 i32 i32 i32) (result i32)))
          (import "wasi_snapshot_preview1" "fd_fdstat_get" (func $fdstat (param i32 i32) (result i32)))
          (import "wasi_snapshot_preview1" "fd_fdstat_set_flags" (func $flags (param i32 i32) (result i32)))
          (import "efi_process" "dup2" (func $dup (param i32 i32) (result i32)))
          (import "wasi_snapshot_preview1" "fd_filestat_get" (func $stat (param i32 i32) (result i32)))
          (import "wasi_snapshot_preview1" "fd_sync" (func $sync (param i32) (result i32)))
          (import "wasi_snapshot_preview1" "fd_filestat_set_size" (func $truncate (param i32 i64) (result i32)))
          (import "wasi_snapshot_preview1" "poll_oneoff" (func $poll (param i32 i32 i32 i32) (result i32)))
          (memory (export "memory") 1)
          (data (i32.const 100) "dev/random")
          (func $eq (param $a i32) (param $b i32) (if (i32.ne (local.get $a) (local.get $b)) (then unreachable)))
          (func (export "_start") (local $fd i32)
            (call $eq (call $open (i32.const 3) (i32.const 1) (i32.const 100) (i32.const 10) (i32.const 0) (i64.const 140509311) (i64.const 0) (i32.const 4) (i32.const 0)) (i32.const 0))
            (local.set $fd (i32.load (i32.const 0)))
            (call $eq (call $fdstat (local.get $fd) (i32.const 256)) (i32.const 0))
            (call $eq (i32.load8_u (i32.const 256)) (i32.const 0))
            (call $eq (i32.load16_u (i32.const 258)) (i32.const 4))
            (call $eq (call $flags (local.get $fd) (i32.const 5)) (i32.const 0))
            (call $eq (call $dup (local.get $fd) (i32.const 20)) (i32.const 20))
            (call $eq (call $fdstat (i32.const 20) (i32.const 256)) (i32.const 0))
            (call $eq (i32.load16_u (i32.const 258)) (i32.const 5))
            (call $eq (call $flags (i32.const 20) (i32.const 0)) (i32.const 0))
            (call $eq (call $flags (local.get $fd) (i32.const 8)) (i32.const 58))
            (call $eq (call $fdstat (local.get $fd) (i32.const 256)) (i32.const 0))
            (call $eq (i32.load16_u (i32.const 258)) (i32.const 0))
            (call $eq (call $open (i32.const 3) (i32.const 1) (i32.const 100) (i32.const 10) (i32.const 0) (i64.const 2) (i64.const 0) (i32.const 0) (i32.const 4)) (i32.const 0))
            (call $eq (call $flags (i32.load (i32.const 4)) (i32.const 4)) (i32.const 76))
            (call $eq (call $stat (local.get $fd) (i32.const 288)) (i32.const 0))
            (call $eq (i32.load8_u (i32.const 304)) (i32.const 2))
            (call $eq (call $sync (local.get $fd)) (i32.const 28))
            (call $eq (call $truncate (local.get $fd) (i64.const 0)) (i32.const 28))
            (i32.store8 (i32.const 648) (i32.const 1))
            (i32.store (i32.const 656) (local.get $fd))
            (call $eq (call $poll (i32.const 640) (i32.const 704) (i32.const 1) (i32.const 768)) (i32.const 0))
            (call $eq (i32.load (i32.const 768)) (i32.const 1))
            (call $eq (i32.load16_u (i32.const 712)) (i32.const 0))
            (i32.store (i32.const 16) (i32.const 128))
            (i32.store (i32.const 20) (i32.const 3))
            (i32.store (i32.const 24) (i32.const 65535))
            (i32.store (i32.const 28) (i32.const 2))
            (i32.store (i32.const 64) (i32.const 0x12345678))
            (i32.store (i32.const 128) (i32.const 0xaabbccdd))
            (call $eq (call $read (local.get $fd) (i32.const 16) (i32.const 2) (i32.const 64)) (i32.const 21))
            (call $eq (i32.load (i32.const 64)) (i32.const 0x12345678))
            (call $eq (i32.load (i32.const 128)) (i32.const 0xaabbccdd))
            (call $eq (call $read (local.get $fd) (i32.const 16) (i32.const 1) (i32.const 64)) (i32.const {expected_error}))
            (call $eq (i32.load (i32.const 64)) (i32.const {expected_count}))
            (call $eq (i32.load (i32.const 128)) (i32.const {expected_data}))))"#
        );
        world
            .register("devices", &wat::parse_str(&module).unwrap())
            .unwrap();
        let pid = world.spawn(vec!["devices".into()], vec![]).unwrap();
        for _ in 0..100 {
            if !world.status(pid).unwrap().is_live() {
                break;
            }
            world.step();
        }
        assert_eq!(
            world.status(pid).unwrap(),
            &Status::Exited(0),
            "{failure:?}"
        );
        assert_eq!(source.calls.get(), 1);
    }
}
