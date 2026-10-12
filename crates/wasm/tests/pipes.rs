use efi_agent_wasm::{Status, World};

fn register(world: &mut World, name: &str, wat: &str) {
    world.register(name, &wat::parse_str(wat).unwrap()).unwrap();
}

fn finish(world: &mut World, pid: u32) {
    for _ in 0..5000 {
        if !world.status(pid).unwrap().is_live() {
            return;
        }
        assert!(
            world.step().is_some(),
            "Deadlocked process: {:?}",
            world.status(pid)
        );
    }
    panic!("Process failed to finish");
}

#[test]
fn pipeline_checks_every_byte_across_two_bounded_pipes() {
    let mut world = World::new();
    register(
        &mut world,
        "producer",
        include_str!("../../../scripts/fixtures/wasm/producer.wat"),
    );
    register(
        &mut world,
        "copy",
        include_str!("../../../scripts/fixtures/wasm/copy.wat"),
    );
    register(
        &mut world,
        "pipeline",
        include_str!("../../../scripts/fixtures/wasm/pipeline.wat"),
    );
    let pid = world.spawn(vec!["pipeline".into()], vec![]).unwrap();
    let mut blocked_reader = false;
    let mut blocked_writer = false;
    for _ in 0..5000 {
        for process in world.process_ids() {
            blocked_reader |= matches!(world.status(process).unwrap(), Status::Reading(_));
            blocked_writer |= matches!(world.status(process).unwrap(), Status::Writing(_));
        }
        if !world.status(pid).unwrap().is_live() {
            break;
        }
        assert!(world.step().is_some(), "Pipeline deadlocked");
    }
    assert!(blocked_reader, "Pipeline never exercised a suspended read");
    assert!(blocked_writer, "Pipeline never exercised a suspended write");
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
    assert_eq!(
        world.take_output(pid).unwrap(),
        (b"PIPELINE_BYTES_OK_196608".to_vec(), vec![])
    );
    assert_eq!(world.process_ids().collect::<Vec<_>>(), vec![pid]);
}

