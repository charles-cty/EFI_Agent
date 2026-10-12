use efi_agent_wasm::{
    Status, World,
    random::{Random, RandomError},
};
use std::{cell::Cell, rc::Rc};

struct Sequence {
    next: Cell<u8>,
    calls: Cell<usize>,
    failure: Option<RandomError>,
}

impl Random for Sequence {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), RandomError> {
        self.calls.set(self.calls.get() + 1);
        for byte in bytes {
            *byte = self.next.get();
            self.next.set(self.next.get().wrapping_add(1));
        }
        match self.failure {
            Some(failure) => Err(failure),
            None => Ok(()),
        }
    }
}

fn run(world: &mut World, source: &str) {
    world
        .register("random", &wat::parse_str(source).unwrap())
        .unwrap();
    let pid = world.spawn(vec!["random".into()], vec![]).unwrap();
    for _ in 0..100 {
        if !world.status(pid).unwrap().is_live() {
            break;
        }
        world.step();
    }
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
}

#[test]
fn boundary_ranges_are_checked_before_entropy_is_consumed() {
    let source = Rc::new(Sequence {
        next: Cell::new(73),
        calls: Cell::new(0),
        failure: None,
    });
    let mut world = World::new();
    world.set_random(source.clone());
    run(
        &mut world,
        r#"(module
      (import "wasi_snapshot_preview1" "random_get" (func $random (param i32 i32) (result i32)))
      (memory (export "memory") 9)
      (func (export "_start")
        (i32.store8 (i32.const 589822) (i32.const 165))
        (if (i32.ne (call $random (i32.const 589823) (i32.const 2)) (i32.const 21)) (then unreachable))
        (if (i32.ne (call $random (i32.const -1) (i32.const 2)) (i32.const 21)) (then unreachable))
        (if (i32.ne (call $random (i32.const 0) (i32.const 524289)) (i32.const 28)) (then unreachable))
        (if (call $random (i32.const 589824) (i32.const 0)) (then unreachable))
        (if (i32.ne (call $random (i32.const 589825) (i32.const 0)) (i32.const 21)) (then unreachable))
        (if (call $random (i32.const 589823) (i32.const 1)) (then unreachable))
        (if (i32.ne (i32.load8_u (i32.const 589823)) (i32.const 73)) (then unreachable))
        (if (i32.ne (i32.load8_u (i32.const 589822)) (i32.const 165)) (then unreachable))
        (if (call $random (i32.const 0) (i32.const 524288)) (then unreachable))
        (if (i32.ne (i32.load8_u (i32.const 0)) (i32.const 74)) (then unreachable))
        (if (i32.ne (i32.load8_u (i32.const 524287)) (i32.const 73)) (then unreachable))))"#,
    );
    assert_eq!(source.calls.get(), 2);
}

#[test]
fn unavailable_or_failed_source_does_not_publish_partial_bytes() {
    for failure in [None, Some(RandomError::Unavailable), Some(RandomError::Io)] {
        let mut world = World::new();
        let source = Rc::new(Sequence {
            next: Cell::new(9),
            calls: Cell::new(0),
            failure,
        });
        if failure.is_some() {
            world.set_random(source.clone());
        }
        let errno = if failure == Some(RandomError::Io) {
            29
        } else {
            58
        };
        run(
            &mut world,
            &format!(
                r#"(module
          (import "wasi_snapshot_preview1" "random_get" (func $random (param i32 i32) (result i32)))
          (memory (export "memory") 1)
          (func (export "_start")
            (i32.store (i32.const 32) (i32.const 0x12345678))
            (if (call $random (i32.const 65536) (i32.const 0)) (then unreachable))
            (if (i32.ne (call $random (i32.const 32) (i32.const 4)) (i32.const {errno})) (then unreachable))
            (if (i32.ne (i32.load (i32.const 32)) (i32.const 0x12345678)) (then unreachable))))"#
            ),
        );
        assert_eq!(source.calls.get(), usize::from(failure.is_some()));
    }
}

#[test]
fn multiple_instances_use_one_continuous_source_without_restarting_it() {
    let source = Rc::new(Sequence {
        next: Cell::new(19),
        calls: Cell::new(0),
        failure: None,
    });
    let mut world = World::new();
    world.set_random(source.clone());
    world.register("random", &wat::parse_str(r#"(module
      (import "wasi_snapshot_preview1" "random_get" (func $random (param i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (func (export "_start")
        (if (call $random (i32.const 32) (i32.const 37)) (then unreachable))
        (i32.store (i32.const 0) (i32.const 32))
        (i32.store (i32.const 4) (i32.const 37))
        (if (call $write (i32.const 1) (i32.const 0) (i32.const 1) (i32.const 8)) (then unreachable))))"#).unwrap()).unwrap();
    let pids: Vec<_> = (0..3)
        .map(|_| world.spawn(vec!["random".into()], vec![]).unwrap())
        .collect();
    for _ in 0..100 {
        if pids
            .iter()
            .all(|pid| !world.status(*pid).unwrap().is_live())
        {
            break;
        }
        world.step();
    }
    let mut outputs: Vec<_> = pids
        .iter()
        .map(|pid| {
            assert_eq!(world.status(*pid).unwrap(), &Status::Exited(0));
            let (stdout, stderr) = world.take_output(*pid).unwrap();
            assert!(stderr.is_empty());
            stdout
        })
        .collect();
    outputs.sort();
    let expected: Vec<Vec<u8>> = (0..3)
        .map(|block| (19 + 37 * block..19 + 37 * (block + 1)).collect())
        .collect();
    assert_eq!(outputs, expected);
    assert_eq!(source.calls.get(), 3);
}
