use efi_agent_wasm::{Status, World};

fn run(parent: &str, child: &str) -> (Vec<u8>, Vec<u8>) {
    let mut world = World::new();
    for (name, source) in [("parent", parent), ("child", child)] {
        world
            .register(name, &wat::parse_str(source).unwrap())
            .unwrap();
    }
    let pid = world.spawn(vec!["parent".into()], vec![]).unwrap();
    for _ in 0..10000 {
        if !world.status(pid).unwrap().is_live() {
            assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
            return world.take_output(pid).unwrap();
        }
        assert!(world.step().is_some(), "Mapped descriptor fixture blocked");
    }
    panic!("Mapped descriptor fixture did not finish");
}

const IMPORTS: &str = r#"
  (import "efi_process" "spawn_mapped_fds" (func $spawn (param i32 i32 i32 i32 i32 i32) (result i32)))
  (import "efi_process" "waitpid" (func $wait (param i32 i32) (result i32)))
  (import "efi_process" "input_snapshot" (func $snapshot (param i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "fd_close" (func $close (param i32) (result i32)))
  (memory (export "memory") 1)
  (data (i32.const 64) "child\00")
"#;

#[test]
fn mappings_swap_outputs_and_share_binary_input_after_parent_close() {
    let parent = format!(
        r#"(module {IMPORTS}
      (data (i32.const 80) "ab\00\ffYZ")
      (func (export "_start") (local $fd i32) (local $pid i32)
        (local.set $fd (call $snapshot (i32.const 80) (i32.const 6)))
        (i32.store (i32.const 128) (i32.const 1))
        (i32.store (i32.const 132) (i32.const 2))
        (i32.store (i32.const 136) (i32.const 2))
        (i32.store (i32.const 140) (i32.const 1))
        (i32.store (i32.const 144) (i32.const 4))
        (i32.store (i32.const 148) (local.get $fd))
        (i32.store (i32.const 152) (i32.const 5))
        (i32.store (i32.const 156) (local.get $fd))
        (local.set $pid (call $spawn (i32.const 64) (i32.const 6)
          (i32.const 0) (i32.const 0) (i32.const 128) (i32.const 4)))
        (if (i32.ne (local.get $pid) (i32.const 2)) (then unreachable))
        (memory.fill (i32.const 80) (i32.const 88) (i32.const 6))
        (if (call $close (local.get $fd)) (then unreachable))
        (if (i32.ne (call $wait (local.get $pid) (i32.const 0)) (local.get $pid)) (then unreachable))
        (if (i32.load (i32.const 0)) (then unreachable))))"#
    );
    let child = r#"(module
      (import "wasi_snapshot_preview1" "fd_read" (func $read (param i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_close" (func $close (param i32) (result i32)))
      (memory (export "memory") 1)
      (data (i32.const 64) "x")
      (func (export "_start")
        (i32.store (i32.const 0) (i32.const 32))
        (i32.store (i32.const 4) (i32.const 2))
        (if (call $read (i32.const 4) (i32.const 0) (i32.const 1) (i32.const 8)) (then unreachable))
        (if (i32.ne (i32.load (i32.const 8)) (i32.const 2)) (then unreachable))
        (if (call $close (i32.const 4)) (then unreachable))
        (i32.store (i32.const 0) (i32.const 34))
        (i32.store (i32.const 4) (i32.const 4))
        (if (call $read (i32.const 5) (i32.const 0) (i32.const 1) (i32.const 8)) (then unreachable))
        (if (i32.ne (i32.load (i32.const 8)) (i32.const 4)) (then unreachable))
        (i32.store (i32.const 0) (i32.const 32))
        (i32.store (i32.const 4) (i32.const 6))
        (if (call $write (i32.const 1) (i32.const 0) (i32.const 1) (i32.const 8)) (then unreachable))
        (i32.store (i32.const 0) (i32.const 64))
        (i32.store (i32.const 4) (i32.const 1))
        (if (call $write (i32.const 2) (i32.const 0) (i32.const 1) (i32.const 8)) (then unreachable))
        (if (i32.ne (call $read (i32.const 0) (i32.const 0) (i32.const 1) (i32.const 8)) (i32.const 8)) (then unreachable))))"#;
    assert_eq!(run(&parent, child), (b"x".to_vec(), b"ab\0\xffYZ".to_vec()));
}

#[test]
fn invalid_maps_do_not_spawn_and_empty_map_does_not_inherit_stdio() {
    let parent = format!(
        r#"(module {IMPORTS}
      (func (export "_start") (local $pid i32)
        (if (i32.ne (call $spawn (i32.const 64) (i32.const 6) (i32.const 0) (i32.const 0)
          (i32.const 0) (i32.const 128)) (i32.const -7)) (then unreachable))
        (if (i32.ne (call $spawn (i32.const 64) (i32.const 6) (i32.const 0) (i32.const 0)
          (i32.const 65532) (i32.const 1)) (i32.const -14)) (then unreachable))
        (i32.store (i32.const 128) (i32.const 3))
        (i32.store (i32.const 132) (i32.const 1))
        (if (i32.ne (call $spawn (i32.const 64) (i32.const 6) (i32.const 0) (i32.const 0)
          (i32.const 128) (i32.const 1)) (i32.const -22)) (then unreachable))
        (i32.store (i32.const 128) (i32.const 128))
        (if (i32.ne (call $spawn (i32.const 64) (i32.const 6) (i32.const 0) (i32.const 0)
          (i32.const 128) (i32.const 1)) (i32.const -22)) (then unreachable))
        (i32.store (i32.const 128) (i32.const 4))
        (i32.store (i32.const 132) (i32.const 127))
        (if (i32.ne (call $spawn (i32.const 64) (i32.const 6) (i32.const 0) (i32.const 0)
          (i32.const 128) (i32.const 1)) (i32.const -9)) (then unreachable))
        (i32.store (i32.const 132) (i32.const 1))
        (i32.store (i32.const 136) (i32.const 4))
        (i32.store (i32.const 140) (i32.const 2))
        (if (i32.ne (call $spawn (i32.const 64) (i32.const 6) (i32.const 0) (i32.const 0)
          (i32.const 128) (i32.const 2)) (i32.const -22)) (then unreachable))
        (local.set $pid (call $spawn (i32.const 64) (i32.const 6) (i32.const 0) (i32.const 0)
          (i32.const 65536) (i32.const 0)))
        (if (i32.ne (local.get $pid) (i32.const 2)) (then unreachable))
        (if (i32.ne (call $wait (local.get $pid) (i32.const 0)) (local.get $pid)) (then unreachable))
        (if (i32.load (i32.const 0)) (then unreachable))))"#
    );
    let child = r#"(module
      (import "wasi_snapshot_preview1" "fd_fdstat_get" (func $stat (param i32 i32) (result i32)))
      (memory (export "memory") 1)
      (func (export "_start")
        (if (i32.ne (call $stat (i32.const 0) (i32.const 0)) (i32.const 8)) (then unreachable))
        (if (i32.ne (call $stat (i32.const 1) (i32.const 0)) (i32.const 8)) (then unreachable))
        (if (i32.ne (call $stat (i32.const 2) (i32.const 0)) (i32.const 8)) (then unreachable))))"#;
    assert_eq!(run(&parent, child), (vec![], vec![]));
}
