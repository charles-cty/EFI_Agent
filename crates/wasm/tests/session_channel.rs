use efi_agent_wasm::{Status, World, filesystem::namespace::Namespace};
use std::rc::Rc;

#[test]
fn session_stream_waits_without_consuming_stdin_and_cannot_escape_through_c_imports() {
    let module = wat::parse_str(r#"(module
      (import "wasi_snapshot_preview1" "path_open" (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_read" (func $read (param i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_renumber" (func $renumber (param i32 i32) (result i32)))
      (import "efi_process" "get_fd_flags" (func $flags (param i32) (result i32)))
      (import "efi_process" "set_fd_flags" (func $set (param i32 i32) (result i32)))
      (import "efi_process" "dupfd" (func $dup (param i32 i32) (result i32)))
      (import "efi_process" "dup2" (func $dup2 (param i32 i32) (result i32)))
      (memory (export "memory") 1)
      (data (i32.const 0) ".efi-session")
      (func $equal (param i32 i32)
        (if (i32.ne (local.get 0) (local.get 1)) (then unreachable)))
      (func (export "_start")
        (call $equal (call $open (i32.const 3) (i32.const 0) (i32.const 0) (i32.const 12) (i32.const 0) (i64.const 66) (i64.const 0) (i32.const 0) (i32.const 32)) (i32.const 0))
        (call $equal (i32.load (i32.const 32)) (i32.const 4))
        (call $equal (call $flags (i32.const 4)) (i32.const 1))
        (call $equal (call $set (i32.const 4) (i32.const 0)) (i32.const -22))
        (call $equal (call $dup (i32.const 4) (i32.const 6)) (i32.const -9))
        (call $equal (call $dup2 (i32.const 4) (i32.const 6)) (i32.const -9))
        (call $equal (call $dup2 (i32.const 0) (i32.const 4)) (i32.const -9))
        (call $equal (call $renumber (i32.const 4) (i32.const 6)) (i32.const 8))
        (call $equal (call $renumber (i32.const 0) (i32.const 4)) (i32.const 8))
        (i32.store (i32.const 48) (i32.const 100))
        (i32.store (i32.const 52) (i32.const 8))
        (call $equal (call $read (i32.const 0) (i32.const 48) (i32.const 1) (i32.const 60)) (i32.const 0))
        (call $equal (i32.load (i32.const 60)) (i32.const 0))
        (call $equal (call $read (i32.const 4) (i32.const 48) (i32.const 1) (i32.const 60)) (i32.const 0))
        (call $equal (i32.load (i32.const 60)) (i32.const 8))
        (call $equal (i32.load (i32.const 100)) (i32.const 1))
        (call $equal (i32.load (i32.const 104)) (i32.const 1))
        (i32.store (i32.const 48) (i32.const 108))
        (i32.store (i32.const 52) (i32.const 1))
        (call $equal (call $read (i32.const 4) (i32.const 48) (i32.const 1) (i32.const 60)) (i32.const 0))
        (call $equal (i32.load8_u (i32.const 108)) (i32.const 120))
        (i32.store (i32.const 48) (i32.const 100))
        (i32.store (i32.const 52) (i32.const 7))
        (call $equal (call $write (i32.const 4) (i32.const 48) (i32.const 1) (i32.const 60)) (i32.const 28))
        (i32.store (i32.const 52) (i32.const 8))
        (i32.store (i32.const 104) (i32.const 7))
        (call $equal (call $write (i32.const 4) (i32.const 48) (i32.const 1) (i32.const 60)) (i32.const 0))
        (loop $hold (br $hold))))"#).unwrap();
    let mut world = World::new();
    world
        .set_filesystem(Rc::new(Namespace::temporary(65536, 32, 1, None)))
        .unwrap();
    world.register("ash", &module).unwrap();
    let mut session = world
        .start_session(efi_agent_wasm::session::SessionKind::Shell)
        .unwrap();
    for _ in 0..100 {
        world.step();
    }
    assert_eq!(world.status(session.pid()).unwrap(), &Status::Reading(4));
    assert!(session.take_completion().is_none());
    session.submit("x").unwrap();
    let mut completion = None;
    for _ in 0..100 {
        world.step();
        completion = session.take_completion();
        if completion.is_some() {
            break;
        }
    }
    assert_eq!(completion, Some(7));
    assert_eq!(world.take_output(session.pid()).unwrap(), (vec![], vec![]));
    world.cancel(session.pid()).unwrap();
    world.reap(session.pid()).unwrap();
}
