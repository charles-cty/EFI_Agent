use efi_agent_wasm::{Status, World, filesystem::namespace::Namespace};
use std::rc::Rc;

#[test]
fn direct_terminal_preserves_write_order_waits_for_stdin_and_restores_finite_tool_input() {
    let module = wat::parse_str(r#"(module
      (import "wasi_snapshot_preview1" "path_open" (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_read" (func $read (param i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (data (i32.const 0) ".efi-session")
      (data (i32.const 32) "ABC")
      (func $check (param $errno i32) (if (local.get $errno) (then unreachable)))
      (func $out (param $fd i32) (param $ptr i32) (param $len i32)
        (i32.store (i32.const 64) (local.get $ptr))
        (i32.store (i32.const 68) (local.get $len))
        (call $check (call $write (local.get $fd) (i32.const 64) (i32.const 1) (i32.const 72))))
      (func (export "_start") (local $session i32)
        (call $check (call $open (i32.const 3) (i32.const 0) (i32.const 0) (i32.const 12) (i32.const 0) (i64.const 66) (i64.const 0) (i32.const 0) (i32.const 48)))
        (local.set $session (i32.load (i32.const 48)))
        (loop $request
          (i32.store (i32.const 64) (i32.const 100))
          (i32.store (i32.const 68) (i32.const 13))
          (call $check (call $read (local.get $session) (i32.const 64) (i32.const 1) (i32.const 72)))
          (if (i32.eqz (i32.load (i32.const 72))) (then return))
          (call $out (i32.const 1) (i32.const 32) (i32.const 1))
          (call $out (i32.const 2) (i32.const 33) (i32.const 1))
          (call $out (i32.const 1) (i32.const 34) (i32.const 1))
          (block $eof (loop $input
            (i32.store (i32.const 64) (i32.const 128))
            (i32.store (i32.const 68) (i32.const 256))
            (call $check (call $read (i32.const 0) (i32.const 64) (i32.const 1) (i32.const 72)))
            (br_if $eof (i32.eqz (i32.load (i32.const 72))))
            (call $out (i32.const 1) (i32.const 128) (i32.load (i32.const 72)))
            (br $input)))
          (i32.store (i32.const 104) (i32.const 0))
          (call $out (local.get $session) (i32.const 100) (i32.const 8))
          (br $request))))"#).unwrap();
    let mut world = World::new();
    world
        .set_filesystem(Rc::new(Namespace::temporary(65536, 32, 1, None)))
        .unwrap();
    world.register("ash", &module).unwrap();
    let mut session = world
        .start_session(efi_agent_wasm::session::SessionKind::Shell)
        .unwrap();
    session.submit_terminal("x").unwrap();
    for _ in 0..100 {
        world.step();
    }
    assert_eq!(world.status(session.pid()).unwrap(), &Status::Reading(0));
    assert_eq!(session.take_terminal_output(), b"ABC");
    assert_eq!(world.take_output(session.pid()).unwrap(), (vec![], vec![]));
    session.write_stdin("input 中\n".as_bytes()).unwrap();
    for _ in 0..100 {
        world.step();
    }
    assert_eq!(session.take_terminal_output(), "input 中\n".as_bytes());
    assert_eq!(world.status(session.pid()).unwrap(), &Status::Reading(0));
    assert!(session.take_completion().is_none());
    session.close_stdin();
    for _ in 0..100 {
        world.step();
    }
    assert_eq!(session.take_completion(), Some(0));
    session.detach_terminal();
    session.submit("y").unwrap();
    for _ in 0..100 {
        world.step();
    }
    assert_eq!(session.take_completion(), Some(0));
    assert!(session.take_terminal_output().is_empty());
    assert_eq!(
        world.take_output(session.pid()).unwrap(),
        (b"AC".to_vec(), b"B".to_vec())
    );
    assert_eq!(world.status(session.pid()).unwrap(), &Status::Reading(4));
    session.close_source().unwrap();
    assert!(session.submit("after EOF").is_err());
    for _ in 0..100 {
        world.step();
    }
    assert_eq!(world.status(session.pid()).unwrap(), &Status::Exited(0));
    assert!(session.take_terminal_output().is_empty());
    world.reap(session.pid()).unwrap();
}

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
        (i32.store (i32.const 52) (i32.const 12))
        (call $equal (call $read (i32.const 4) (i32.const 48) (i32.const 1) (i32.const 60)) (i32.const 0))
        (call $equal (i32.load (i32.const 60)) (i32.const 12))
        (call $equal (i32.load (i32.const 100)) (i32.const 1))
        (call $equal (i32.load (i32.const 104)) (i32.const 1))
        (call $equal (i32.load (i32.const 108)) (i32.const 0))
        (i32.store (i32.const 48) (i32.const 112))
        (i32.store (i32.const 52) (i32.const 1))
        (call $equal (call $read (i32.const 4) (i32.const 48) (i32.const 1) (i32.const 60)) (i32.const 0))
        (call $equal (i32.load8_u (i32.const 112)) (i32.const 120))
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
