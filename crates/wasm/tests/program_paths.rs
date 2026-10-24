use efi_agent_wasm::filesystem::{
    FileError, FileSystem, namespace::Namespace, native::NativeFileSystem,
};
use efi_agent_wasm::{Status, World};
use std::{
    fs,
    rc::Rc,
    time::{SystemTime, UNIX_EPOCH},
};

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
fn filesystem_path_order_modes_shebangs_and_symlink_parent_semantics() {
    let root = std::env::temp_dir().join(format!(
        "efi-paths-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let module = |code| {
        wat::parse_str(format!(
            r#"(module
        (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
        (memory (export "memory") 1)
        (func (export "_start") (call $exit (i32.const {code}))))"#
        ))
        .unwrap()
    };
    for (directory, code) in [("one", 23), ("two", 41)] {
        fs::create_dir(root.join(directory)).unwrap();
        fs::write(root.join(directory).join("pick"), module(code)).unwrap();
    }
    let namespace =
        Rc::new(Namespace::open(Rc::new(NativeFileSystem(root.clone())), None).unwrap());
    namespace.set_mode("one/pick", 0o700).unwrap();
    namespace.set_mode("two/pick", 0o700).unwrap();
    namespace.create_directory("two/inner").unwrap();
    namespace.symlink("two/inner", "linked").unwrap();
    let mut world = World::new();
    world.set_filesystem(namespace.clone());
    for (name, path, code) in [
        ("pick", "/one:/two", 23),
        ("pick", "/missing:/two:/one", 41),
        ("./two/pick", "/one", 41),
        ("/linked/../pick", "/one", 41),
    ] {
        let pid = world
            .spawn_with_environment(vec![name.into()], vec![format!("PATH={path}")], vec![])
            .unwrap();
        for _ in 0..100 {
            if !world.status(pid).unwrap().is_live() {
                break;
            }
            world.step();
        }
        assert_eq!(
            world.status(pid).unwrap(),
            &Status::Exited(code),
            "{name} {path}"
        );
    }
    namespace.set_mode("one/pick", 0o600).unwrap();
    let pid = world
        .spawn_with_environment(vec!["pick".into()], vec!["PATH=/one:/two".into()], vec![])
        .unwrap();
    for _ in 0..100 {
        if !world.status(pid).unwrap().is_live() {
            break;
        }
        world.step();
    }
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(41));
    assert!(matches!(
        world.spawn(vec!["/one/pick".into()], vec![]),
        Err(efi_agent_wasm::Error::ProgramFile(FileError::Access))
    ));
    assert!(
        world
            .spawn(vec!["/one/../missing/pick".into()], vec![])
            .is_err()
    );
    drop(world);
    drop(namespace);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn installation_rejects_bin_collisions_without_publishing_partial_aliases() {
    let root = std::env::temp_dir().join(format!(
        "efi-bin-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    fs::create_dir(root.join("bin")).unwrap();
    let module =
        wat::parse_str("(module (memory (export \"memory\") 1) (func (export \"_start\")))")
            .unwrap();
    fs::write(root.join("ash.wasm"), module).unwrap();
    fs::write(root.join("bin/ls"), b"workspace file").unwrap();
    let namespace =
        Rc::new(Namespace::open(Rc::new(NativeFileSystem(root.clone())), None).unwrap());
    let mut world = World::new();
    world.set_filesystem(namespace.clone());
    assert_eq!(world.discover_programs(), Err(FileError::Exists));
    assert!(!world.has_program("ash"));
    assert!(!world.has_program("ls"));
    assert_eq!(namespace.metadata("bin/ls").unwrap().size, 14);
    namespace.unlink("bin/ls").unwrap();
    world.discover_programs().unwrap();
    assert!(world.has_program("ls"));
    drop(world);
    drop(namespace);
    fs::remove_dir_all(root).unwrap();
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
