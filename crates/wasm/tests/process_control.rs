use efi_agent_wasm::filesystem::{
    DirectoryEntries, File, FileError, FileSystem, Metadata, OpenOptions,
};
use efi_agent_wasm::{Status, World};
use std::rc::Rc;

struct Unused;
impl FileSystem for Unused {
    fn open(&self, _: &str, _: OpenOptions) -> Result<Box<dyn File>, FileError> {
        panic!("Virtual endpoint must not open host files")
    }
    fn metadata(&self, _: &str) -> Result<Metadata, FileError> {
        panic!("Virtual endpoint must not inspect host files")
    }
    fn read_directory(&self, _: &str) -> Result<DirectoryEntries, FileError> {
        panic!("Virtual endpoint must not enumerate files")
    }
}

fn run(body: &str) {
    let module = wat::parse_str(format!(r#"(module
      (import "wasi_snapshot_preview1" "path_open" (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_read" (func $read (param i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (data (i32.const 64) ".efi-process")
      (func $equal (param $a i32) (param $b i32)
        (if (i32.ne (local.get $a) (local.get $b)) (then unreachable)))
      (func $send (param $size i32) (result i32)
        (i32.store (i32.const 128) (i32.const 256))
        (i32.store (i32.const 132) (local.get $size))
        (call $write (i32.load (i32.const 32)) (i32.const 128) (i32.const 1) (i32.const 48)))
      (func $response (result i32)
        (i32.store (i32.const 128) (i32.const 512))
        (i32.store (i32.const 132) (i32.const 8))
        (call $equal (call $read (i32.load (i32.const 32)) (i32.const 128) (i32.const 1) (i32.const 48)) (i32.const 0))
        (call $equal (i32.load (i32.const 48)) (i32.const 8))
        (i32.load (i32.const 512)))
      (func (export "_start")
        (call $equal (call $open (i32.const 3) (i32.const 0) (i32.const 64) (i32.const 12) (i32.const 0) (i64.const 66) (i64.const 0) (i32.const 0) (i32.const 32)) (i32.const 0))
        {body}))"#)).unwrap();
    let mut world = World::new();
    world.set_filesystem(Rc::new(Unused));
    world.register("control", &module).unwrap();
    let pid = world.spawn(vec!["control".into()], vec![]).unwrap();
    for _ in 0..1000 {
        if !world.status(pid).unwrap().is_live() {
            break;
        }
        world.step();
    }
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
}

#[test]
fn malformed_packets_preserve_pid_and_endpoint_and_responses_are_bounded() {
    run(r#"
      (call $equal (call $send (i32.const 0)) (i32.const 0))
      (call $equal (call $response) (i32.const -22))
      (i32.store (i32.const 256) (i32.const 6))
      (call $equal (call $send (i32.const 4)) (i32.const 0))
      (call $equal (call $send (i32.const 4)) (i32.const 6))
      (call $equal (call $response) (i32.const 1))
      (i32.store (i32.const 256) (i32.const 3))
      (i32.store (i32.const 260) (i32.const 1))
      (i32.store (i32.const 264) (i32.load (i32.const 32)))
      (call $equal (call $send (i32.const 12)) (i32.const 0))
      (call $equal (call $response) (i32.const -22))
      (i32.store (i32.const 256) (i32.const 6))
      (call $equal (call $send (i32.const 4)) (i32.const 0))
      (call $equal (call $response) (i32.const 1))
    "#);
}

#[test]
fn invalid_spawn_frames_fail_before_allocating_a_child_or_descriptors() {
    run(r#"
      (i32.store (i32.const 256) (i32.const 9))
      (i32.store (i32.const 260) (i32.const -1))
      (call $equal (call $send (i32.const 16)) (i32.const 0))
      (call $equal (call $response) (i32.const -7))
      (i32.store (i32.const 260) (i32.const 0))
      (i32.store (i32.const 268) (i32.const 1))
      (i32.store (i32.const 272) (i32.const 3))
      (i32.store (i32.const 276) (i32.const 1))
      (call $equal (call $send (i32.const 24)) (i32.const 0))
      (call $equal (call $response) (i32.const -9))
      (i32.store (i32.const 256) (i32.const 1))
      (call $equal (call $send (i32.const 4)) (i32.const 0))
      (call $equal (call $response) (i32.const 5))
      (call $equal (i32.load (i32.const 516)) (i32.const 6))
    "#);
}
