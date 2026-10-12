use efi_agent_wasm::{Status, World};

#[test]
fn registry_lookup_is_live_and_has_no_spawn_side_effects() {
    let module = wat::parse_str(
        r#"(module
        (import "efi_process" "program_exists" (func $exists (param i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "sched_yield" (func $yield (result i32)))
        (memory (export "memory") 1)
        (data (i32.const 0) "late")
        (data (i32.const 16) "bad\00name\ff")
        (data (i32.const 65532) "late")
        (func $equal (param $actual i32) (param $expected i32)
            (if (i32.ne (local.get $actual) (local.get $expected)) (then unreachable)))
        (func (export "_start")
            (call $equal (call $exists (i32.const 0) (i32.const 4)) (i32.const 0))
            (call $equal (call $exists (i32.const 65532) (i32.const 5)) (i32.const -14))
            (call $equal (call $exists (i32.const 0) (i32.const 0)) (i32.const -22))
            (call $equal (call $exists (i32.const 0) (i32.const 65537)) (i32.const -7))
            (call $equal (call $exists (i32.const 16) (i32.const 8)) (i32.const -22))
            (call $equal (call $exists (i32.const 24) (i32.const 1)) (i32.const -22))
            (drop (call $yield))
            (call $equal (call $exists (i32.const 65532) (i32.const 4)) (i32.const 1))))"#,
    )
    .unwrap();
    let mut world = World::new();
    world.register("lookup", &module).unwrap();
    let pid = world.spawn(vec!["lookup".into()], vec![]).unwrap();
    // First lookup suspends to consult the world's registry. No PID is created.
    assert_eq!(world.step(), Some(pid));
    assert_eq!(world.process_ids().collect::<Vec<_>>(), vec![pid]);
    // Resume the absent result, validate bad inputs and reach sched_yield.
    assert_eq!(world.step(), Some(pid));
    let late = wat::parse_str("(module (memory (export \"memory\") 1) (func (export \"_start\")))")
        .unwrap();
    world.register("late", &late).unwrap();
    for _ in 0..100 {
        if !world.status(pid).unwrap().is_live() {
            break;
        }
        world.step();
    }
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
    assert_eq!(world.spawn(vec!["late".into()], vec![]).unwrap(), pid + 1);
}
