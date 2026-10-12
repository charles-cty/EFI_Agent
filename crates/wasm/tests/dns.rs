use efi_agent_wasm::{
    Status, World,
    clock::{Clock, ClockError, ClockId},
    network::{Connection, Network, NetworkError, Readiness},
    random::{Random, RandomError},
};
use std::{cell::Cell, rc::Rc};

struct Time(Cell<Result<u64, ClockError>>);
impl Clock for Time {
    fn now(&self, id: ClockId) -> Result<u64, ClockError> {
        assert_eq!(id, ClockId::Realtime);
        self.0.get()
    }
    fn resolution(&self, _: ClockId) -> Result<u64, ClockError> {
        Ok(1)
    }
}

struct Entropy {
    calls: Cell<usize>,
    failure: Cell<bool>,
}
impl Random for Entropy {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), RandomError> {
        self.calls.set(self.calls.get() + 1);
        assert_eq!(bytes.len(), 2);
        bytes.copy_from_slice(&[0x31, 0x79]);
        if self.failure.get() {
            Err(RandomError::Io)
        } else {
            Ok(())
        }
    }
}

#[derive(Default)]
struct Transport {
    attempts: Cell<usize>,
    drops: Rc<Cell<usize>>,
}
struct Pending(Rc<Cell<usize>>);
impl Drop for Pending {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
impl Network for Transport {
    fn connect(&self, address: [u8; 4], port: u16) -> Result<Box<dyn Connection>, NetworkError> {
        assert_eq!(address, [127, 2, 3, 4]);
        assert_eq!(port, 5353);
        self.attempts.set(self.attempts.get() + 1);
        Ok(Box::new(Pending(self.drops.clone())))
    }
}
impl Connection for Pending {
    fn poll_connect(&mut self) -> Result<Option<()>, NetworkError> {
        Ok(None)
    }
    fn poll_close(&mut self) -> Result<Option<()>, NetworkError> {
        unreachable!("Pending DNS cleanup drops the connection")
    }
    fn read(&mut self, _: usize) -> Result<Option<Vec<u8>>, NetworkError> {
        unreachable!("Connection remains pending")
    }
    fn write(&mut self, _: &[u8]) -> Result<Option<usize>, NetworkError> {
        unreachable!("Connection remains pending")
    }
    fn readiness(&mut self, _: bool) -> Result<Option<Readiness>, NetworkError> {
        unreachable!("DNS does not use readiness")
    }
}

fn setup() -> (World, Rc<Time>, Rc<Entropy>, Rc<Transport>) {
    let time = Rc::new(Time(Cell::new(Ok(731))));
    let random = Rc::new(Entropy {
        calls: Cell::new(0),
        failure: Cell::new(false),
    });
    let network = Rc::new(Transport::default());
    let mut world = World::new();
    world.set_clock(time.clone());
    world.set_random(random.clone());
    world.set_network(network.clone());
    world.set_dns_server([127, 2, 3, 4], 5353);
    (world, time, random, network)
}

fn spawn(world: &mut World, body: &str) -> u32 {
    let module = format!(
        r#"(module
        (import "efi_net" "resolve_ipv4" (func $resolve (param i32 i32 i32) (result i32)))
        (memory (export "memory") 1)
        (data (i32.const 1024) "MiXeD.Example.")
        (func (export "_start")
            (i32.store (i32.const 65532) (i32.const 1699299089))
            {body}
            (if (i32.ne (i32.load (i32.const 65532)) (i32.const 1699299089))
                (then unreachable))))"#
    );
    world
        .register("dns", &wat::parse_str(module).unwrap())
        .unwrap();
    world.spawn(vec!["dns".into()], vec![]).unwrap()
}

fn call(expected: i32) -> String {
    format!(
        "(if (i32.ne (call $resolve (i32.const 1024) (i32.const 14) (i32.const 65532)) (i32.const {expected})) (then unreachable))"
    )
}

fn finish(world: &mut World, pid: u32) {
    for _ in 0..100 {
        if !world.status(pid).unwrap().is_live() {
            break;
        }
        world.step();
    }
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
}

#[test]
fn deadline_boundary_preserves_output_and_releases_connection_before_reap() {
    let (mut world, time, _, network) = setup();
    let pid = spawn(&mut world, &call(73));
    world.step();
    assert_eq!(world.status(pid).unwrap(), &Status::Resolving);
    time.0.set(Ok(10_000_000_730));
    world.step();
    assert_eq!(world.status(pid).unwrap(), &Status::Resolving);
    assert_eq!(network.drops.get(), 0);
    time.0.set(Ok(10_000_000_731));
    finish(&mut world, pid);
    assert_eq!(network.attempts.get(), 1);
    assert_eq!(network.drops.get(), 1);
}

#[test]
fn cancellation_and_clock_failure_release_pending_query() {
    for cancel in [false, true] {
        let (mut world, time, _, network) = setup();
        let pid = spawn(&mut world, &call(29));
        world.step();
        assert_eq!(world.status(pid).unwrap(), &Status::Resolving);
        if cancel {
            world.cancel(pid).unwrap();
            assert_eq!(world.status(pid).unwrap(), &Status::Exited(130));
        } else {
            time.0.set(Err(ClockError::Io));
            finish(&mut world, pid);
        }
        assert_eq!(network.drops.get(), 1);
    }
}

#[test]
fn invalid_ranges_and_names_have_no_random_or_network_effects() {
    let (mut world, _, random, network) = setup();
    let pid = spawn(
        &mut world,
        r#"
        (if (i32.ne (call $resolve (i32.const 1024) (i32.const 14) (i32.const 65533)) (i32.const 21)) (then unreachable))
        (if (i32.ne (call $resolve (i32.const 65533) (i32.const 14) (i32.const 65532)) (i32.const 21)) (then unreachable))
        (if (i32.ne (call $resolve (i32.const 1024) (i32.const 0) (i32.const 65532)) (i32.const 28)) (then unreachable))
        "#,
    );
    finish(&mut world, pid);
    assert_eq!(random.calls.get(), 0);
    assert_eq!(network.attempts.get(), 0);
}

#[test]
fn entropy_failure_and_deadline_overflow_do_not_start_network() {
    for overflow in [false, true] {
        let (mut world, time, random, network) = setup();
        let expected = if overflow {
            time.0.set(Ok(u64::MAX - 9_999_999_999));
            61
        } else {
            random.failure.set(true);
            29
        };
        let pid = spawn(&mut world, &call(expected));
        finish(&mut world, pid);
        assert_eq!(random.calls.get(), usize::from(!overflow));
        assert_eq!(network.attempts.get(), 0);
    }
}
