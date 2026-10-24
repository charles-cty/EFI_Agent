use efi_agent_wasm::{
    Status, World,
    filesystem::{FileError, FileSystem, OpenOptions, namespace::Namespace},
};
use std::rc::Rc;

fn program(path: &str, terminal: &str) -> Vec<u8> {
    wat::parse_str(format!(r#"(module
      (import "wasi_snapshot_preview1" "path_open" (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_read" (func $read (param i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "sched_yield" (func $yield (result i32)))
      (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
      (memory (export "memory") 1)
      (data (i32.const 64) ".efi-process")
      (data (i32.const 256) "\11\00\00\00{path}")
      (func $check (param $result i32)
        (if (local.get $result) (then unreachable)))
      (func (export "_start")
        (call $check (call $open (i32.const 3) (i32.const 0) (i32.const 64) (i32.const 12) (i32.const 0) (i64.const 66) (i64.const 0) (i32.const 0) (i32.const 32)))
        (i32.store (i32.const 128) (i32.const 256))
        (i32.store (i32.const 132) (i32.const {size}))
        (call $check (call $write (i32.load (i32.const 32)) (i32.const 128) (i32.const 1) (i32.const 48)))
        (i32.store (i32.const 128) (i32.const 512))
        (i32.store (i32.const 132) (i32.const 8))
        (call $check (call $read (i32.load (i32.const 32)) (i32.const 128) (i32.const 1) (i32.const 48)))
        (call $check (i32.load (i32.const 512)))
        (drop (call $yield))
        {terminal}))"#, size = path.len() + 4)).unwrap()
}

#[test]
fn libc_cwd_routing_preserves_user_path_and_symlink_budgets() {
    let namespace = Rc::new(Namespace::temporary(1024, 128, 0, None));
    let file = namespace
        .open(
            "data",
            OpenOptions {
                read: true,
                write: true,
                create: true,
                exclusive: false,
                truncate: false,
                append: false,
            },
        )
        .unwrap();
    drop(file);
    for index in (0..40).rev() {
        let target = if index == 39 {
            String::from("data")
        } else {
            format!("link{}", index + 1)
        };
        namespace.symlink(&target, &format!("link{index}")).unwrap();
    }
    namespace.symlink("link0", "extra").unwrap();
    let cases = [
        (format!("proc/self/cwd/{}data", "./".repeat(2046)), 0),
        (format!("proc/self/cwd/{}data", "./".repeat(2047)), 48),
        (String::from("proc/self/cwd/link0"), 0),
        (String::from("proc/self/cwd/extra"), 32),
    ];
    let mut data = String::new();
    let mut body = String::new();
    let mut offset = 256;
    for (path, expected) in cases {
        data.push_str(&format!("(data (i32.const {offset}) \"{path}\")"));
        body.push_str(&format!(r#"
          (if (i32.ne (call $open (i32.const 3) (i32.const 1) (i32.const {offset}) (i32.const {length}) (i32.const 0) (i64.const 2) (i64.const 0) (i32.const 0) (i32.const 32)) (i32.const {expected})) (then unreachable))
        "#, length=path.len()));
        if expected == 0 {
            body.push_str("(drop (call $close (i32.load (i32.const 32))))");
        }
        offset += path.len();
    }
    let wasm = wat::parse_str(format!(r#"(module
      (import "wasi_snapshot_preview1" "path_open" (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_close" (func $close (param i32) (result i32)))
      (memory (export "memory") 1) {data}
      (func (export "_start") {body}))"#)).unwrap();
    let mut world = World::new();
    world.set_filesystem(namespace).unwrap();
    world.register("paths", &wasm).unwrap();
    let pid = world.spawn(vec!["paths".into()], vec![]).unwrap();
    for _ in 0..100 {
        if !world.status(pid).unwrap().is_live() {
            break;
        }
        world.step();
    }
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
}

#[test]
fn executable_cwd_aliases_resolve_to_the_current_file_and_compiled_command() {
    let namespace = Rc::new(Namespace::temporary(1024 * 1024, 32, 0, None));
    namespace.create_directory("a").unwrap();
    let module = |code| {
        wat::parse_str(format!(
            r#"(module
      (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
      (memory (export "memory") 1)
      (func (export "_start") (call $exit (i32.const {code}))))"#
        ))
        .unwrap()
    };
    let write = |path: &str, bytes: &[u8]| {
        let mut file = namespace
            .open(
                path,
                OpenOptions {
                    read: false,
                    write: true,
                    create: true,
                    exclusive: false,
                    truncate: true,
                    append: false,
                },
            )
            .unwrap();
        assert_eq!(file.write(bytes).unwrap(), bytes.len());
        namespace.set_mode(path, 0o700).unwrap();
    };
    write("a/tool", &module(7));
    namespace.symlink("/bin/installed", "a/command").unwrap();
    let mut world = World::new();
    world.set_filesystem(namespace.clone()).unwrap();
    world.register("installed", &module(11)).unwrap();
    world
        .register(
            "parent",
            &program("/a", "(loop $again (drop (call $yield)) (br $again))"),
        )
        .unwrap();
    let parent = world.spawn(vec!["parent".into()], vec![]).unwrap();
    let root = world.filesystem().unwrap();
    for _ in 0..10 {
        world.step();
        if root.read_link(&format!("proc/{parent}/cwd")).unwrap() == "/a" {
            break;
        }
    }
    let finish = |world: &mut World, command: String, expected| {
        let pid = world.spawn(vec![command], vec![]).unwrap();
        for _ in 0..100 {
            if !world.status(pid).unwrap().is_live() {
                break;
            }
            world.step();
        }
        assert_eq!(world.status(pid).unwrap(), &Status::Exited(expected));
        world.reap(pid).unwrap();
    };
    finish(&mut world, format!("/proc/{parent}/cwd/tool"), 7);
    namespace.rename("a", "renamed").unwrap();
    write("renamed/tool", &module(9));
    finish(&mut world, format!("/proc/{parent}/cwd/tool"), 9);
    finish(&mut world, format!("/proc/{parent}/cwd/command"), 11);
    world.cancel(parent).unwrap();
}

#[test]
fn full_spawn_frame_keeps_the_private_cwd_prefix_outside_user_limits() {
    let mut arguments = b"child\0".to_vec();
    arguments.extend(std::iter::repeat_n(b'x', 65_529));
    arguments.push(0);
    assert_eq!(arguments.len(), 65_536);
    let cwd = format!("/proc/self/cwd/{}..", "./".repeat(2047));
    let mut packet = Vec::new();
    for word in [23, arguments.len() as u32, 0, 127, cwd.len() as u32, 0o077] {
        packet.extend(word.to_le_bytes());
    }
    for target in (0..128u32).filter(|target| *target != 3) {
        packet.extend(target.to_le_bytes());
        packet.extend(0u32.to_le_bytes());
    }
    packet.extend(arguments);
    packet.extend(cwd.bytes());
    assert_eq!(packet.len(), 70_687);
    let encoded = packet
        .iter()
        .map(|byte| format!("\\{byte:02x}"))
        .collect::<String>();
    let wasm = wat::parse_str(format!(r#"(module
      (import "wasi_snapshot_preview1" "path_open" (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_read" (func $read (param i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 2)
      (data (i32.const 64) ".efi-process")
      (data (i32.const 256) "{encoded}")
      (func $check (param $result i32) (if (local.get $result) (then unreachable)))
      (func (export "_start")
        (call $check (call $open (i32.const 3) (i32.const 0) (i32.const 64) (i32.const 12) (i32.const 0) (i64.const 66) (i64.const 0) (i32.const 0) (i32.const 32)))
        (i32.store (i32.const 128) (i32.const 256))
        (i32.store (i32.const 132) (i32.const {size}))
        (call $check (call $write (i32.load (i32.const 32)) (i32.const 128) (i32.const 1) (i32.const 48)))
        (i32.store (i32.const 128) (i32.const 512))
        (i32.store (i32.const 132) (i32.const 8))
        (call $check (call $read (i32.load (i32.const 32)) (i32.const 128) (i32.const 1) (i32.const 48)))
        (if (i32.ne (i32.load (i32.const 512)) (i32.const 2)) (then unreachable))))"#, size=packet.len())).unwrap();
    let mut world = World::new();
    world
        .set_filesystem(Rc::new(Namespace::temporary(1024, 32, 0, None)))
        .unwrap();
    let child =
        wat::parse_str("(module (memory (export \"memory\") 1) (func (export \"_start\")))")
            .unwrap();
    world.register("child", &child).unwrap();
    world.register("parent", &wasm).unwrap();
    let pid = world.spawn(vec!["parent".into()], vec![]).unwrap();
    for _ in 0..100 {
        if !world.status(pid).unwrap().is_live() {
            break;
        }
        world.step();
    }
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
}

#[test]
fn cwd_magic_links_retain_identity_and_release_deleted_ancestry_on_all_terminal_paths() {
    for terminal in [
        "",
        "unreachable",
        "(call $exit (i32.const 7))",
        "(loop $again (drop (call $yield)) (br $again))",
    ] {
        let namespace = Rc::new(Namespace::temporary(1024, 32, 0, None));
        namespace.create_directory("a").unwrap();
        namespace.create_directory("a/child").unwrap();
        let inode = namespace.metadata("a/child").unwrap().inode;
        let parent_inode = namespace.metadata("a").unwrap().inode;
        let mut world = World::new();
        world.set_filesystem(namespace.clone()).unwrap();
        world
            .register("cwd", &program("/a/child", terminal))
            .unwrap();
        let pid = world.spawn(vec!["cwd".into()], vec![]).unwrap();
        let root = world.filesystem().unwrap();
        let link = format!("proc/{pid}/cwd");
        for _ in 0..10 {
            world.step();
            if root.read_link(&link).unwrap() == "/a/child" {
                break;
            }
        }
        assert_eq!(root.read_link(&link).unwrap(), "/a/child");
        namespace.rename("a", "moved").unwrap();
        namespace.create_directory("a").unwrap();
        namespace.create_directory("a/child").unwrap();
        assert_eq!(root.read_link(&link).unwrap(), "/moved/child");
        assert_eq!(root.metadata(&link).unwrap().inode, inode);
        assert_eq!(
            root.metadata(&format!("{link}/..")).unwrap().inode,
            parent_inode
        );
        let capability = root.directory_view(&link).unwrap().unwrap();
        assert_eq!(
            capability.metadata("..").unwrap_err(),
            FileError::NotCapable
        );
        drop(capability);
        namespace.remove_directory("moved/child").unwrap();
        namespace.remove_directory("moved").unwrap();
        assert!(root.read_link(&link).unwrap().ends_with(" (deleted)"));
        assert_eq!(root.metadata(&format!("{link}/.")).unwrap().inode, inode);
        assert_eq!(
            root.metadata(&format!("{link}/..")).unwrap().inode,
            parent_inode
        );
        assert_eq!(namespace.filesystem_statistics("").unwrap().files_free, 27);
        if terminal.starts_with("(loop") {
            world.cancel(pid).unwrap();
        } else {
            for _ in 0..10 {
                if !world.status(pid).unwrap().is_live() {
                    break;
                }
                world.step();
            }
        }
        assert!(!world.status(pid).unwrap().is_live());
        if terminal == "unreachable" {
            assert!(matches!(world.status(pid).unwrap(), Status::Trapped(_)));
        }
        assert_eq!(namespace.filesystem_statistics("").unwrap().files_free, 29);
        assert_eq!(root.metadata(&link).unwrap_err(), FileError::NotFound);
        world.reap(pid).unwrap();
        assert!(namespace.take_errors().is_empty());
    }
}
