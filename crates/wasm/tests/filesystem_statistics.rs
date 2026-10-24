use efi_agent_wasm::{
    Status, World,
    filesystem::{
        Capacity, DirectoryEntries, EntryType, File, FileError, FileSystem, FileSystemStatistics,
        Metadata, Mount, OpenOptions,
    },
};
use std::{cell::RefCell, rc::Rc};

#[derive(Default)]
struct Statistics(RefCell<Vec<String>>);
impl FileSystem for Statistics {
    fn open(&self, _: &str, _: OpenOptions) -> Result<Box<dyn File>, FileError> {
        panic!("statistics must not open provider files")
    }
    fn metadata(&self, path: &str) -> Result<Metadata, FileError> {
        match path {
            "" | "dir" => {
                let mut metadata = Metadata::new(EntryType::Directory, 0);
                metadata.allocated = Some(0);
                Ok(metadata)
            }
            _ => Err(FileError::NotFound),
        }
    }
    fn read_directory(&self, _: &str) -> Result<DirectoryEntries, FileError> {
        panic!("statistics must not enumerate directories")
    }
    fn filesystem_statistics(&self, path: &str) -> Result<FileSystemStatistics, FileError> {
        self.0.borrow_mut().push(path.into());
        Ok(FileSystemStatistics {
            capacity: Capacity {
                total: (1u64 << 52) + 8192,
                free: (1u64 << 41) + 4096,
                available: (1u64 << 40) + 4096,
                block_size: 4096,
            },
            files: 8193,
            files_free: 731,
            id: 193,
            kind: 0x45464941,
            name_max: 1024,
            read_only: false,
        })
    }
    fn mounts(&self) -> Result<Vec<Mount>, FileError> {
        Ok(vec![Mount {
            source: "disk name\\test".into(),
            path: "/".into(),
            kind: "efi-agent".into(),
            read_only: false,
        }])
    }
}

