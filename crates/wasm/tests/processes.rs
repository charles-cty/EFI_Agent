use efi_agent_wasm::{Error, Status, World};

fn register(world: &mut World, name: &str, source: &str) {
    world
        .register(name, &wat::parse_str(source).unwrap())
        .unwrap();
}

fn finish(world: &mut World, pid: u32) {
    for _ in 0..1000 {
        if matches!(
            world.status(pid).unwrap(),
            Status::Exited(_) | Status::Trapped(_)
        ) {
            return;
        }
        assert!(world.step().is_some(), "Blocked without runnable work");
    }
    panic!("Process did not finish");
}

#[test]
fn cooperative_calls_resume_once_and_instances_are_isolated() {
    let mut world = World::new();
    register(
        &mut world,
        "counter",
        r#"(module
      (import "wasi_snapshot_preview1" "sched_yield" (func $yield (result i32)))
      (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
      (memory (export "memory") 1)
      (global $counter (mut i32) (i32.const 0))
      (func (export "_start")
        (global.set $counter (i32.add (global.get $counter) (i32.const 3)))
        (drop (call $yield))
        (global.set $counter (i32.add (global.get $counter) (i32.const 7)))
        (drop (call $yield))
        (call $exit (global.get $counter))
        unreachable))"#,
    );
    let a = world.spawn(vec!["counter".into()], vec![]).unwrap();
    let b = world.spawn(vec!["counter".into()], vec![]).unwrap();
    assert_eq!(
        [world.step(), world.step(), world.step(), world.step()],
        [Some(a), Some(b), Some(a), Some(b)]
    );
    assert_eq!(world.status(a).unwrap(), &Status::Ready);
    finish(&mut world, b);
    assert_eq!(world.status(a).unwrap(), &Status::Exited(10));
    assert_eq!(world.status(b).unwrap(), &Status::Exited(10));
}

#[test]
fn runaway_instance_does_not_block_other_instances_and_can_be_cancelled() {
    let mut world = World::new();
    register(
        &mut world,
        "loop",
        r#"(module (memory (export "memory") 1)
      (func (export "_start") (loop $forever (br $forever))))"#,
    );
    register(
        &mut world,
        "exit",
        r#"(module (memory (export "memory") 1) (func (export "_start")))"#,
    );
    let runaway = world.spawn(vec!["loop".into()], vec![]).unwrap();
    let other = world.spawn(vec!["exit".into()], vec![]).unwrap();
    assert_eq!(world.step(), Some(runaway));
    assert_eq!(world.step(), Some(other));
    assert_eq!(world.status(other).unwrap(), &Status::Exited(0));
    world.cancel(runaway).unwrap();
    assert_eq!(world.status(runaway).unwrap(), &Status::Exited(130));
    assert_eq!(world.step(), None);
}