#[test]
fn pipe_payload_matches_host_oracle_with_asymmetric_read_sizes() {
    let expected: Vec<u8> = (0..3)
        .flat_map(|_| (0..65536).map(|index| (index % 251) as u8))
        .collect();
    for chunk in [257, 997, 8191] {
        let mut world = World::new();
        register(
            &mut world,
            "producer",
            include_str!("../../../scripts/fixtures/wasm/producer.wat"),
        );
        let copy = include_str!("../../../scripts/fixtures/wasm/copy.wat")
            .replace("(i32.const 997)", &format!("(i32.const {chunk})"));
        register(&mut world, "copy", &copy);
        // Capture the bytes at the final reader. The oracle above is derived
        // from the payload contract, without using the guest checker output.
        let parent = include_str!("../../../scripts/fixtures/wasm/pipeline.wat").replace("(br $drain)", r#"
            (i32.store (i32.const 20) (local.get $count))
            (if (call $write (i32.const 1) (i32.const 16) (i32.const 1) (i32.const 24)) (then unreachable))
            (br $drain)"#);
        register(&mut world, "pipeline", &parent);
        let pid = world.spawn(vec!["pipeline".into()], vec![]).unwrap();
        finish(&mut world, pid);
        assert_eq!(
            world.status(pid).unwrap(),
            &Status::Exited(0),
            "chunk {chunk}"
        );
        let output = world.take_output(pid).unwrap().0;
        assert_eq!(&output[..expected.len()], &expected, "chunk {chunk}");
        assert_eq!(&output[expected.len()..], b"PIPELINE_BYTES_OK_196608");
    }
}

#[test]
fn eof_waits_for_last_duplicate_writer_and_read_resumes_once() {
    let mut world = World::new();
    register(
        &mut world,
        "reader",
        r#"(module
      (import "wasi_snapshot_preview1" "fd_read" (func $read (param i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (global $calls (mut i32) (i32.const 0))
      (func (export "_start")
        (global.set $calls (i32.add (global.get $calls) (i32.const 1)))
        (i32.store (i32.const 0) (i32.const 128)) (i32.store (i32.const 4) (i32.const 1))
        (if (call $read (i32.const 0) (i32.const 0) (i32.const 1) (i32.const 8)) (then unreachable))
        (if (i32.load (i32.const 8)) (then unreachable))
        (if (i32.ne (global.get $calls) (i32.const 1)) (then unreachable))))"#,
    );
    register(
        &mut world,
        "parent",
        r#"(module
      (import "efi_process" "pipe" (func $pipe (param i32) (result i32)))
      (import "efi_process" "dup2" (func $dup (param i32 i32) (result i32)))
      (import "efi_process" "spawn_fds" (func $spawn (param i32 i32 i32 i32 i32) (result i32)))
      (import "efi_process" "waitpid" (func $wait (param i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_close" (func $close (param i32) (result i32)))
      (import "wasi_snapshot_preview1" "sched_yield" (func $yield (result i32)))
      (memory (export "memory") 1) (data (i32.const 64) "reader\00")
      (func (export "_start") (local $pid i32)
        (if (call $pipe (i32.const 0)) (then unreachable))
        (if (i32.ne (call $dup (i32.load (i32.const 4)) (i32.const 5)) (i32.const 5)) (then unreachable))
        (local.set $pid (call $spawn (i32.const 64) (i32.const 7) (i32.load (i32.const 0)) (i32.const 1) (i32.const 2)))
        (drop (call $close (i32.load (i32.const 0))))
        (drop (call $close (i32.load (i32.const 4))))
        (drop (call $yield))
        (drop (call $close (i32.const 5)))
        (if (i32.ne (call $wait (local.get $pid) (i32.const 8)) (local.get $pid)) (then unreachable))
        (if (i32.load (i32.const 8)) (then unreachable))))"#,
    );
    let parent = world.spawn(vec!["parent".into()], vec![]).unwrap();
    assert_eq!(world.step(), Some(parent));
    let child = world.process_ids().find(|pid| *pid != parent).unwrap();
    assert_eq!(world.step(), Some(child));
    assert_eq!(world.status(child).unwrap(), &Status::Reading(0));
    assert_eq!(world.step(), Some(parent));
    assert_eq!(world.status(child).unwrap(), &Status::Reading(0));
    finish(&mut world, parent);
    assert_eq!(world.status(parent).unwrap(), &Status::Exited(0));
}

#[test]
fn cancelled_last_reader_wakes_full_pipe_writer_with_epipe() {
    let mut world = World::new();
    register(
        &mut world,
        "writer",
        r#"(module
      (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
      (memory (export "memory") 2)
      (func (export "_start")
        (i32.store (i32.const 0) (i32.const 128))
        (i32.store (i32.const 4) (i32.const 65536))
        (if (call $write (i32.const 1) (i32.const 0) (i32.const 1) (i32.const 8)) (then unreachable))
        (if (i32.ne (i32.load (i32.const 8)) (i32.const 65536)) (then unreachable))
        (i32.store (i32.const 4) (i32.const 1))
        (call $exit (call $write (i32.const 1) (i32.const 0) (i32.const 1) (i32.const 8)))))"#,
    );
    register(
        &mut world,
        "parent",
        r#"(module
      (import "efi_process" "pipe" (func $pipe (param i32) (result i32)))
      (import "efi_process" "spawn_fds" (func $spawn (param i32 i32 i32 i32 i32) (result i32)))
      (import "efi_process" "waitpid" (func $wait (param i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_close" (func $close (param i32) (result i32)))
      (memory (export "memory") 1) (data (i32.const 64) "writer\00")
      (func (export "_start") (local $pid i32)
        (drop (call $pipe (i32.const 0)))
        (local.set $pid (call $spawn (i32.const 64) (i32.const 7) (i32.const 0) (i32.load (i32.const 4)) (i32.const 2)))
        (drop (call $close (i32.load (i32.const 4))))
        (drop (call $wait (local.get $pid) (i32.const 8)))))"#,
    );
    let parent = world.spawn(vec!["parent".into()], vec![]).unwrap();
    world.step();
    let child = world.process_ids().find(|pid| *pid != parent).unwrap();
    for _ in 0..20 {
        if world.status(child).unwrap() == &Status::Writing(1) {
            break;
        }
        world.step();
    }
    assert_eq!(world.status(child).unwrap(), &Status::Writing(1));
    world.cancel(parent).unwrap();
    finish(&mut world, child);
    assert_eq!(world.status(child).unwrap(), &Status::Exited(64));
    assert_eq!(world.status(parent).unwrap(), &Status::Exited(130));
}

#[test]
fn descriptor_allocation_and_invalid_arguments_have_no_partial_effects() {
    let mut world = World::new();
    register(
        &mut world,
        "descriptors",
        r#"(module
      (import "efi_process" "pipe" (func $pipe (param i32) (result i32)))
      (import "efi_process" "dup2" (func $dup (param i32 i32) (result i32)))
      (import "efi_process" "spawn_fds" (func $spawn (param i32 i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_close" (func $close (param i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_renumber" (func $move (param i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_read" (func $read (param i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_fdstat_get" (func $stat (param i32 i32) (result i32)))
      (memory (export "memory") 1) (data (i32.const 64) "descriptors\00")
      (func (export "_start") (local $fd i32)
        (if (i32.ne (call $pipe (i32.const 65532)) (i32.const -14)) (then unreachable))
        (if (call $pipe (i32.const 0)) (then unreachable))
        (if (i32.ne (i32.load (i32.const 0)) (i32.const 3)) (then unreachable))
        (if (i32.ne (i32.load (i32.const 4)) (i32.const 4)) (then unreachable))
        ;; A zero-byte read on an open empty pipe must return without blocking.
        (if (call $read (i32.const 3) (i32.const 0) (i32.const 0) (i32.const 8)) (then unreachable))
        (if (i32.load (i32.const 8)) (then unreachable))
        (if (call $write (i32.const 4) (i32.const 0) (i32.const 0) (i32.const 8)) (then unreachable))
        (if (i32.ne (call $read (i32.const 4) (i32.const 0) (i32.const 0) (i32.const 8)) (i32.const 8)) (then unreachable))
        (if (i32.ne (call $write (i32.const 3) (i32.const 0) (i32.const 0) (i32.const 8)) (i32.const 8)) (then unreachable))
        (if (call $stat (i32.const 3) (i32.const 16)) (then unreachable))
        (if (i64.ne (i64.load (i32.const 24)) (i64.const 136314882)) (then unreachable))
        (if (call $stat (i32.const 4) (i32.const 16)) (then unreachable))
        (if (i64.ne (i64.load (i32.const 24)) (i64.const 136314944)) (then unreachable))
        (if (i32.ne (call $dup (i32.const 4) (i32.const 4)) (i32.const 4)) (then unreachable))
        (if (call $move (i32.const 4) (i32.const 5)) (then unreachable))
        (if (i32.ne (call $close (i32.const 4)) (i32.const 8)) (then unreachable))
        (if (call $close (i32.const 5)) (then unreachable))
        (if (call $close (i32.const 3)) (then unreachable))
        (if (i32.ne (call $dup (i32.const 1) (i32.const 128)) (i32.const -9)) (then unreachable))
        (if (i32.ne (call $spawn (i32.const 64) (i32.const 12) (i32.const 0) (i32.const 127) (i32.const 2)) (i32.const -9)) (then unreachable))
        (local.set $fd (i32.const 3))
        (loop $fill
          (if (i32.ne (call $dup (i32.const 1) (local.get $fd)) (local.get $fd)) (then unreachable))
          (local.set $fd (i32.add (local.get $fd) (i32.const 1)))
          (br_if $fill (i32.lt_u (local.get $fd) (i32.const 128))))
        (if (i32.ne (call $pipe (i32.const 0)) (i32.const -24)) (then unreachable))
        (drop (call $close (i32.const 127)))
        (if (i32.ne (call $pipe (i32.const 0)) (i32.const -24)) (then unreachable))
        (drop (call $close (i32.const 126)))
        (if (call $pipe (i32.const 0)) (then unreachable))
        (if (i32.ne (i32.load (i32.const 0)) (i32.const 126)) (then unreachable))
        (if (i32.ne (i32.load (i32.const 4)) (i32.const 127)) (then unreachable))))"#,
    );
    let pid = world.spawn(vec!["descriptors".into()], vec![]).unwrap();
    finish(&mut world, pid);
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
    assert_eq!(world.process_ids().collect::<Vec<_>>(), vec![pid]);
}

#[test]
fn cancellation_closes_writers_and_allows_orphan_reader_to_finish() {
    let mut world = World::new();
    register(
        &mut world,
        "copy",
        include_str!("../../../scripts/fixtures/wasm/copy.wat"),
    );
    register(
        &mut world,
        "parent",
        r#"(module
      (import "efi_process" "pipe" (func $pipe (param i32) (result i32)))
      (import "efi_process" "spawn_fds" (func $spawn (param i32 i32 i32 i32 i32) (result i32)))
      (import "efi_process" "waitpid" (func $wait (param i32 i32) (result i32)))
      (memory (export "memory") 1) (data (i32.const 64) "copy\00")
      (func (export "_start") (local $pid i32)
        (drop (call $pipe (i32.const 0)))
        (local.set $pid (call $spawn (i32.const 64) (i32.const 5) (i32.load (i32.const 0)) (i32.const 1) (i32.const 2)))
        (drop (call $wait (local.get $pid) (i32.const 8)))))"#,
    );
    let parent = world.spawn(vec!["parent".into()], vec![]).unwrap();
    world.step();
    let child = world.process_ids().find(|pid| *pid != parent).unwrap();
    world.step();
    world.step();
    assert_eq!(world.status(child).unwrap(), &Status::Reading(0));
    assert_eq!(world.status(parent).unwrap(), &Status::Waiting(child));
    assert_eq!(world.step(), None);
    world.cancel(parent).unwrap();
    finish(&mut world, child);
    assert_eq!(world.status(child).unwrap(), &Status::Exited(0));
    assert!(world.take_output(child).unwrap().0.is_empty());
}
