use efi_agent_wasm::{Status, World};

fn register(world: &mut World, name: &str, source: &str) {
    world
        .register(name, &wat::parse_str(source).unwrap())
        .unwrap();
}

#[test]
fn poll_suspends_and_preserves_payload_userdata_and_event_order() {
    let mut world = World::new();
    register(
        &mut world,
        "child",
        include_str!("../../../scripts/fixtures/wasm/child.wat"),
    );
    register(
        &mut world,
        "poll",
        include_str!("../../../scripts/fixtures/wasm/poll.wat"),
    );
    let pid = world.spawn(vec!["poll".into()], vec![]).unwrap();
    let mut suspended = false;
    for _ in 0..100 {
        suspended |= world.status(pid).unwrap() == &Status::Polling;
        if !world.status(pid).unwrap().is_live() {
            break;
        }
        assert!(world.step().is_some());
    }
    assert!(suspended);
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
    assert_eq!(world.take_output(pid).unwrap().0, b"POLL_READY_OK");
    assert_eq!(world.process_ids().collect::<Vec<_>>(), vec![pid]);
}

#[test]
fn immediate_timeout_reports_only_ready_events_and_validates_before_writing() {
    let mut world = World::new();
    register(
        &mut world,
        "zero",
        r#"(module
      (import "efi_process" "pipe" (func $pipe (param i32) (result i32)))
      (import "wasi_snapshot_preview1" "poll_oneoff" (func $poll (param i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (func (export "_start")
        (drop (call $pipe (i32.const 0)))
        (i64.store (i32.const 128) (i64.const 11))
        (i32.store8 (i32.const 136) (i32.const 1))
        (i32.store (i32.const 144) (i32.load (i32.const 0)))
        (i64.store (i32.const 176) (i64.const 77))
        (if (call $poll (i32.const 128) (i32.const 256) (i32.const 2) (i32.const 8)) (then unreachable))
        (if (i32.ne (i32.load (i32.const 8)) (i32.const 1)) (then unreachable))
        (if (i64.ne (i64.load (i32.const 256)) (i64.const 77)) (then unreachable))
        (if (i32.load8_u (i32.const 266)) (then unreachable))
        (i32.store (i32.const 8) (i32.const 12345))
        (if (i32.ne (call $poll (i32.const 176) (i32.const 65520) (i32.const 1) (i32.const 8)) (i32.const 21)) (then unreachable))
        (if (i32.ne (i32.load (i32.const 8)) (i32.const 12345)) (then unreachable))
        (if (i32.ne (call $poll (i32.const 176) (i32.const 256) (i32.const 0) (i32.const 8)) (i32.const 28)) (then unreachable))
        (i64.store (i32.const 200) (i64.const 1))
        (if (i32.ne (call $poll (i32.const 176) (i32.const 256) (i32.const 1) (i32.const 8)) (i32.const 58)) (then unreachable))))"#,
    );
    let pid = world.spawn(vec!["zero".into()], vec![]).unwrap();
    world.step();
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
}
