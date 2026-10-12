use efi_agent_wasm::{
    Status, World,
    network::{Connection, Network, NetworkError, Readiness},
};
use std::{cell::Cell, rc::Rc};

struct Provider {
    attempts: Cell<usize>,
    dropped: Rc<Cell<usize>>,
    release: Rc<Cell<bool>>,
    close_release: Rc<Cell<bool>>,
    close_failure: Rc<Cell<bool>>,
    fail_first: bool,
}

struct Pending {
    release: Rc<Cell<bool>>,
    dropped: Rc<Cell<usize>>,
    fail: bool,
    close_release: Rc<Cell<bool>>,
    close_failure: Rc<Cell<bool>>,
}

impl Drop for Pending {
    fn drop(&mut self) {
        self.dropped.set(self.dropped.get() + 1);
    }
}

impl Network for Provider {
    fn connect(&self, address: [u8; 4], port: u16) -> Result<Box<dyn Connection>, NetworkError> {
        assert_eq!(address, [127, 2, 3, 4]);
        assert_eq!(port, 49153);
        let attempt = self.attempts.get();
        self.attempts.set(attempt + 1);
        Ok(Box::new(Pending {
            release: self.release.clone(),
            dropped: self.dropped.clone(),
            fail: self.fail_first && attempt == 0,
            close_release: self.close_release.clone(),
            close_failure: self.close_failure.clone(),
        }))
    }
}

impl Connection for Pending {
    fn poll_close(&mut self) -> Result<Option<()>, NetworkError> {
        if self.close_failure.get() {
            return Err(NetworkError::TimedOut);
        }
        Ok(self.close_release.get().then_some(()))
    }
    fn poll_connect(&mut self) -> Result<Option<()>, NetworkError> {
        if !self.release.get() {
            Ok(None)
        } else if self.fail {
            Err(NetworkError::ConnectionRefused)
        } else {
            Ok(Some(()))
        }
    }
    fn read(&mut self, _: usize) -> Result<Option<Vec<u8>>, NetworkError> {
        Ok(None)
    }
    fn write(&mut self, _: &[u8]) -> Result<Option<usize>, NetworkError> {
        Ok(None)
    }
    fn readiness(&mut self, _: bool) -> Result<Option<Readiness>, NetworkError> {
        Ok(None)
    }
}

fn provider(fail_first: bool) -> Rc<Provider> {
    Rc::new(Provider {
        attempts: Cell::new(0),
        dropped: Rc::new(Cell::new(0)),
        release: Rc::new(Cell::new(false)),
        close_release: Rc::new(Cell::new(false)),
        close_failure: Rc::new(Cell::new(false)),
        fail_first,
    })
}

