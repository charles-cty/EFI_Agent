use efi_agent_wasm::{Error, Status, World};

fn register(world: &mut World, name: &str, text: &str) {
    world
        .register(name, &wat::parse_str(text).unwrap())
        .unwrap();
}

fn finish(world: &mut World, pid: u32) {
    for _ in 0..1000 {
        if !world.status(pid).unwrap().is_live() {
            return;
        }
        assert!(world.step().is_some());
    }
    panic!("Environment fixture did not complete");
}

const DUMP: &str = r#"(module
  (import "wasi_snapshot_preview1" "environ_sizes_get" (func $sizes (param i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "environ_get" (func $get (param i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 2)
  (func (export "_start")
    (if (call $sizes (i32.const 0) (i32.const 4)) (then unreachable))
    (if (call $get (i32.const 64) (i32.const 16384)) (then unreachable))
    (i32.store (i32.const 8) (i32.const 16384))
    (i32.store (i32.const 12) (i32.load (i32.const 4)))
    (if (call $write (i32.const 1) (i32.const 8) (i32.const 1) (i32.const 16)) (then unreachable))))"#;

#[test]
fn environment_validation_and_joint_limit_do_not_consume_pids() {
    let mut world = World::new();
    register(&mut world, "dump", DUMP);
    for environment in [
        vec!["=v".into()],
        vec!["missing".into()],
        vec!["A=1".into(), "A=2".into()],
        vec!["A=x\0y".into()],
    ] {
        assert!(matches!(
            world.spawn_with_environment(vec!["dump".into()], environment, vec![]),
            Err(Error::InvalidArguments)
        ));
    }
    for size in [65535, 65536, 65537] {
        // argv takes five bytes; the remaining entry is KEY=value plus NUL.
        let environment = vec![format!("K={}", "x".repeat(size - 8))];
        let result = world.spawn_with_environment(vec!["dump".into()], environment, vec![]);
        if size <= 65536 {
            let pid = result.unwrap();
            assert_eq!(pid, (size - 65534) as u32);
            finish(&mut world, pid);
            assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
        } else {
            assert!(matches!(result, Err(Error::ArgumentsTooLarge)));
        }
    }
    assert!(matches!(
        world.spawn_with_environment(
            vec!["dump".into()],
            vec![format!("K={}", "x".repeat(65529))],
            vec![]
        ),
        Err(Error::ArgumentsTooLarge)
    ));
    assert_eq!(world.spawn(vec!["dump".into()], vec![]).unwrap(), 3);
}

#[test]
fn structured_environments_match_independent_serialization() {
    let mut state = 17u32;
    for count in [0, 1, 3, 11, 31] {
        let mut environment = Vec::new();
        let mut expected = Vec::new();
        for index in 0..count {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let value = match state % 4 {
                0 => "".to_owned(),
                1 => "λ=值".to_owned(),
                2 => "x y\n".to_owned(),
                _ => state.to_string(),
            };
            let entry = format!("KEY{index}={value}");
            expected.extend_from_slice(entry.as_bytes());
            expected.push(0);
            environment.push(entry);
        }
        let mut world = World::new();
        register(&mut world, "dump", DUMP);
        let pid = world
            .spawn_with_environment(vec!["dump".into()], environment, vec![])
            .unwrap();
        finish(&mut world, pid);
        assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
        assert_eq!(world.take_output(pid).unwrap().0, expected);
    }
}

#[test]
fn failed_environment_copy_does_not_touch_output() {
    let mut world = World::new();
    register(
        &mut world,
        "edge",
        r#"(module
      (import "wasi_snapshot_preview1" "environ_get" (func $get (param i32 i32) (result i32)))
      (memory (export "memory") 1)
      (func (export "_start")
        (i32.store (i32.const 0) (i32.const 12345))
        (if (i32.ne (call $get (i32.const 0) (i32.const 65533)) (i32.const 21)) (then unreachable))
        (if (i32.ne (i32.load (i32.const 0)) (i32.const 12345)) (then unreachable))
        (if (call $get (i32.const 65532) (i32.const 65532)) (then unreachable))
        (if (i32.ne (i32.load (i32.const 65532)) (i32.const 4013377)) (then unreachable))))"#,
    );
    let pid = world
        .spawn_with_environment(vec!["edge".into()], vec!["A==".into()], vec![])
        .unwrap();
    finish(&mut world, pid);
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
}

#[test]
fn raw_spawn_rejects_bad_memory_and_entries_before_allocating_a_child() {
    let mut world = World::new();
    register(&mut world, "dump", DUMP);
    register(
        &mut world,
        "parent",
        r#"(module
      (import "efi_process" "spawn_env_fds" (func $spawn (param i32 i32 i32 i32 i32 i32 i32) (result i32)))
      (import "efi_process" "waitpid" (func $wait (param i32 i32) (result i32)))
      (memory (export "memory") 1)
      (data (i32.const 64) "dump\00")
      (data (i32.const 96) "A=1\00A=2\00")
      (data (i32.const 112) "A=\ff\00")
      (data (i32.const 128) "A=1X")
      (func (export "_start") (local $pid i32)
        (if (i32.ne (call $spawn (i32.const 64) (i32.const 5) (i32.const 96) (i32.const 8) (i32.const 0) (i32.const 1) (i32.const 2)) (i32.const -22)) (then unreachable))
        (if (i32.ne (call $spawn (i32.const 64) (i32.const 5) (i32.const 112) (i32.const 4) (i32.const 0) (i32.const 1) (i32.const 2)) (i32.const -22)) (then unreachable))
        (if (i32.ne (call $spawn (i32.const 64) (i32.const 5) (i32.const 128) (i32.const 4) (i32.const 0) (i32.const 1) (i32.const 2)) (i32.const -22)) (then unreachable))
        (if (i32.ne (call $spawn (i32.const 64) (i32.const 5) (i32.const 65535) (i32.const 4) (i32.const 0) (i32.const 1) (i32.const 2)) (i32.const -14)) (then unreachable))
        (if (i32.ne (call $spawn (i32.const 64) (i32.const 5) (i32.const 96) (i32.const 65532) (i32.const 0) (i32.const 1) (i32.const 2)) (i32.const -7)) (then unreachable))
        (if (i32.ne (call $spawn (i32.const 64) (i32.const 5) (i32.const 96) (i32.const 4) (i32.const 127) (i32.const 1) (i32.const 2)) (i32.const -9)) (then unreachable))
        (local.set $pid (call $spawn (i32.const 64) (i32.const 5) (i32.const 96) (i32.const 4) (i32.const 0) (i32.const 1) (i32.const 2)))
        (if (i32.ne (local.get $pid) (i32.const 2)) (then unreachable))
        (if (i32.ne (call $wait (local.get $pid) (i32.const 0)) (local.get $pid)) (then unreachable))))"#,
    );
    let pid = world.spawn(vec!["parent".into()], vec![]).unwrap();
    finish(&mut world, pid);
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
    assert_eq!(world.take_output(pid).unwrap().0, b"A=1\0");
    assert_eq!(world.process_ids().collect::<Vec<_>>(), vec![pid]);
}

#[test]
fn inherited_environment_counts_toward_child_argument_limit() {
    let mut world = World::new();
    register(&mut world, "dump", DUMP);
    register(
        &mut world,
        "parent",
        r#"(module
      (import "efi_process" "spawn" (func $spawn (param i32 i32) (result i32)))
      (import "efi_process" "waitpid" (func $wait (param i32 i32) (result i32)))
      (memory (export "memory") 1)
      (data (i32.const 32) "dump\00longer\00")
      (func (export "_start") (local $pid i32)
        (if (i32.ne (call $spawn (i32.const 32) (i32.const 12)) (i32.const -7)) (then unreachable))
        (local.set $pid (call $spawn (i32.const 32) (i32.const 5)))
        (if (i32.ne (local.get $pid) (i32.const 2)) (then unreachable))
        (if (i32.ne (call $wait (local.get $pid) (i32.const 0)) (local.get $pid)) (then unreachable))))"#,
    );
    let value = format!("K={}", "x".repeat(65525));
    let pid = world
        .spawn_with_environment(vec!["parent".into()], vec![value.clone()], vec![])
        .unwrap();
    finish(&mut world, pid);
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
    let mut expected = value.into_bytes();
    expected.push(0);
    assert_eq!(world.take_output(pid).unwrap().0, expected);
}
