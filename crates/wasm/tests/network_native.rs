#[path = "../examples/support/native_net.rs"]
mod native_net;

use efi_agent_wasm::{Status, World};
use std::{
    io::Read,
    net::TcpListener,
    rc::Rc,
    time::{Duration, Instant},
};

#[test]
fn blocked_native_tcp_read_does_not_block_peer_and_cancellation_closes_socket() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let mut world = World::new();
    world.set_network(Rc::new(native_net::NativeNetwork));
    let source = format!(
        r#"(module
      (import "efi_net" "tcp_connect" (func $connect (param i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_read" (func $read (param i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (data (i32.const 32) "\7f\00\00\01")
      (func (export "_start")
        (if (call $connect (i32.const 32) (i32.const {port}) (i32.const 0)) (then unreachable))
        (i32.store (i32.const 8) (i32.const 64))
        (i32.store (i32.const 12) (i32.const 17))
        (drop (call $read (i32.load (i32.const 0)) (i32.const 8) (i32.const 1) (i32.const 4)))
        unreachable))"#
    );
    world
        .register("network", &wat::parse_str(source).unwrap())
        .unwrap();
    world
        .register(
            "peer",
            &wat::parse_str(
                r#"(module
      (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
      (memory (export "memory") 1)
      (func (export "_start") (call $exit (i32.const 37))))"#,
            )
            .unwrap(),
        )
        .unwrap();
    let blocked = world.spawn(vec!["network".into()], vec![]).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !matches!(world.status(blocked).unwrap(), Status::Reading(_)) {
        assert!(Instant::now() < deadline, "TCP read did not suspend");
        assert!(world.status(blocked).unwrap().is_live());
        world.step();
        std::thread::yield_now();
    }
    let (mut server, _) = listener.accept().unwrap();
    server.set_nonblocking(true).unwrap();
    let peer = world.spawn(vec!["peer".into()], vec![]).unwrap();
    assert_eq!(world.step(), Some(peer));
    assert_eq!(world.status(peer).unwrap(), &Status::Exited(37));
    assert!(matches!(world.status(blocked).unwrap(), Status::Reading(_)));
    world.cancel(blocked).unwrap();
    assert_eq!(world.status(blocked).unwrap(), &Status::Exited(130));
    loop {
        match server.read(&mut [0]) {
            Ok(0) => break,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "Cancelled TCP socket remained open"
                );
                std::thread::yield_now();
            }
            result => panic!("Unexpected bytes/error after cancellation: {result:?}"),
        }
    }
    assert_eq!(world.reap(blocked).unwrap(), Status::Exited(130));
}
