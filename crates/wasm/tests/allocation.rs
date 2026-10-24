use efi_agent_wasm::{
    Status, World,
    filesystem::{DirectoryEntries, EntryType, File, FileError, FileSystem, Metadata, OpenOptions},
};
use std::rc::Rc;

struct AllocationFiles;

impl FileSystem for AllocationFiles {
    fn open(&self, _: &str, _: OpenOptions) -> Result<Box<dyn File>, FileError> {
        panic!("Allocation queries must not open provider files")
    }

    fn read_directory(&self, _: &str) -> Result<DirectoryEntries, FileError> {
        panic!("Allocation queries must not enumerate directories")
    }

    fn symlink_metadata(&self, path: &str) -> Result<Metadata, FileError> {
        self.metadata(path)
    }

    fn metadata(&self, path: &str) -> Result<Metadata, FileError> {
        let mut info = Metadata::new(EntryType::File, 193);
        info.allocated = match path {
            "" => {
                info.kind = EntryType::Directory;
                Some(0)
            }
            "big" => Some((1 << 40) + 513),
            "zero" => Some(0),
            "none" => None,
            "lost" | "bin" => return Err(FileError::NotFound),
            other => panic!("Unexpected provider lookup: {other}"),
        };
        Ok(info)
    }
}

#[test]
fn private_allocation_preserves_u64_and_checks_flags_descriptors_rights_and_support() {
    let module = wat::parse_str(r#"(module
      (import "wasi_snapshot_preview1" "path_open" (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_read" (func $read (param i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_close" (func $close (param i32) (result i32)))
      (memory (export "memory") 1)
      (global $channel (mut i32) (i32.const 0))
      (data (i32.const 64) ".efi-process")
      (data (i32.const 256) "zero")
      (data (i32.const 264) "big")
      (data (i32.const 272) "none")
      (data (i32.const 280) "lost")
      (data (i32.const 288) ".")
      (func $equal (param $a i32) (param $b i32)
        (if (i32.ne (local.get $a) (local.get $b)) (then unreachable)))
      (func $query (param $fd i32) (param $flags i32) (param $name i32) (param $length i32) (param $error i32) (result i32)
        (i32.store (i32.const 128) (i32.const 24))
        (i32.store (i32.const 132) (local.get $fd))
        (i32.store (i32.const 136) (local.get $flags))
        (memory.copy (i32.const 140) (local.get $name) (local.get $length))
        (i32.store (i32.const 16) (i32.const 128))
        (i32.store (i32.const 20) (i32.add (i32.const 12) (local.get $length)))
        (call $equal (call $write (global.get $channel) (i32.const 16) (i32.const 1) (i32.const 24)) (i32.const 0))
        (i32.store (i32.const 16) (i32.const 192))
        (i32.store (i32.const 20) (i32.const 8))
        (call $equal (call $read (global.get $channel) (i32.const 16) (i32.const 1) (i32.const 24)) (i32.const 0))
        (call $equal (i32.load (i32.const 24)) (i32.const 8))
        (call $equal (i32.load (i32.const 192)) (local.get $error))
        (i32.load (i32.const 196)))
      (func $allocation (param $name i32) (param $length i32) (param $expected i64) (local $reply i32)
        (local.set $reply (call $query (i32.const 3) (i32.const 1) (local.get $name) (local.get $length) (i32.const 0)))
        (i32.store (i32.const 16) (i32.const 208))
        (i32.store (i32.const 20) (i32.const 8))
        (call $equal (call $read (local.get $reply) (i32.const 16) (i32.const 1) (i32.const 24)) (i32.const 0))
        (if (i64.ne (i64.load (i32.const 208)) (local.get $expected)) (then unreachable))
        (call $equal (call $close (local.get $reply)) (i32.const 0)))
      (func (export "_start") (local $restricted i32) (local $count i32)
        (call $equal (call $open (i32.const 3) (i32.const 0) (i32.const 64) (i32.const 12) (i32.const 0) (i64.const 66) (i64.const 0) (i32.const 0) (i32.const 0)) (i32.const 0))
        (global.set $channel (i32.load (i32.const 0)))
        (drop (call $query (i32.const 127) (i32.const 1) (i32.const 0) (i32.const 0) (i32.const 8)))
        (drop (call $query (i32.const 3) (i32.const 2) (i32.const 0) (i32.const 0) (i32.const 28)))
        (drop (call $query (i32.const 1) (i32.const 1) (i32.const 256) (i32.const 4) (i32.const 54)))
        (call $allocation (i32.const 256) (i32.const 4) (i64.const 0))
        (call $allocation (i32.const 264) (i32.const 3) (i64.const 1099511628289))
        ;; Unknown allocation must not be reported as known zero.
        (drop (call $query (i32.const 3) (i32.const 1) (i32.const 272) (i32.const 4) (i32.const 58)))
        (drop (call $query (i32.const 3) (i32.const 1) (i32.const 280) (i32.const 4) (i32.const 44)))
        ;; "big" includes a NUL in the fixed four-byte name field: reject it.
        (drop (call $query (i32.const 3) (i32.const 1) (i32.const 264) (i32.const 4) (i32.const -22)))
        (call $equal (call $open (i32.const 3) (i32.const 0) (i32.const 288) (i32.const 1) (i32.const 2) (i64.const 0) (i64.const 0) (i32.const 0) (i32.const 0)) (i32.const 0))
        (local.set $restricted (i32.load (i32.const 0)))
        (drop (call $query (local.get $restricted) (i32.const 1) (i32.const 256) (i32.const 4) (i32.const 76)))
        (drop (call $query (local.get $restricted) (i32.const 1) (i32.const 0) (i32.const 0) (i32.const 76)))
        ;; Keep replies open to cross the descriptor quota on the next query.
        (loop $fill
          (drop (call $query (i32.const 3) (i32.const 1) (i32.const 256) (i32.const 4) (i32.const 0)))
          (local.set $count (i32.add (local.get $count) (i32.const 1)))
          (br_if $fill (i32.lt_u (local.get $count) (i32.const 122))))
        (drop (call $query (i32.const 3) (i32.const 1) (i32.const 256) (i32.const 4) (i32.const 33)))
        (drop (call $query (i32.const 3) (i32.const 2) (i32.const 0) (i32.const 0) (i32.const 28)))
      ))"#).unwrap();
    let mut world = World::new();
    world.set_filesystem(Rc::new(AllocationFiles));
    world.register("allocation", &module).unwrap();
    let pid = world.spawn(vec!["allocation".into()], vec![]).unwrap();
    for _ in 0..1000 {
        if !world.status(pid).unwrap().is_live() {
            break;
        }
        world.step();
    }
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
}