#[test]
fn statistics_reply_preserves_u64_reserved_space_and_checks_rights_before_effects() {
    let module = wat::parse_str(r#"(module
      (import "wasi_snapshot_preview1" "path_open" (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_read" (func $read (param i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_close" (func $close (param i32) (result i32)))
      (memory (export "memory") 1)
      (global $channel (mut i32) (i32.const 0))
      (data (i32.const 64) ".efi-process")
      (data (i32.const 256) "dir")
      (data (i32.const 264) "../bad")
      (data (i32.const 280) ".")
      (func $equal (param $a i32) (param $b i32)
        (if (i32.ne (local.get $a) (local.get $b)) (then unreachable)))
      (func $query (param $fd i32) (param $mode i32) (param $path i32) (param $length i32) (param $error i32) (result i32)
        (i32.store (i32.const 128) (i32.const 26))
        (i32.store (i32.const 132) (local.get $fd))
        (i32.store (i32.const 136) (local.get $mode))
        (memory.copy (i32.const 140) (local.get $path) (local.get $length))
        (i32.store (i32.const 16) (i32.const 128))
        (i32.store (i32.const 20) (i32.add (i32.const 12) (local.get $length)))
        (call $equal (call $write (global.get $channel) (i32.const 16) (i32.const 1) (i32.const 24)) (i32.const 0))
        (i32.store (i32.const 16) (i32.const 192))
        (i32.store (i32.const 20) (i32.const 8))
        (call $equal (call $read (global.get $channel) (i32.const 16) (i32.const 1) (i32.const 24)) (i32.const 0))
        (call $equal (i32.load (i32.const 192)) (local.get $error))
        (i32.load (i32.const 196)))
      (func (export "_start") (local $reply i32) (local $restricted i32)
        (call $equal (call $open (i32.const 3) (i32.const 0) (i32.const 64) (i32.const 12) (i32.const 0) (i64.const 66) (i64.const 0) (i32.const 0) (i32.const 0)) (i32.const 0))
        (global.set $channel (i32.load (i32.const 0)))
        (local.set $reply (call $query (i32.const 3) (i32.const 1) (i32.const 256) (i32.const 3) (i32.const 0)))
        (i32.store (i32.const 16) (i32.const 512))
        (i32.store (i32.const 20) (i32.const 96))
        (call $equal (call $read (local.get $reply) (i32.const 16) (i32.const 1) (i32.const 24)) (i32.const 0))
        (call $equal (i32.load (i32.const 24)) (i32.const 96))
        (if (i64.ne (i64.load (i32.const 528)) (i64.const 1099511627778)) (then unreachable))
        (if (i64.ne (i64.load (i32.const 536)) (i64.const 536870913)) (then unreachable))
        (if (i64.ne (i64.load (i32.const 544)) (i64.const 268435457)) (then unreachable))
        (call $equal (i32.load (i32.const 552)) (i32.const 8193))
        (call $equal (i32.load (i32.const 560)) (i32.const 731))
        (call $equal (i32.load (i32.const 576)) (i32.const 193))
        (call $equal (i32.load (i32.const 592)) (i32.const 1024))
        (call $equal (call $close (local.get $reply)) (i32.const 0))
        (drop (call $query (i32.const 127) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 8)))
        (drop (call $query (i32.const 1) (i32.const 1) (i32.const 256) (i32.const 3) (i32.const 54)))
        (drop (call $query (i32.const 1) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 58)))
        (drop (call $query (i32.const 3) (i32.const 2) (i32.const 0) (i32.const 0) (i32.const 28)))
        (drop (call $query (i32.const 3) (i32.const 0) (i32.const 256) (i32.const 3) (i32.const 28)))
        (drop (call $query (i32.const 3) (i32.const 1) (i32.const 264) (i32.const 6) (i32.const 76)))
        (drop (call $query (i32.const 3) (i32.const 1) (i32.const 256) (i32.const 4) (i32.const 76)))
        (call $equal (call $open (i32.const 3) (i32.const 0) (i32.const 280) (i32.const 1) (i32.const 2) (i64.const 0) (i64.const 0) (i32.const 0) (i32.const 0)) (i32.const 0))
        (local.set $restricted (i32.load (i32.const 0)))
        (drop (call $query (local.get $restricted) (i32.const 1) (i32.const 256) (i32.const 3) (i32.const 76)))
        (drop (call $query (local.get $restricted) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 76)))
        ;; All reply snapshots retain private close-on-exec descriptors and quota.
        (call $equal (call $close (local.get $restricted)) (i32.const 0))
        (block $done
          (loop $fill
            (local.set $reply (call $query (i32.const 3) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 0)))
            (br_if $done (i32.eq (local.get $reply) (i32.const 127)))
            (br $fill)))
        (drop (call $query (i32.const 3) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 33)))
      ))"#).unwrap();
    let provider = Rc::new(Statistics::default());
    let mut world = World::new();
    world.set_filesystem(provider.clone());
    world.register("statistics", &module).unwrap();
    let pid = world.spawn(vec!["statistics".into()], vec![]).unwrap();
    for _ in 0..1000 {
        if !world.status(pid).unwrap().is_live() {
            break;
        }
        world.step();
    }
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
    assert!(provider.0.borrow().iter().all(|path| path.is_empty()));
    assert_eq!(provider.0.borrow().len(), 125);
}