fn spawn(world: &mut World, body: &str) -> u32 {
    let source = format!(
        r#"(module
      (import "efi_net" "tcp_connect" (func $connect (param i32 i32 i32) (result i32)))
      (import "efi_net" "tcp_open" (func $open (param i32 i32) (result i32)))
      (import "efi_net" "tcp_connect_fd" (func $connect_fd (param i32 i32 i32) (result i32)))
      (import "efi_net" "tcp_error" (func $error (param i32 i32) (result i32)))
      (import "efi_net" "tcp_close" (func $close_socket (param i32) (result i32)))
      (import "efi_process" "dup2" (func $dup (param i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_close" (func $close (param i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_fdstat_get" (func $stat (param i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_fdstat_set_flags" (func $flags (param i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_read" (func $read (param i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "poll_oneoff" (func $poll (param i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (data (i32.const 65532) "\7f\02\03\04")
      (func (export "_start") (local $fd i32) {body}))"#
    );
    world
        .register("network", &wat::parse_str(source).unwrap())
        .unwrap();
    world.spawn(vec!["network".into()], vec![]).unwrap()
}

fn finish(world: &mut World, pid: u32) {
    for _ in 0..1000 {
        if !world.status(pid).unwrap().is_live() {
            break;
        }
        world.step();
    }
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
}

#[test]
fn validation_precedes_network_effects_and_full_table_does_not_start_connect() {
    let network = provider(false);
    let mut world = World::new();
    world.set_network(network.clone());
    let pid = spawn(
        &mut world,
        r#"
      (i32.store (i32.const 0) (i32.const 1234567))
      (if (i32.ne (call $connect (i32.const 65533) (i32.const 49153) (i32.const 0)) (i32.const 21)) (then unreachable))
      (if (i32.ne (call $connect (i32.const 65532) (i32.const 49153) (i32.const 65533)) (i32.const 21)) (then unreachable))
      (if (i32.ne (call $connect (i32.const 65532) (i32.const 65536) (i32.const 0)) (i32.const 28)) (then unreachable))
      (if (i32.ne (call $connect (i32.const 65532) (i32.const 0) (i32.const 0)) (i32.const 28)) (then unreachable))
      (if (i32.ne (i32.load (i32.const 0)) (i32.const 1234567)) (then unreachable))
      (local.set $fd (i32.const 3))
      (loop $fill
        (drop (call $dup (i32.const 1) (local.get $fd)))
        (local.set $fd (i32.add (local.get $fd) (i32.const 1)))
        (br_if $fill (i32.lt_u (local.get $fd) (i32.const 128))))
      (if (i32.ne (call $connect (i32.const 65532) (i32.const 49153) (i32.const 0)) (i32.const 33)) (then unreachable))
      (if (i32.ne (i32.load (i32.const 0)) (i32.const 1234567)) (then unreachable))
    "#,
    );
    finish(&mut world, pid);
    assert_eq!(network.attempts.get(), 0);
}

#[test]
fn asynchronous_failure_preserves_output_frees_slot_and_allows_retry() {
    let network = provider(true);
    let mut world = World::new();
    world.set_network(network.clone());
    let pid = spawn(
        &mut world,
        r#"
      (i32.store (i32.const 0) (i32.const 7654321))
      (if (i32.ne (call $connect (i32.const 65532) (i32.const 49153) (i32.const 0)) (i32.const 14)) (then unreachable))
      (if (i32.ne (i32.load (i32.const 0)) (i32.const 7654321)) (then unreachable))
      (if (call $connect (i32.const 65532) (i32.const 49153) (i32.const 65532)) (then unreachable))
      (if (i32.ne (i32.load (i32.const 65532)) (i32.const 3)) (then unreachable))
      (if (call $close (i32.const 3)) (then unreachable))
    "#,
    );
    world.step();
    assert_eq!(world.status(pid).unwrap(), &Status::Connecting);
    assert_eq!(network.attempts.get(), 1);
    assert_eq!(world.step(), None);
    assert_eq!(network.attempts.get(), 1);
    network.release.set(true);
    finish(&mut world, pid);
    assert_eq!(network.attempts.get(), 2);
    assert_eq!(network.dropped.get(), 2);
}

#[test]
fn cancelling_connect_drops_pending_connection_and_provider_absence_is_explicit() {
    let network = provider(false);
    let mut world = World::new();
    world.set_network(network.clone());
    let pid = spawn(
        &mut world,
        "(drop (call $connect (i32.const 65532) (i32.const 49153) (i32.const 0)))",
    );
    world.step();
    assert_eq!(world.status(pid).unwrap(), &Status::Connecting);
    world.cancel(pid).unwrap();
    assert_eq!(network.dropped.get(), 1);
    assert_eq!(world.step(), None);
    assert_eq!(world.reap(pid).unwrap(), Status::Exited(130));

    let mut world = World::new();
    let pid = spawn(
        &mut world,
        "(if (i32.ne (call $connect (i32.const 65532) (i32.const 49153) (i32.const 0)) (i32.const 58)) (then unreachable))",
    );
    finish(&mut world, pid);
}

#[test]
fn port_boundaries_are_unsigned_and_immediate_failure_leaves_output_unchanged() {
    struct Ports(std::cell::RefCell<Vec<u16>>);
    impl Network for Ports {
        fn connect(
            &self,
            address: [u8; 4],
            port: u16,
        ) -> Result<Box<dyn Connection>, NetworkError> {
            assert_eq!(address, [127, 2, 3, 4]);
            self.0.borrow_mut().push(port);
            Err(NetworkError::ConnectionRefused)
        }
    }
    let network = Rc::new(Ports(std::cell::RefCell::new(Vec::new())));
    let mut world = World::new();
    world.set_network(network.clone());
    let pid = spawn(
        &mut world,
        r#"
      (i32.store (i32.const 0) (i32.const 98765))
      (if (i32.ne (call $connect (i32.const 65532) (i32.const 1) (i32.const 0)) (i32.const 14)) (then unreachable))
      (if (i32.ne (call $connect (i32.const 65532) (i32.const 65535) (i32.const 0)) (i32.const 14)) (then unreachable))
      (if (i32.ne (call $connect (i32.const 65532) (i32.const 65536) (i32.const 0)) (i32.const 28)) (then unreachable))
      (if (i32.ne (call $connect (i32.const 65532) (i32.const -1) (i32.const 0)) (i32.const 28)) (then unreachable))
      (if (i32.ne (call $connect (i32.const 65532) (i32.const 0) (i32.const 0)) (i32.const 28)) (then unreachable))
      (if (i32.ne (i32.load (i32.const 0)) (i32.const 98765)) (then unreachable))
    "#,
    );
    finish(&mut world, pid);
    assert_eq!(*network.0.borrow(), vec![1, 65535]);
}

#[test]
fn graceful_close_suspends_without_blocking_peer_and_consumes_descriptor_once() {
    let network = provider(false);
    network.release.set(true);
    let mut world = World::new();
    world.set_network(network.clone());
    let pid = spawn(
        &mut world,
        r#"
      (if (i32.ne (call $close_socket (i32.const 127)) (i32.const 8)) (then unreachable))
      (if (i32.ne (call $close_socket (i32.const 1)) (i32.const 57)) (then unreachable))
      (if (call $connect (i32.const 65532) (i32.const 49153) (i32.const 0)) (then unreachable))
      (if (call $close_socket (i32.load (i32.const 0))) (then unreachable))
      (if (i32.ne (call $close_socket (i32.load (i32.const 0))) (i32.const 8)) (then unreachable))
    "#,
    );
    for _ in 0..100 {
        world.step();
        if matches!(world.status(pid).unwrap(), Status::Closing(_)) {
            break;
        }
    }
    assert_eq!(world.status(pid).unwrap(), &Status::Closing(3));
    world
        .register(
            "peer",
            &wat::parse_str(r#"(module (memory (export "memory") 1) (func (export "_start")))"#)
                .unwrap(),
        )
        .unwrap();
    let peer = world.spawn(vec!["peer".into()], vec![]).unwrap();
    assert_eq!(world.step(), Some(peer));
    assert_eq!(world.status(peer).unwrap(), &Status::Exited(0));
    assert_eq!(network.dropped.get(), 0);
    network.close_release.set(true);
    finish(&mut world, pid);
    assert_eq!(network.dropped.get(), 1);
}

#[test]
fn cancellation_during_graceful_close_releases_pending_connection() {
    let network = provider(false);
    network.release.set(true);
    let mut world = World::new();
    world.set_network(network.clone());
    let pid = spawn(
        &mut world,
        r#"
      (drop (call $connect (i32.const 65532) (i32.const 49153) (i32.const 0)))
      (drop (call $close_socket (i32.load (i32.const 0))))
    "#,
    );
    world.step();
    assert_eq!(world.status(pid).unwrap(), &Status::Closing(3));
    world.cancel(pid).unwrap();
    assert_eq!(network.dropped.get(), 1);
    assert_eq!(world.step(), None);
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(130));
}

#[test]
fn transport_error_during_close_consumes_descriptor_and_returns_exact_errno() {
    let network = provider(false);
    network.release.set(true);
    network.close_failure.set(true);
    let mut world = World::new();
    world.set_network(network.clone());
    let pid = spawn(
        &mut world,
        r#"
      (if (call $connect (i32.const 65532) (i32.const 49153) (i32.const 0)) (then unreachable))
      (if (i32.ne (call $close_socket (i32.load (i32.const 0))) (i32.const 73)) (then unreachable))
      (if (i32.ne (call $close (i32.load (i32.const 0))) (i32.const 8)) (then unreachable))
    "#,
    );
    finish(&mut world, pid);
    assert_eq!(network.dropped.get(), 1);
}

#[test]
fn nonblocking_socket_flags_are_shared_and_again_preserves_guest_outputs() {
    let network = provider(false);
    network.release.set(true);
    let mut world = World::new();
    world.set_network(network.clone());
    let pid = spawn(
        &mut world,
        r#"
      (if (call $connect (i32.const 65532) (i32.const 49153) (i32.const 0)) (then unreachable))
      (if (i32.ne (call $dup (i32.const 3) (i32.const 9)) (i32.const 9)) (then unreachable))
      (if (call $flags (i32.const 3) (i32.const 4)) (then unreachable))
      (if (call $stat (i32.const 9) (i32.const 64)) (then unreachable))
      (if (i32.ne (i32.load16_u (i32.const 66)) (i32.const 4)) (then unreachable))
      (if (i64.eqz (i64.and (i64.load (i32.const 72)) (i64.const 8))) (then unreachable))
      (if (i32.ne (call $flags (i32.const 9) (i32.const 5)) (i32.const 58)) (then unreachable))
      (if (i32.ne (call $flags (i32.const 9) (i32.const 32)) (i32.const 28)) (then unreachable))
      (if (call $stat (i32.const 3) (i32.const 64)) (then unreachable))
      (if (i32.ne (i32.load16_u (i32.const 66)) (i32.const 4)) (then unreachable))
      (i32.store (i32.const 128) (i32.const 256))
      (i32.store (i32.const 132) (i32.const 1))
      (i32.store8 (i32.const 256) (i32.const 137))
      (i32.store (i32.const 512) (i32.const 97831))
      (if (i32.ne (call $read (i32.const 9) (i32.const 128) (i32.const 1) (i32.const 512)) (i32.const 6)) (then unreachable))
      (if (i32.ne (call $write (i32.const 3) (i32.const 128) (i32.const 1) (i32.const 512)) (i32.const 6)) (then unreachable))
      (if (i32.ne (i32.load (i32.const 512)) (i32.const 97831)) (then unreachable))
      (if (i32.ne (i32.load8_u (i32.const 256)) (i32.const 137)) (then unreachable))
      (i32.store (i32.const 132) (i32.const 0))
      (if (call $read (i32.const 9) (i32.const 128) (i32.const 1) (i32.const 512)) (then unreachable))
      (if (i32.load (i32.const 512)) (then unreachable))
      (if (call $write (i32.const 3) (i32.const 128) (i32.const 1) (i32.const 512)) (then unreachable))
      (if (i32.load (i32.const 512)) (then unreachable))
      (if (call $flags (i32.const 9) (i32.const 0)) (then unreachable))
      (if (call $stat (i32.const 3) (i32.const 64)) (then unreachable))
      (if (i32.load16_u (i32.const 66)) (then unreachable))
      (i32.store (i32.const 132) (i32.const 1))
      (drop (call $read (i32.const 3) (i32.const 128) (i32.const 1) (i32.const 512)))
    "#,
    );
    world.step();
    assert_eq!(world.status(pid).unwrap(), &Status::Reading(3));
    assert_eq!(world.step(), None);
    world.cancel(pid).unwrap();
    assert_eq!(network.dropped.get(), 1);
}

#[test]
fn asynchronous_connect_poll_error_clear_and_retry_share_one_socket() {
    for failure in [false, true] {
        let network = provider(failure);
        let mut world = World::new();
        world.set_network(network.clone());
        let expected = if failure { 14 } else { 0 };
        let retry = if failure {
            r#"
            (i32.store (i32.const 320) (i32.const 384))
            (i32.store (i32.const 324) (i32.const 1))
            (i32.store (i32.const 512) (i32.const 7719))
            (if (i32.ne (call $read (i32.const 9) (i32.const 320) (i32.const 1) (i32.const 512)) (i32.const 53)) (then unreachable))
            (if (i32.ne (i32.load (i32.const 512)) (i32.const 7719)) (then unreachable))
            (if (call $connect_fd (i32.const 9) (i32.const 65532) (i32.const 49153)) (then unreachable))
            "#
        } else {
            ""
        };
        let body = format!(
            r#"
          (if (call $open (i32.const 65540) (i32.const 0)) (then unreachable))
          (if (i32.ne (i32.load (i32.const 0)) (i32.const 3)) (then unreachable))
          (if (i32.ne (call $dup (i32.const 3) (i32.const 9)) (i32.const 9)) (then unreachable))
          (if (call $stat (i32.const 9) (i32.const 64)) (then unreachable))
          (if (i32.ne (i32.load16_u (i32.const 66)) (i32.const 4)) (then unreachable))
          (if (i32.ne (call $connect_fd (i32.const 3) (i32.const 65532) (i32.const 49153)) (i32.const 26)) (then unreachable))
          (if (i32.ne (call $connect_fd (i32.const 9) (i32.const 65532) (i32.const 49153)) (i32.const 7)) (then unreachable))
          (if (call $error (i32.const 9) (i32.const 64)) (then unreachable))
          (if (i32.load (i32.const 64)) (then unreachable))
          (i64.store (i32.const 128) (i64.const 91731))
          (i32.store8 (i32.const 136) (i32.const 2))
          (i32.store (i32.const 144) (i32.const 9))
          (if (call $poll (i32.const 128) (i32.const 256) (i32.const 1) (i32.const 512)) (then unreachable))
          (if (i32.ne (i32.load (i32.const 512)) (i32.const 1)) (then unreachable))
          (if (i64.ne (i64.load (i32.const 256)) (i64.const 91731)) (then unreachable))
          (if (i32.ne (i32.load16_u (i32.const 264)) (i32.const {expected})) (then unreachable))
          (if (i32.ne (call $error (i32.const 9) (i32.const 65533)) (i32.const 21)) (then unreachable))
          (if (call $error (i32.const 3) (i32.const 64)) (then unreachable))
          (if (i32.ne (i32.load (i32.const 64)) (i32.const {expected})) (then unreachable))
          (if (call $error (i32.const 9) (i32.const 64)) (then unreachable))
          (if (i32.load (i32.const 64)) (then unreachable))
          {retry}
          (if (i32.ne (call $connect_fd (i32.const 3) (i32.const 65532) (i32.const 49153)) (i32.const 30)) (then unreachable))
          (if (call $close (i32.const 3)) (then unreachable))
          (if (call $close (i32.const 9)) (then unreachable))
        "#
        );
        let pid = spawn(&mut world, &body);
        world.step();
        assert_eq!(world.status(pid).unwrap(), &Status::Polling);
        assert_eq!(network.attempts.get(), 1);
        assert_eq!(world.step(), None);
        network.release.set(true);
        finish(&mut world, pid);
        assert_eq!(network.attempts.get(), if failure { 2 } else { 1 });
        assert_eq!(network.dropped.get(), network.attempts.get());
    }
}

#[test]
fn socket_creation_validation_and_blocking_connect_preserve_descriptor_on_failure() {
    let network = provider(true);
    let mut world = World::new();
    world.set_network(network.clone());
    let pid = spawn(
        &mut world,
        r#"
      (i32.store (i32.const 0) (i32.const 7193))
      (if (i32.ne (call $open (i32.const 4) (i32.const 65533)) (i32.const 21)) (then unreachable))
      (if (i32.ne (call $open (i32.const 1) (i32.const 0)) (i32.const 28)) (then unreachable))
      (if (i32.ne (i32.load (i32.const 0)) (i32.const 7193)) (then unreachable))
      (if (call $open (i32.const 0) (i32.const 0)) (then unreachable))
      (if (i32.ne (i32.load (i32.const 0)) (i32.const 3)) (then unreachable))
      (if (i32.ne (call $connect_fd (i32.const 1) (i32.const 65532) (i32.const 49153)) (i32.const 57)) (then unreachable))
      (if (i32.ne (call $connect_fd (i32.const 127) (i32.const 65532) (i32.const 49153)) (i32.const 8)) (then unreachable))
      (if (i32.ne (call $connect_fd (i32.const 3) (i32.const 65533) (i32.const 49153)) (i32.const 21)) (then unreachable))
      (if (i32.ne (call $connect_fd (i32.const 3) (i32.const 65532) (i32.const 65536)) (i32.const 28)) (then unreachable))
      (if (i32.ne (call $connect_fd (i32.const 3) (i32.const 65532) (i32.const 49153)) (i32.const 14)) (then unreachable))
      (if (call $stat (i32.const 3) (i32.const 64)) (then unreachable))
      (if (call $close (i32.const 3)) (then unreachable))
    "#,
    );
    world.step();
    assert_eq!(world.status(pid).unwrap(), &Status::Connecting);
    assert_eq!(network.attempts.get(), 1);
    network.release.set(true);
    finish(&mut world, pid);
    assert_eq!(network.dropped.get(), 1);
}

#[test]
fn cancelling_nonblocking_connect_poll_releases_connection_with_duplicates() {
    let network = provider(false);
    let mut world = World::new();
    world.set_network(network.clone());
    let pid = spawn(
        &mut world,
        r#"
      (if (call $open (i32.const 4) (i32.const 0)) (then unreachable))
      (drop (call $dup (i32.const 3) (i32.const 9)))
      (if (i32.ne (call $connect_fd (i32.const 3) (i32.const 65532) (i32.const 49153)) (i32.const 26)) (then unreachable))
      (i32.store8 (i32.const 136) (i32.const 2))
      (i32.store (i32.const 144) (i32.const 9))
      (drop (call $poll (i32.const 128) (i32.const 256) (i32.const 1) (i32.const 512)))
    "#,
    );
    world.step();
    assert_eq!(world.status(pid).unwrap(), &Status::Polling);
    world
        .register(
            "peer",
            &wat::parse_str("(module (memory (export \"memory\") 1) (func (export \"_start\")))")
                .unwrap(),
        )
        .unwrap();
    let peer = world.spawn(vec!["peer".into()], vec![]).unwrap();
    assert_eq!(world.step(), Some(peer));
    assert_eq!(world.status(peer).unwrap(), &Status::Exited(0));
    assert_eq!(network.dropped.get(), 0);
    world.cancel(pid).unwrap();
    assert_eq!(network.dropped.get(), 1);
}
