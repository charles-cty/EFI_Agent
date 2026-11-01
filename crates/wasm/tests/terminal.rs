use efi_agent_wasm::{Status, World, filesystem::namespace::Namespace, session::SessionKind};
use std::rc::Rc;

const MODULE: &str = r#"(module
    (import "efi_process" "terminal_get" (func $get (param i32 i32) (result i32)))
    (import "efi_process" "terminal_set" (func $set (param i32 i32) (result i32)))
    (import "wasi_snapshot_preview1" "path_open" (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
    (import "wasi_snapshot_preview1" "fd_read" (func $read (param i32 i32 i32 i32) (result i32)))
    (import "wasi_snapshot_preview1" "fd_fdstat_get" (func $stat (param i32 i32) (result i32)))
    (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
    (memory (export "memory") 1)
    (data (i32.const 0) "/dev/tty")
    (data (i32.const 16) "/dev/null")
    (func $equal (param $a i32) (param $b i32)
        (if (i32.ne (local.get $a) (local.get $b)) (then unreachable)))
    (func $open_path (param $ptr i32) (param $len i32) (param $flags i32) (result i32)
        (call $equal (call $open (i32.const 3) (i32.const 0) (local.get $ptr) (local.get $len)
            (i32.const 0) (i64.const 134217802) (i64.const 0) (local.get $flags) (i32.const 48)) (i32.const 0))
        (i32.load (i32.const 48)))
    (func (export "_start") (local $tty i32) (local $file i32)
        (call $equal (call $get (i32.const 0) (i32.const 64)) (i32.const 0))
        (call $equal (i32.load (i32.const 64)) (i32.const 7))
        (call $equal (i32.load (i32.const 68)) (i32.const 101))
        (call $equal (i32.load (i32.const 72)) (i32.const 37))
        (call $equal (call $get (i32.const 0) (i32.const 65530)) (i32.const -14))
        (call $equal (call $get (i32.const 127) (i32.const 64)) (i32.const -9))
        (local.set $file (call $open_path (i32.const 16) (i32.const 9) (i32.const 0)))
        (call $equal (call $get (local.get $file) (i32.const 64)) (i32.const -25))
        (call $equal (call $set (i32.const 0) (i32.const 8)) (i32.const -22))
        (call $equal (call $set (i32.const 0) (i32.const 0)) (i32.const 0))
        (local.set $tty (call $open_path (i32.const 0) (i32.const 8) (i32.const 4)))
        (call $equal (call $stat (local.get $tty) (i32.const 80)) (i32.const 0))
        (call $equal (i32.load8_u (i32.const 80)) (i32.const 2))
        (call $equal (i32.load16_u (i32.const 82)) (i32.const 4))
        (i32.store (i32.const 112) (i32.const 128))
        (i32.store (i32.const 116) (i32.const 1))
        (call $equal (call $read (local.get $tty) (i32.const 112) (i32.const 1) (i32.const 120)) (i32.const 6))
        (loop $wait (br_if $wait (i32.eq (call $read (local.get $tty) (i32.const 112) (i32.const 1) (i32.const 120)) (i32.const 6))))
        (call $equal (i32.load8_u (i32.const 128)) (i32.const 113))
        (call $exit (i32.const 0))))"#;

#[test]
fn direct_terminal_validates_descriptors_modes_and_memory_and_resets_on_exit() {
    let mut world = World::new();
    world
        .set_filesystem(Rc::new(Namespace::temporary(65536, 32, 1, None)))
        .unwrap();
    world
        .register("ash", &wat::parse_str(MODULE).unwrap())
        .unwrap();
    let mut session = world.start_session(SessionKind::Shell).unwrap();
    session.submit_terminal("unused").unwrap();
    session.set_terminal_size(101, 37);
    for _ in 0..50 {
        world.step();
    }
    assert!(world.status(session.pid()).unwrap().is_live());
    assert_eq!(session.terminal_mode(), 0);
    session.write_stdin(b"q").unwrap();
    for _ in 0..50 {
        world.step();
    }
    assert_eq!(world.status(session.pid()).unwrap(), &Status::Exited(0));
    assert_eq!(session.terminal_mode(), 7);
}

#[test]
fn retained_tty_descriptors_report_detachment_instead_of_capturing_tool_output() {
    let tail = r#"(call $equal (call $read (local.get $tty) (i32.const 112) (i32.const 1) (i32.const 120)) (i32.const 60))
        (call $equal (call $stat (local.get $tty) (i32.const 80)) (i32.const 0))
        (call $equal (i32.load8_u (i32.const 80)) (i32.const 0))
        (call $equal (call $get (local.get $tty) (i32.const 64)) (i32.const -25))"#;
    let module = MODULE.replace(
        "(call $equal (i32.load8_u (i32.const 128)) (i32.const 113))",
        tail,
    );
    let mut world = World::new();
    world
        .set_filesystem(Rc::new(Namespace::temporary(65536, 32, 1, None)))
        .unwrap();
    world
        .register("ash", &wat::parse_str(module).unwrap())
        .unwrap();
    let mut session = world.start_session(SessionKind::Shell).unwrap();
    session.set_terminal_size(101, 37);
    session.submit_terminal("unused").unwrap();
    for _ in 0..50 {
        world.step();
    }
    assert_eq!(session.terminal_mode(), 0);
    session.detach_terminal();
    for _ in 0..50 {
        world.step();
    }
    assert_eq!(world.status(session.pid()).unwrap(), &Status::Exited(0));
    assert!(session.take_terminal_output().is_empty());
}

#[test]
fn captured_tools_have_no_controlling_terminal() {
    let mut world = World::new();
    let module = MODULE.replace(
        "(call $equal (call $get (i32.const 0) (i32.const 64)) (i32.const 0))",
        "(call $equal (call $get (i32.const 0) (i32.const 64)) (i32.const -25)) (call $exit (i32.const 0))",
    );
    world
        .register("tool", &wat::parse_str(module).unwrap())
        .unwrap();
    let pid = world.spawn(vec!["tool".into()], vec![]).unwrap();
    for _ in 0..20 {
        world.step();
    }
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
}