#[test]
fn spawn_wait_exit_status_and_inherited_output() {
    let mut world = World::new();
    register(
        &mut world,
        "child",
        r#"(module
      (import "wasi_snapshot_preview1" "sched_yield" (func $yield (result i32)))
      (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
      (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
      (import "efi_process" "getppid" (func $ppid (result i32)))
      (memory (export "memory") 1)
      (data (i32.const 32) "child-output")
      (func (export "_start")
        (drop (call $yield))
        (drop (call $yield))
        (i32.store (i32.const 0) (i32.const 32))
        (i32.store (i32.const 4) (i32.const 12))
        (drop (call $write (i32.const 1) (i32.const 0) (i32.const 1) (i32.const 8)))
        (call $exit (i32.add (call $ppid) (i32.const 36)))))"#,
    );
    register(
        &mut world,
        "parent",
        r#"(module
      (import "efi_process" "spawn" (func $spawn (param i32 i32) (result i32)))
      (import "efi_process" "waitpid" (func $wait (param i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
      (memory (export "memory") 1)
      (data (i32.const 32) "child\00argument\00")
      (func (export "_start") (local $pid i32)
        (local.set $pid (call $spawn (i32.const 32) (i32.const 15)))
        (if (i32.ne (call $wait (local.get $pid) (i32.const 0)) (local.get $pid)) (then unreachable))
        (call $exit (i32.load (i32.const 0)))))"#,
    );
    let parent = world.spawn(vec!["parent".into()], vec![]).unwrap();
    assert_eq!(world.step(), Some(parent));
    let child = world.process_ids().find(|pid| *pid != parent).unwrap();
    assert_eq!(world.root(child).unwrap(), parent);
    assert_eq!(world.step(), Some(child));
    assert_eq!(world.step(), Some(parent));
    assert_eq!(world.status(parent).unwrap(), &Status::Waiting(child));
    finish(&mut world, parent);
    assert_eq!(world.status(parent).unwrap(), &Status::Exited(37));
    assert_eq!(
        world.take_output(parent).unwrap(),
        (b"child-output".to_vec(), vec![])
    );
    assert_eq!(world.process_ids().collect::<Vec<_>>(), vec![parent]);
}

#[test]
fn exited_parent_can_be_reaped_before_live_child_reports_adoption_and_output() {
    let mut world = World::new();
    register(
        &mut world,
        "orphan",
        r#"(module
          (import "efi_process" "getppid" (func $ppid (result i32)))
          (import "wasi_snapshot_preview1" "sched_yield" (func $yield (result i32)))
          (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
          (memory (export "memory") 1)
          (data (i32.const 32) "after-parent")
          (func (export "_start")
            (if (i32.ne (call $ppid) (i32.const 1)) (then unreachable))
            (drop (call $yield))
            (if (call $ppid) (then unreachable))
            (i32.store (i32.const 0) (i32.const 32))
            (i32.store (i32.const 4) (i32.const 12))
            (drop (call $write (i32.const 1) (i32.const 0) (i32.const 1) (i32.const 8)))))"#,
    );
    register(
        &mut world,
        "parent",
        r#"(module
          (import "efi_process" "spawn" (func $spawn (param i32 i32) (result i32)))
          (memory (export "memory") 1)
          (data (i32.const 32) "orphan\00")
          (func (export "_start") (drop (call $spawn (i32.const 32) (i32.const 7)))))"#,
    );
    let parent = world.spawn(vec!["parent".into()], vec![]).unwrap();
    assert_eq!(world.step(), Some(parent));
    let child = world.process_ids().find(|pid| *pid != parent).unwrap();
    assert_eq!(world.step(), Some(child));
    assert_eq!(world.step(), Some(parent));
    assert_eq!(world.reap(parent).unwrap(), Status::Exited(0));
    assert_eq!(world.root(child).unwrap(), parent);
    finish(&mut world, child);
    assert_eq!(world.status(child).unwrap(), &Status::Exited(0));
    assert_eq!(world.take_output(child).unwrap().0, b"after-parent");
    assert_eq!(world.reap(child).unwrap(), Status::Exited(0));
}

#[test]
fn bad_iovec_has_no_partial_output_and_wasm_trap_is_local() {
    let mut world = World::new();
    register(
        &mut world,
        "bad-vector",
        r#"(module
      (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
      (memory (export "memory") 1)
      (data (i32.const 32) "valid")
      (func (export "_start")
        (i32.store (i32.const 0) (i32.const 32)) (i32.store (i32.const 4) (i32.const 5))
        (i32.store (i32.const 8) (i32.const 65535)) (i32.store (i32.const 12) (i32.const 2))
        (call $exit (call $write (i32.const 1) (i32.const 0) (i32.const 2) (i32.const 16)))))"#,
    );
    register(
        &mut world,
        "trap",
        r#"(module (memory (export "memory") 1) (func (export "_start") unreachable))"#,
    );
    let bad = world.spawn(vec!["bad-vector".into()], vec![]).unwrap();
    let trap = world.spawn(vec!["trap".into()], vec![]).unwrap();
    finish(&mut world, trap);
    assert_eq!(world.status(bad).unwrap(), &Status::Exited(21));
    assert!(world.take_output(bad).unwrap().0.is_empty());
    assert!(matches!(world.status(trap).unwrap(), Status::Trapped(_)));
}

#[test]
fn wasi_arguments_include_empty_and_unicode_strings() {
    let mut world = World::new();
    register(
        &mut world,
        "args",
        r#"(module
      (import "wasi_snapshot_preview1" "args_sizes_get" (func $sizes (param i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "args_get" (func $args (param i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (func (export "_start")
        (drop (call $sizes (i32.const 0) (i32.const 4)))
        (if (i32.ne (i32.load (i32.const 0)) (i32.const 3)) (then unreachable))
        (drop (call $args (i32.const 32) (i32.const 64)))
        (i32.store (i32.const 8) (i32.const 64))
        (i32.store (i32.const 12) (i32.load (i32.const 4)))
        (drop (call $write (i32.const 1) (i32.const 8) (i32.const 1) (i32.const 16)))))"#,
    );
    let pid = world
        .spawn(vec!["args".into(), "".into(), "λ".into()], vec![])
        .unwrap();
    finish(&mut world, pid);
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
    assert_eq!(world.take_output(pid).unwrap().0, "args\0\0λ\0".as_bytes());
}

#[test]
fn reject_unscheduled_start_and_excess_memory_without_consuming_pid() {
    let mut world = World::new();
    let start = wat::parse_str(r#"(module (func $start) (start $start))"#).unwrap();
    assert!(world.register("start", &start).is_err());
    register(
        &mut world,
        "large",
        r#"(module (memory (export "memory") 1025) (func (export "_start")))"#,
    );
    assert!(world.spawn(vec!["large".into()], vec![]).is_err());
    assert!(matches!(
        world.spawn(vec!["missing".into()], vec![]),
        Err(Error::UnknownProgram(_))
    ));
    register(
        &mut world,
        "ok",
        r#"(module (memory (export "memory") 1) (func (export "_start")))"#,
    );
    assert_eq!(world.spawn(vec!["ok".into()], vec![]).unwrap(), 1);
    register(
        &mut world,
        "allocation",
        r#"(module
      (import "efi_process" "spawn" (func $spawn (param i32 i32) (result i32)))
      (memory (export "memory") 1)
      (data (i32.const 32) "large\00")
      (func (export "_start")
        (if (i32.ne (call $spawn (i32.const 32) (i32.const 6)) (i32.const -12)) (then unreachable))))"#,
    );
    let parent = world.spawn(vec!["allocation".into()], vec![]).unwrap();
    finish(&mut world, parent);
    assert_eq!(world.status(parent).unwrap(), &Status::Exited(0));
    assert_eq!(world.process_ids().count(), 2);
}

#[test]
fn iovec_at_memory_end_accepts_one_byte_and_rejects_two() {
    for (length, expected_status, expected_output) in [(1, 0, vec![b'X']), (2, 21, vec![])] {
        let mut world = World::new();
        register(
            &mut world,
            "edge",
            &format!(
                r#"(module
          (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
          (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
          (memory (export "memory") 1)
          (data (i32.const 65535) "X")
          (func (export "_start")
            (i32.store (i32.const 0) (i32.const 65535))
            (i32.store (i32.const 4) (i32.const {length}))
            (call $exit (call $write (i32.const 1) (i32.const 0) (i32.const 1) (i32.const 8)))))"#
            ),
        );
        let pid = world.spawn(vec!["edge".into()], vec![]).unwrap();
        finish(&mut world, pid);
        assert_eq!(world.status(pid).unwrap(), &Status::Exited(expected_status));
        assert_eq!(world.take_output(pid).unwrap().0, expected_output);
    }
}

#[test]
fn exited_processes_use_slots_until_reaped_and_reap_does_not_reuse_pid() {
    let mut world = World::new();
    register(
        &mut world,
        "ok",
        r#"(module (memory (export "memory") 1) (func (export "_start")))"#,
    );
    for _ in 0..efi_agent_wasm::MAX_PROCESSES {
        let pid = world.spawn(vec!["ok".into()], vec![]).unwrap();
        finish(&mut world, pid);
    }
    assert!(matches!(
        world.spawn(vec!["ok".into()], vec![]),
        Err(Error::ProcessLimit)
    ));
    assert_eq!(world.reap(1).unwrap(), Status::Exited(0));
    let pid = world.spawn(vec!["ok".into()], vec![]).unwrap();
    assert_eq!(pid, 33);
    assert!(matches!(world.reap(pid), Err(Error::NotExited(33))));
    world.cancel(pid).unwrap();
    assert_eq!(world.reap(pid).unwrap(), Status::Exited(130));
}

#[test]
fn memory_growth_reaches_limit_and_then_returns_failure() {
    let mut world = World::new();
    register(
        &mut world,
        "grow",
        r#"(module
      (memory (export "memory") 1)
      (func (export "_start")
        (if (i32.ne (memory.grow (i32.const 1023)) (i32.const 1)) (then unreachable))
        (if (i32.ne (memory.grow (i32.const 1)) (i32.const -1)) (then unreachable))))"#,
    );
    let pid = world.spawn(vec!["grow".into()], vec![]).unwrap();
    finish(&mut world, pid);
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
}

#[test]
fn nonblocking_wait_preserves_status_until_exit_and_reaps_once() {
    let mut world = World::new();
    register(
        &mut world,
        "child",
        r#"(module
      (import "wasi_snapshot_preview1" "sched_yield" (func $yield (result i32)))
      (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
      (memory (export "memory") 1)
      (func (export "_start") (drop (call $yield)) (drop (call $yield))
        (drop (call $yield)) (call $exit (i32.const 47))))"#,
    );
    register(
        &mut world,
        "parent",
        r#"(module
      (import "efi_process" "spawn" (func $spawn (param i32 i32) (result i32)))
      (import "efi_process" "waitpid_nohang" (func $wait (param i32 i32) (result i32)))
      (memory (export "memory") 1)
      (data (i32.const 32) "child\00")
      (func (export "_start") (local $pid i32) (local $result i32)
        (local.set $pid (call $spawn (i32.const 32) (i32.const 6)))
        (i32.store (i32.const 0) (i32.const 123456))
        (if (i32.ne (call $wait (local.get $pid) (i32.const 65535)) (i32.const -14))
          (then unreachable))
        (if (call $wait (local.get $pid) (i32.const 0)) (then unreachable))
        (if (i32.ne (i32.load (i32.const 0)) (i32.const 123456)) (then unreachable))
        (loop $retry
          (local.set $result (call $wait (local.get $pid) (i32.const 0)))
          (br_if $retry (i32.eqz (local.get $result))))
        (if (i32.ne (local.get $result) (local.get $pid)) (then unreachable))
        (if (i32.ne (i32.load (i32.const 0)) (i32.const 47)) (then unreachable))
        (if (i32.ne (call $wait (local.get $pid) (i32.const 0)) (i32.const -10))
          (then unreachable))))"#,
    );
    let parent = world.spawn(vec!["parent".into()], vec![]).unwrap();
    finish(&mut world, parent);
    assert_eq!(world.status(parent).unwrap(), &Status::Exited(0));
    assert_eq!(world.process_ids().collect::<Vec<_>>(), vec![parent]);
}

#[test]
fn large_callee_keeps_a_continuation_when_a_quantum_expires() {
    let mut world = World::new();
    let operations = "(global.set $sum (i32.add (global.get $sum) (i32.const 1)))".repeat(20_003);
    register(
        &mut world,
        "large",
        &format!(
            r#"(module
      (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
      (memory (export "memory") 1)
      (global $sum (mut i32) (i32.const 0))
      (func $large {operations})
      (func (export "_start") (call $large) (call $exit (global.get $sum))))"#
        ),
    );
    register(
        &mut world,
        "peer",
        r#"(module
      (memory (export "memory") 1) (func (export "_start")))"#,
    );
    let large = world.spawn(vec!["large".into()], vec![]).unwrap();
    let peer = world.spawn(vec!["peer".into()], vec![]).unwrap();
    assert_eq!(world.step(), Some(large));
    assert_eq!(world.status(large).unwrap(), &Status::Ready);
    assert_eq!(world.step(), Some(peer));
    assert_eq!(world.status(peer).unwrap(), &Status::Exited(0));
    finish(&mut world, large);
    assert_eq!(world.status(large).unwrap(), &Status::Exited(20_003));
}

#[test]
fn stream_seek_reports_spipe_and_never_changes_the_result() {
    let mut world = World::new();
    register(
        &mut world,
        "seek",
        r#"(module
      (import "wasi_snapshot_preview1" "fd_seek" (func $seek (param i32 i64 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (func (export "_start")
        (i64.store (i32.const 0) (i64.const 987654321))
        (if (i32.ne (call $seek (i32.const 1) (i64.const 0) (i32.const 0) (i32.const 0)) (i32.const 70)) (then unreachable))
        (if (i64.ne (i64.load (i32.const 0)) (i64.const 987654321)) (then unreachable))
        (if (i32.ne (call $seek (i32.const 127) (i64.const 0) (i32.const 0) (i32.const 0)) (i32.const 8)) (then unreachable))
        (if (i32.ne (call $seek (i32.const 1) (i64.const 0) (i32.const 3) (i32.const 0)) (i32.const 28)) (then unreachable))
        (if (i32.ne (call $seek (i32.const 1) (i64.const 0) (i32.const 0) (i32.const 65528)) (i32.const 70)) (then unreachable))
        (if (i32.ne (call $seek (i32.const 1) (i64.const 0) (i32.const 0) (i32.const 65529)) (i32.const 21)) (then unreachable))))"#,
    );
    let pid = world.spawn(vec!["seek".into()], vec![]).unwrap();
    finish(&mut world, pid);
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
}
