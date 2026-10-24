use efi_agent_wasm::{Status, World};

#[test]
fn child_cancellation_preserves_other_roots_and_reaps_descendants_before_wait() {
    let mut world = World::new();
    for (name, source) in [
        (
            "leaf",
            r#"(module
            (import "wasi_snapshot_preview1" "sched_yield" (func $yield (result i32)))
            (memory (export "memory") 1)
            (func (export "_start") (loop $again (drop (call $yield)) (br $again))))"#,
        ),
        (
            "branch",
            r#"(module
            (import "efi_process" "waitpid" (func $wait (param i32 i32) (result i32)))
            (import "efi_process" "spawn" (func $spawn (param i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "sched_yield" (func $yield (result i32)))
            (memory (export "memory") 1) (data (i32.const 64) "leaf\00")
            (func (export "_start")
                (drop (call $wait (call $spawn (i32.const 64) (i32.const 5)) (i32.const 0)))
                (loop $again (drop (call $yield)) (br $again))))"#,
        ),
        (
            "parent",
            r#"(module
            (import "efi_process" "spawn" (func $spawn (param i32 i32) (result i32)))
            (import "efi_process" "cancel_child" (func $cancel (param i32) (result i32)))
            (import "efi_process" "waitpid" (func $wait (param i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "sched_yield" (func $yield (result i32)))
            (memory (export "memory") 1) (data (i32.const 64) "branch\00")
            (func (export "_start") (local $child i32) (local $count i32)
                (if (i32.ne (call $cancel (i32.const 1)) (i32.const -10)) (then unreachable))
                (if (i32.ne (call $cancel (i32.const 2)) (i32.const -10)) (then unreachable))
                (if (i32.ne (call $cancel (i32.const 9999)) (i32.const -10)) (then unreachable))
                (local.set $child (call $spawn (i32.const 64) (i32.const 7)))
                (loop $allow_children
                    (drop (call $yield))
                    (local.set $count (i32.add (local.get $count) (i32.const 1)))
                    (br_if $allow_children (i32.lt_u (local.get $count) (i32.const 8))))
                (if (call $cancel (local.get $child)) (then unreachable))
                (if (call $cancel (local.get $child)) (then unreachable))
                (if (i32.ne (call $wait (local.get $child) (i32.const 0)) (local.get $child)) (then unreachable))
                (if (i32.ne (i32.load (i32.const 0)) (i32.const 130)) (then unreachable))
                (if (i32.ne (call $cancel (local.get $child)) (i32.const -10)) (then unreachable))))"#,
        ),
    ] {
        world
            .register(name, &wat::parse_str(source).unwrap())
            .unwrap();
    }
    let other = world.spawn(vec!["leaf".into()], vec![]).unwrap();
    let parent = world.spawn(vec!["parent".into()], vec![]).unwrap();
    let mut saw_grandchild = false;
    for _ in 0..100 {
        saw_grandchild |= world.process_ids().count() == 4;
        if !world.status(parent).unwrap().is_live() {
            break;
        }
        world.step();
    }
    assert!(saw_grandchild);
    assert_eq!(world.status(parent).unwrap(), &Status::Exited(0));
    assert!(world.status(other).unwrap().is_live());
    assert_eq!(world.process_ids().count(), 2);
}
