use efi_agent_wasm::{
    Status, World,
    clock::{Clock, ClockError, ClockId},
};
use std::{cell::Cell, rc::Rc};

struct TestClock {
    time: Cell<u64>,
    failed: Cell<bool>,
}

#[test]
fn relative_realtime_wait_uses_elapsed_time_but_absolute_wait_tracks_the_wall() {
    struct SeparateClocks {
        wall: Cell<u64>,
        elapsed: Cell<u64>,
    }
    impl Clock for SeparateClocks {
        fn now(&self, id: ClockId) -> Result<u64, ClockError> {
            match id {
                ClockId::Realtime => Ok(self.wall.get()),
                ClockId::Monotonic => Ok(self.elapsed.get()),
                _ => Err(ClockError::Unavailable),
            }
        }
        fn resolution(&self, _: ClockId) -> Result<u64, ClockError> {
            Ok(1)
        }
    }
    let source = wat::parse_str(r#"(module
      (import "wasi_snapshot_preview1" "poll_oneoff" (func $poll (param i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (func (export "_start")
        (i64.store (i32.const 152) (i64.const 5))
        (if (call $poll (i32.const 128) (i32.const 256) (i32.const 1) (i32.const 0)) (then unreachable))
        (i64.store (i32.const 152) (i64.const 500))
        (i32.store16 (i32.const 168) (i32.const 1))
        (if (call $poll (i32.const 128) (i32.const 256) (i32.const 1) (i32.const 0)) (then unreachable))))"#).unwrap();
    let clock = Rc::new(SeparateClocks {
        wall: Cell::new(1000),
        elapsed: Cell::new(10),
    });
    let mut world = World::new();
    world.set_clock(clock.clone());
    world.register("clock", &source).unwrap();
    let pid = world.spawn(vec!["clock".into()], vec![]).unwrap();
    assert_eq!(world.step(), Some(pid));
    assert_eq!(world.status(pid).unwrap(), &Status::Polling);
    clock.wall.set(1_000_000);
    clock.elapsed.set(14);
    assert_eq!(world.step(), None);
    clock.wall.set(0);
    clock.elapsed.set(15);
    assert_eq!(world.step(), Some(pid));
    assert_eq!(world.status(pid).unwrap(), &Status::Polling);
    clock.elapsed.set(100_000);
    assert_eq!(world.step(), None);
    clock.wall.set(499);
    assert_eq!(world.step(), None);
    clock.wall.set(500);
    assert_eq!(world.step(), Some(pid));
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
}
impl Clock for TestClock {
    fn now(&self, id: ClockId) -> Result<u64, ClockError> {
        if id != ClockId::Realtime && id != ClockId::Monotonic {
            return Err(ClockError::Unavailable);
        }
        if self.failed.get() {
            return Err(ClockError::Io);
        }
        Ok(self.time.get())
    }
    fn resolution(&self, id: ClockId) -> Result<u64, ClockError> {
        if id != ClockId::Realtime && id != ClockId::Monotonic {
            return Err(ClockError::Unavailable);
        }
        Ok(11)
    }
}

fn world(source: &str, time: u64) -> (World, Rc<TestClock>, u32) {
    let clock = Rc::new(TestClock {
        time: Cell::new(time),
        failed: Cell::new(false),
    });
    let mut world = World::new();
    world.set_clock(clock.clone());
    world
        .register("clock", &wat::parse_str(source).unwrap())
        .unwrap();
    let pid = world.spawn(vec!["clock".into()], vec![]).unwrap();
    (world, clock, pid)
}

#[test]
fn relative_deadline_is_saved_once_and_absolute_deadline_is_not_added_to_now() {
    let (mut world, clock, pid) = world(
        r#"(module
      (import "wasi_snapshot_preview1" "clock_time_get" (func $time (param i32 i64 i32) (result i32)))
      (import "wasi_snapshot_preview1" "clock_res_get" (func $res (param i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "poll_oneoff" (func $poll (param i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (func (export "_start")
        (if (call $res (i32.const 0) (i32.const 8)) (then unreachable))
        (if (i64.ne (i64.load (i32.const 8)) (i64.const 11)) (then unreachable))
        (if (call $time (i32.const 0) (i64.const 0) (i32.const 8)) (then unreachable))
        (if (i64.ne (i64.load (i32.const 8)) (i64.const 100)) (then unreachable))
        (i64.store (i32.const 128) (i64.const 73))
        (i64.store (i32.const 152) (i64.const 50))
        (if (call $poll (i32.const 128) (i32.const 256) (i32.const 1) (i32.const 0)) (then unreachable))
        (if (i32.ne (i32.load (i32.const 0)) (i32.const 1)) (then unreachable))
        (if (i64.ne (i64.load (i32.const 256)) (i64.const 73)) (then unreachable))
        (if (call $time (i32.const 0) (i64.const 0) (i32.const 8)) (then unreachable))
        (if (i64.ne (i64.load (i32.const 8)) (i64.const 150)) (then unreachable))
        (i64.store (i32.const 152) (i64.const 200))
        (i32.store16 (i32.const 168) (i32.const 1))
        (if (call $poll (i32.const 128) (i32.const 256) (i32.const 1) (i32.const 0)) (then unreachable))))"#,
        100,
    );
    assert_eq!(world.step(), Some(pid));
    assert_eq!(world.status(pid).unwrap(), &Status::Polling);
    clock.time.set(149);
    assert_eq!(world.step(), None);
    clock.time.set(90);
    assert_eq!(world.step(), None);
    clock.time.set(150);
    assert_eq!(world.step(), Some(pid));
    assert_eq!(world.status(pid).unwrap(), &Status::Polling);
    clock.time.set(199);
    assert_eq!(world.step(), None);
    clock.time.set(200);
    assert_eq!(world.step(), Some(pid));
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
}

#[test]
fn clock_failure_resumes_the_wait_with_an_error_without_writing_events() {
    let (mut world, clock, pid) = world(
        r#"(module
      (import "wasi_snapshot_preview1" "poll_oneoff" (func $poll (param i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (func (export "_start")
        (i32.store (i32.const 0) (i32.const 777))
        (i64.store (i32.const 152) (i64.const 5))
        (if (i32.ne (call $poll (i32.const 128) (i32.const 256) (i32.const 1) (i32.const 0)) (i32.const 29)) (then unreachable))
        (if (i32.ne (i32.load (i32.const 0)) (i32.const 777)) (then unreachable))))"#,
        100,
    );
    world.step();
    assert_eq!(world.status(pid).unwrap(), &Status::Polling);
    clock.failed.set(true);
    assert_eq!(world.step(), Some(pid));
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
}

#[test]
fn errors_cover_deadline_overflow_invalid_clock_and_output_boundary() {
    let (mut world, _, pid) = world(
        r#"(module
      (import "wasi_snapshot_preview1" "clock_time_get" (func $time (param i32 i64 i32) (result i32)))
      (import "wasi_snapshot_preview1" "poll_oneoff" (func $poll (param i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (func (export "_start")
        (i64.store (i32.const 152) (i64.const 10))
        (if (i32.ne (call $poll (i32.const 128) (i32.const 256) (i32.const 1) (i32.const 0)) (i32.const 61)) (then unreachable))
        (if (i32.ne (call $time (i32.const 4) (i64.const 0) (i32.const 0)) (i32.const 28)) (then unreachable))
        (if (i32.ne (call $time (i32.const 2) (i64.const 0) (i32.const 0)) (i32.const 58)) (then unreachable))
        (if (call $time (i32.const 1) (i64.const 0) (i32.const 65528)) (then unreachable))
        (if (i32.ne (call $time (i32.const 1) (i64.const 0) (i32.const 65529)) (i32.const 21)) (then unreachable))))"#,
        u64::MAX - 5,
    );
    world.step();
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
}