#[test]
fn mount_snapshot_escapes_fields_and_uses_real_virtual_mount_inventory() {
    let module = wat::parse_str(r#"(module
      (import "wasi_snapshot_preview1" "path_open" (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_read" (func $read (param i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (data (i32.const 64) ".efi-process")
      (data (i32.const 128) "\1b\00\00\00")
      (func (export "_start") (local $channel i32) (local $reply i32)
        (if (call $open (i32.const 3) (i32.const 0) (i32.const 64) (i32.const 12) (i32.const 0) (i64.const 66) (i64.const 0) (i32.const 0) (i32.const 0)) (then unreachable))
        (local.set $channel (i32.load (i32.const 0)))
        (i32.store (i32.const 16) (i32.const 128))
        (i32.store (i32.const 20) (i32.const 4))
        (if (call $write (local.get $channel) (i32.const 16) (i32.const 1) (i32.const 24)) (then unreachable))
        (i32.store (i32.const 16) (i32.const 192))
        (i32.store (i32.const 20) (i32.const 8))
        (if (call $read (local.get $channel) (i32.const 16) (i32.const 1) (i32.const 24)) (then unreachable))
        (if (i32.load (i32.const 192)) (then unreachable))
        (local.set $reply (i32.load (i32.const 196)))
        (i32.store (i32.const 16) (i32.const 512))
        (i32.store (i32.const 20) (i32.const 512))
        (if (call $read (local.get $reply) (i32.const 16) (i32.const 1) (i32.const 24)) (then unreachable))
        (i32.store (i32.const 20) (i32.load (i32.const 24)))
        (if (call $write (i32.const 1) (i32.const 16) (i32.const 1) (i32.const 24)) (then unreachable))))"#).unwrap();
    let mut world = World::new();
    world.set_filesystem(Rc::new(Statistics::default()));
    world.register("mounts", &module).unwrap();
    let pid = world.spawn(vec!["mounts".into()], vec![]).unwrap();
    for _ in 0..100 {
        if !world.status(pid).unwrap().is_live() {
            break;
        }
        world.step();
    }
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
    let (stdout, stderr) = world.take_output(pid).unwrap();
    assert!(stderr.is_empty());
    assert_eq!(stdout, b"disk\\040name\\134test / efi-agent rw 0 0\ndevfs /dev devfs ro 0 0\ntmpfs /tmp tmpfs rw 0 0\nproc /proc proc ro 0 0\n");
}

#[test]
fn statistics_reject_inconsistent_provider_values() {
    let provider = Statistics::default();
    let mut stats = provider.filesystem_statistics("").unwrap();
    let good = stats;
    for case in 0..4 {
        stats = good;
        match case {
            0 => stats.capacity.block_size = 0,
            1 => stats.capacity.available = stats.capacity.free + 1,
            2 => stats.capacity.free = stats.capacity.total + 1,
            _ => stats.files_free = stats.files + 1,
        }
        assert_eq!(stats.encode(), Err(FileError::Invalid));
    }
}

#[test]
#[ignore = "requires a built BusyBox image in EFI_AGENT_ASH"]
fn upstream_statfs_preserves_large_counts_and_reserved_capacity() {
    let path = std::env::var_os("EFI_AGENT_ASH").expect("set EFI_AGENT_ASH to the built ash.wasm");
    let module = std::fs::read(path).unwrap();
    let mut world = World::new();
    world.set_filesystem(Rc::new(Statistics::default()));
    world.register("ash", &module).unwrap();
    let pid = world
        .spawn(
            vec![
                "stat".into(),
                "-f".into(),
                "-c".into(),
                "%i|%t|%T|%s|%S|%b|%f|%a|%c|%d|%l".into(),
                "/".into(),
            ],
            vec![],
        )
        .unwrap();
    for _ in 0..10000 {
        if !world.status(pid).unwrap().is_live() {
            break;
        }
        world.step();
    }
    let (stdout, stderr) = world.take_output(pid).unwrap();
    assert_eq!(
        world.status(pid).unwrap(),
        &Status::Exited(0),
        "{}",
        String::from_utf8_lossy(&stderr)
    );
    assert!(stderr.is_empty(), "{}", String::from_utf8_lossy(&stderr));
    // Counts are computed independently from the provider's byte capacities.
    assert_eq!(
        stdout,
        b"c1|45464941|efi-agent|4096|4096|1099511627778|536870913|268435457|8193|731|1024\n"
    );
}
