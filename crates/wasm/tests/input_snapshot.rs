use efi_agent_wasm::{Status, World};

fn run(source: &str) -> (Status, Vec<u8>, Vec<u8>) {
    let mut world = World::new();
    world
        .register("snapshot", &wat::parse_str(source).unwrap())
        .unwrap();
    let pid = world.spawn(vec!["snapshot".into()], vec![]).unwrap();
    for _ in 0..10000 {
        if !world.status(pid).unwrap().is_live() {
            break;
        }
        assert!(world.step().is_some(), "Snapshot reader blocked");
    }
    let status = world.status(pid).unwrap().clone();
    let (stdout, stderr) = world.take_output(pid).unwrap();
    (status, stdout, stderr)
}

#[test]
fn snapshot_copies_binary_memory_and_duplicates_share_consumption() {
    let result = run(r#"(module
          (import "efi_process" "input_snapshot" (func $input (param i32 i32) (result i32)))
          (import "efi_process" "dup2" (func $dup (param i32 i32) (result i32)))
          (import "wasi_snapshot_preview1" "fd_read" (func $read (param i32 i32 i32 i32) (result i32)))
          (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
          (import "wasi_snapshot_preview1" "fd_close" (func $close (param i32) (result i32)))
          (import "wasi_snapshot_preview1" "fd_seek" (func $seek (param i32 i64 i32 i32) (result i32)))
          (memory (export "memory") 1)
          (data (i32.const 64) "a\00\ffZ")
          (func (export "_start") (local $fd i32)
            (local.set $fd (call $input (i32.const 64) (i32.const 4)))
            (if (i32.lt_s (local.get $fd) (i32.const 0)) (then unreachable))
            (memory.fill (i32.const 64) (i32.const 88) (i32.const 4))
            (if (i32.ne (call $dup (local.get $fd) (i32.const 7)) (i32.const 7)) (then unreachable))
            (i32.store (i32.const 0) (i32.const 32))
            (i32.store (i32.const 4) (i32.const 1))
            (if (call $read (local.get $fd) (i32.const 0) (i32.const 1) (i32.const 8)) (then unreachable))
            (if (i32.ne (i32.load (i32.const 8)) (i32.const 1)) (then unreachable))
            (if (call $close (local.get $fd)) (then unreachable))
            (i32.store (i32.const 0) (i32.const 33))
            (i32.store (i32.const 4) (i32.const 9))
            (if (call $read (i32.const 7) (i32.const 0) (i32.const 1) (i32.const 8)) (then unreachable))
            (if (i32.ne (i32.load (i32.const 8)) (i32.const 3)) (then unreachable))
            (i32.store (i32.const 0) (i32.const 32))
            (i32.store (i32.const 4) (i32.const 4))
            (if (call $write (i32.const 1) (i32.const 0) (i32.const 1) (i32.const 8)) (then unreachable))
            (if (call $read (i32.const 7) (i32.const 0) (i32.const 1) (i32.const 8)) (then unreachable))
            (if (i32.load (i32.const 8)) (then unreachable))
            (if (i32.ne (call $write (i32.const 7) (i32.const 0) (i32.const 1) (i32.const 8)) (i32.const 8)) (then unreachable))
            (if (i32.ne (call $seek (i32.const 7) (i64.const 0) (i32.const 0) (i32.const 16)) (i32.const 70)) (then unreachable))))"#);
    assert_eq!(result, (Status::Exited(0), b"a\0\xffZ".to_vec(), vec![]));
}

#[test]
fn snapshot_bounds_and_descriptor_exhaustion_do_not_consume_slots_on_error() {
    let result = run(r#"(module
          (import "efi_process" "input_snapshot" (func $input (param i32 i32) (result i32)))
          (import "wasi_snapshot_preview1" "fd_close" (func $close (param i32) (result i32)))
          (memory (export "memory") 9)
          (func (export "_start") (local $fd i32)
            (if (i32.ne (call $input (i32.const 0) (i32.const 524289)) (i32.const -7)) (then unreachable))
            (if (i32.ne (call $input (i32.const -1) (i32.const 1)) (i32.const -14)) (then unreachable))
            (if (i32.ne (call $input (i32.const 589824) (i32.const 1)) (i32.const -14)) (then unreachable))
            (local.set $fd (call $input (i32.const 0) (i32.const 524288)))
            (if (i32.ne (local.get $fd) (i32.const 3)) (then unreachable))
            (if (call $close (local.get $fd)) (then unreachable))
            (local.set $fd (i32.const 3))
            (loop $fill
              (if (i32.ne (call $input (i32.const 589824) (i32.const 0)) (local.get $fd)) (then unreachable))
              (local.set $fd (i32.add (local.get $fd) (i32.const 1)))
              (br_if $fill (i32.lt_u (local.get $fd) (i32.const 128))))
            (if (i32.ne (call $input (i32.const 0) (i32.const 0)) (i32.const -24)) (then unreachable))
            (if (call $close (i32.const 17)) (then unreachable))
            (if (i32.ne (call $input (i32.const 0) (i32.const 0)) (i32.const 17)) (then unreachable))))"#);
    assert_eq!(result, (Status::Exited(0), vec![], vec![]));
}
