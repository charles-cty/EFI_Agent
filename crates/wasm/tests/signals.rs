use efi_agent_wasm::{Status, World};

fn register(world: &mut World, name: &str, body: &str) {
    let module = format!(
        r#"(module
        (import "efi_process" "kill" (func $kill (param i32 i32) (result i32)))
        (import "efi_process" "getpid" (func $pid (result i32)))
        (import "efi_process" "spawn" (func $spawn (param i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
        (memory (export "memory") 1)
        (data (i32.const 0) "loop\00")
        (func (export "_start") {body}))"#
    );
    world
        .register(name, &wat::parse_str(module).unwrap())
        .unwrap();
}

fn finish(world: &mut World, pid: u32) -> i32 {
    for _ in 0..100 {
        if let Status::Exited(code) = world.status(pid).unwrap() {
            return *code;
        }
        world.step().expect("Runnable process");
    }
    panic!("Process did not exit");
}

#[test]
fn termination_checks_errors_before_effects_and_preserves_other_roots() {
    let mut world = World::new();
    register(&mut world, "loop", "(loop $forever (br $forever))");
    let target = world.spawn(vec!["loop".into()], vec![]).unwrap();
    let independent = world.spawn(vec!["loop".into()], vec![]).unwrap();
    for (name, pid, signal, expected) in [
        ("exists", target as i32, 0, 0),
        ("missing", 99999, 0, 3),
        ("group", 0, 9, 95),
        ("unsupported", target as i32, 19, 95),
        ("invalid", target as i32, 65, 22),
    ] {
        register(
            &mut world,
            name,
            &format!(
                "(call $exit (i32.sub (i32.const 0) (call $kill (i32.const {pid}) (i32.const {signal}))))"
            ),
        );
        let caller = world.spawn(vec![name.into()], vec![]).unwrap();
        assert_eq!(finish(&mut world, caller), expected);
        assert!(world.status(target).unwrap().is_live());
    }
    register(
        &mut world,
        "terminate",
        &format!("(call $exit (call $kill (i32.const {target}) (i32.const 15)))"),
    );
    let caller = world.spawn(vec!["terminate".into()], vec![]).unwrap();
    assert_eq!(finish(&mut world, caller), 0);
    assert_eq!(world.status(target).unwrap(), &Status::Exited(143));
    assert!(world.status(independent).unwrap().is_live());
}

#[test]
fn terminating_a_parent_adopts_its_live_child_without_terminating_it() {
    let mut world = World::new();
    register(&mut world, "loop", "(loop $forever (br $forever))");
    register(
        &mut world,
        "parent",
        "(drop (call $spawn (i32.const 0) (i32.const 5))) (loop $forever (br $forever))",
    );
    let parent = world.spawn(vec!["parent".into()], vec![]).unwrap();
    for _ in 0..10 {
        world.step();
    }
    let child = world.process_ids().find(|&pid| pid != parent).unwrap();
    assert_eq!(world.parent(child).unwrap(), parent);
    register(
        &mut world,
        "terminate",
        &format!("(drop (call $kill (i32.const {parent}) (i32.const 9)))"),
    );
    let caller = world.spawn(vec!["terminate".into()], vec![]).unwrap();
    assert_eq!(finish(&mut world, caller), 0);
    assert_eq!(world.status(parent).unwrap(), &Status::Exited(137));
    assert!(world.status(child).unwrap().is_live());
    assert_eq!(world.parent(child).unwrap(), 0);
    world.reap(parent).unwrap();
    assert!(world.status(child).unwrap().is_live());
}

#[test]
fn a_process_can_terminate_itself() {
    let mut world = World::new();
    register(
        &mut world,
        "self",
        "(drop (call $kill (call $pid) (i32.const 9))) unreachable",
    );
    let pid = world.spawn(vec!["self".into()], vec![]).unwrap();
    assert_eq!(finish(&mut world, pid), 137);
}

#[test]
fn self_termination_wakes_and_reaps_from_a_waiting_parent() {
    let mut world = World::new();
    register(
        &mut world,
        "self",
        "(drop (call $kill (call $pid) (i32.const 2))) unreachable",
    );
    let parent = wat::parse_str(
        r#"(module
        (import "efi_process" "spawn" (func $spawn (param i32 i32) (result i32)))
        (import "efi_process" "waitpid" (func $wait (param i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
        (memory (export "memory") 1)
        (data (i32.const 0) "self\00")
        (func (export "_start")
            (drop (call $wait (call $spawn (i32.const 0) (i32.const 5)) (i32.const 16)))
            (call $exit (i32.load (i32.const 16)))))"#,
    )
    .unwrap();
    world.register("parent", &parent).unwrap();
    let pid = world.spawn(vec!["parent".into()], vec![]).unwrap();
    assert_eq!(finish(&mut world, pid), 130);
    assert_eq!(world.process_ids().count(), 1);
}
