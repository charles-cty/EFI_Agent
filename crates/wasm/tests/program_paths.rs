use efi_agent_wasm::{Status, World};

fn finish(world: &mut World, name: &str, expected: i32) {
    let pid = world.spawn(vec![name.into()], vec![]).unwrap();
    for _ in 0..100 {
        if !world.status(pid).unwrap().is_live() {
            break;
        }
        world.step();
    }
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(expected));
}

#[test]
fn bin_prefix_resolves_registered_modules_and_multicall_commands_only() {
    let module = wat::parse_str(
        r#"(module
            (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
            (memory (export "memory") 1)
            (func (export "_start") (call $exit (i32.const 23))))"#,
    )
    .unwrap();
    let mut world = World::new();
    assert!(!world.has_program("/bin/awk"));
    world.register("ash", &module).unwrap();
    world.register("custom", &module).unwrap();
    for name in [
        "ash",
        "/bin/ash",
        "awk",
        "/bin/awk",
        "custom",
        "/bin/custom",
    ] {
        assert!(world.has_program(name), "{name}");
        finish(&mut world, name, 23);
    }
    for name in [
        "/bin/missing",
        "/sbin/custom",
        "/bin/custom/extra",
        "Custom",
    ] {
        assert!(!world.has_program(name), "{name}");
        assert!(world.spawn(vec![name.into()], vec![]).is_err(), "{name}");
    }
}
