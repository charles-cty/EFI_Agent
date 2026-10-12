use efi_agent_wasm::{Status, World};

#[test]
fn deep_guest_calls_work_and_unbounded_recursion_traps_locally() {
    let mut world = World::new();
    let finite = wat::parse_str(
        r#"(module
        (memory (export "memory") 1)
        (func $count (param $n i32) (result i32)
            (if (result i32) (local.get $n)
                (then (i32.add (i32.const 1) (call $count (i32.sub (local.get $n) (i32.const 1)))))
                (else (i32.const 0))))
        (func (export "_start")
            (if (i32.ne (call $count (i32.const 1200)) (i32.const 1200)) (then unreachable))))"#,
    )
    .unwrap();
    let infinite = wat::parse_str(
        r#"(module (memory (export "memory") 1)
        (func $recurse (call $recurse))
        (func (export "_start") (call $recurse)))"#,
    )
    .unwrap();
    world.register("finite", &finite).unwrap();
    world.register("infinite", &infinite).unwrap();
    let runaway = world.spawn(vec!["infinite".into()], vec![]).unwrap();
    let successful = world.spawn(vec!["finite".into()], vec![]).unwrap();
    for _ in 0..1000 {
        if !world.status(runaway).unwrap().is_live() && !world.status(successful).unwrap().is_live()
        {
            break;
        }
        world.step();
    }
    assert_eq!(world.status(successful).unwrap(), &Status::Exited(0));
    assert!(
        matches!(world.status(runaway).unwrap(), Status::Trapped(error) if error.contains("call stack exhausted"))
    );
}
