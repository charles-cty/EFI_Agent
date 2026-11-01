use efi_agent_wasm::filesystem::{FileError, FileSystem, OpenOptions, namespace::Namespace};
use efi_agent_wasm::{Status, World, installation::Manifest};
use std::rc::Rc;

fn write(fs: &dyn FileSystem, path: &str, bytes: &[u8]) {
    let mut file = fs
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
    file.flush().unwrap();
}

fn command(code: i32) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module
        (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
        (memory (export "memory") 1)
        (func (export "_start") (call $exit (i32.const {code}))))"#
    ))
    .unwrap()
}

fn manifest(commands: &str) -> String {
    format!(
        r#"{{"version":1,"modules":[{{"path":"images/shared","commands":[{commands}],"libraries":["lib"]}}]}}"#
    )
}

fn fixture() -> (World, Rc<Namespace>) {
    let fs = Rc::new(Namespace::temporary(2 * 1024 * 1024, 256, 1, None));
    fs.create_directory("images").unwrap();
    fs.create_directory("lib").unwrap();
    write(fs.as_ref(), "images/shared", &command(23));
    let mut world = World::new();
    world.set_filesystem(fs.clone()).unwrap();
    (world, fs)
}

fn finish(world: &mut World, name: &str, expected: i32) {
    let pid = world.spawn(vec![name.into()], vec![]).unwrap();
    for _ in 0..100 {
        if !world.status(pid).unwrap().is_live() {
            break;
        }
        world.step();
    }
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(expected));
    world.reap(pid).unwrap();
}

#[test]
fn manifest_aliases_reload_remove_and_respect_explicit_registrations() {
    let (mut world, fs) = fixture();
    write(
        fs.as_ref(),
        "programs.json",
        manifest(r#""first","second""#).as_bytes(),
    );
    world.discover_programs().unwrap();
    assert!(!world.has_program("shared"));
    for name in ["first", "/bin/first", "second", "/bin/second"] {
        finish(&mut world, name, 23);
    }
    world.register("first", &command(41)).unwrap();
    write(
        fs.as_ref(),
        "programs.json",
        manifest(r#""third""#).as_bytes(),
    );
    world.discover_programs().unwrap();
    assert!(!world.has_program("second"));
    finish(&mut world, "third", 23);
    finish(&mut world, "first", 41);
    fs.unlink("programs.json").unwrap();
    world.discover_programs().unwrap();
    assert!(!world.has_program("third"));
    finish(&mut world, "first", 41);
}

#[test]
fn interpreter_symlinks_follow_installed_targets_and_refresh() {
    let (mut world, fs) = fixture();
    for name in ["python3", "bash"] {
        assert!(!world.has_program(name));
        assert_eq!(
            world
                .filesystem()
                .unwrap()
                .read_link(&format!("bin/{name}")),
            Err(FileError::NotFound)
        );
    }
    write(
        fs.as_ref(),
        "programs.json",
        manifest(r#""python","ash""#).as_bytes(),
    );
    world.discover_programs().unwrap();
    let root = world.filesystem().unwrap();
    for (name, target) in [("python3", "python"), ("bash", "ash"), ("sh", "ash")] {
        let path = format!("bin/{name}");
        assert_eq!(root.read_link(&path).unwrap(), "/images/shared");
        assert_eq!(
            root.symlink_metadata(&path).unwrap().kind,
            efi_agent_wasm::filesystem::EntryType::Symlink
        );
        assert_eq!(root.metadata(&path).unwrap().mode, 0o555);
        assert_eq!(
            root.metadata(&path).unwrap().inode,
            root.metadata(&format!("bin/{target}")).unwrap().inode
        );
        let mut file = root
            .open(
                &path,
                OpenOptions {
                    read: true,
                    write: false,
                    create: false,
                    exclusive: false,
                    truncate: false,
                    append: false,
                },
            )
            .unwrap();
        let mut header = [0; 8];
        assert_eq!(file.read(&mut header).unwrap(), 8);
        assert_eq!(&header, b"\0asm\x01\0\0\0");
        assert!(world.has_program(name));
        finish(&mut world, name, 23);
        finish(&mut world, &format!("/bin/{name}"), 23);
    }
    write(fs.as_ref(), "images/shared", &command(31));
    for name in ["python3", "bash"] {
        finish(&mut world, name, 31);
    }
    write(
        fs.as_ref(),
        "programs.json",
        manifest(r#""unrelated""#).as_bytes(),
    );
    world.discover_programs().unwrap();
    for name in ["python3", "bash"] {
        assert!(!world.has_program(name));
        assert_eq!(
            root.read_link(&format!("bin/{name}")),
            Err(FileError::NotFound)
        );
    }
}

#[test]
fn interpreter_symlinks_preserve_explicit_commands() {
    for (name, target) in [("python3", "python"), ("bash", "ash")] {
        let (mut world, fs) = fixture();
        write(
            fs.as_ref(),
            "programs.json",
            manifest(&format!("\"{target}\"")).as_bytes(),
        );
        world.discover_programs().unwrap();
        let root = world.filesystem().unwrap();
        assert_eq!(
            root.read_link(&format!("bin/{name}")).unwrap(),
            "/images/shared"
        );
        finish(&mut world, name, 23);
        world.register(name, &command(41)).unwrap();
        finish(&mut world, name, 41);
        finish(&mut world, target, 23);
        assert_eq!(
            root.read_link(&format!("bin/{name}")),
            Err(FileError::Invalid)
        );

        // Manifest-owned commands also override the default link.
        let (mut world, fs) = fixture();
        write(
            fs.as_ref(),
            "programs.json",
            manifest(&format!("\"{name}\"")).as_bytes(),
        );
        world.register(target, &command(7)).unwrap();
        world.discover_programs().unwrap();
        finish(&mut world, name, 23);
        finish(&mut world, target, 7);
        assert_eq!(
            world
                .filesystem()
                .unwrap()
                .read_link(&format!("bin/{name}"))
                .unwrap(),
            "/images/shared"
        );
    }
}

#[test]
fn interpreter_symlink_collisions_preserve_workspace_entries() {
    for (name, target) in [("python3", "python"), ("bash", "ash")] {
        let (mut world, fs) = fixture();
        fs.create_directory("bin").unwrap();
        write(fs.as_ref(), &format!("bin/{name}"), b"user command");
        write(
            fs.as_ref(),
            "programs.json",
            manifest(&format!("\"{target}\"")).as_bytes(),
        );
        assert_eq!(
            world.discover_programs().unwrap_err().kind,
            FileError::Exists
        );
        assert!(!world.has_program(target));
        assert!(!world.has_program(name));
        assert_eq!(
            efi_agent_wasm::filesystem::text::read(
                world.filesystem().unwrap().as_ref(),
                &format!("bin/{name}")
            )
            .unwrap(),
            "user command"
        );
    }
}

#[test]
fn invalid_installation_keeps_prior_bin_and_reports_the_failing_asset() {
    let (mut world, fs) = fixture();
    write(
        fs.as_ref(),
        "programs.json",
        manifest(r#""first""#).as_bytes(),
    );
    world.discover_programs().unwrap();
    write(
        fs.as_ref(),
        "programs.json",
        manifest(r#""second""#).as_bytes(),
    );
    fs.remove_directory("lib").unwrap();
    let error = world.discover_programs().unwrap_err();
    assert_eq!(error.path, "lib");
    assert_eq!(error.kind, FileError::NotFound);
    assert!(!world.has_program("second"));
    finish(&mut world, "first", 23);
    fs.create_directory("lib").unwrap();
    fs.create_directory("bin").unwrap();
    write(fs.as_ref(), "bin/second", b"preserve");
    assert_eq!(
        world.discover_programs().unwrap_err().kind,
        FileError::Exists
    );
    assert!(!world.has_program("second"));
    finish(&mut world, "first", 23);
    fs.unlink("bin/second").unwrap();
    world.discover_programs().unwrap();
    assert!(!world.has_program("first"));
    finish(&mut world, "second", 23);
}

#[test]
fn file_backed_metadata_does_not_fabricate_a_deleted_source() {
    let (mut world, fs) = fixture();
    write(
        fs.as_ref(),
        "programs.json",
        manifest(r#""first""#).as_bytes(),
    );
    world.discover_programs().unwrap();
    finish(&mut world, "first", 23);
    fs.unlink("images/shared").unwrap();
    let root = world.filesystem().unwrap();
    assert_eq!(root.metadata("bin/first").unwrap_err(), FileError::NotFound);
    assert_eq!(
        root.symlink_metadata("bin/first").unwrap().kind,
        efi_agent_wasm::filesystem::EntryType::Symlink
    );
    assert!(world.spawn(vec!["first".into()], vec![]).is_err());
    assert_eq!(world.discover_programs().unwrap_err().path, "images/shared");
}

#[test]
fn explicit_command_overrides_the_implicit_busybox_applet() {
    let (mut world, fs) = fixture();
    world.register("ash", &command(7)).unwrap();
    world.register("ls", &command(9)).unwrap();
    finish(&mut world, "ls", 9);
    finish(&mut world, "/bin/ls", 9);
    finish(&mut world, "cat", 7);
    write(
        fs.as_ref(),
        "programs.json",
        manifest(r#""cat""#).as_bytes(),
    );
    world.discover_programs().unwrap();
    finish(&mut world, "cat", 23);
    finish(&mut world, "/bin/../bin/cat", 23);
}

#[test]
fn command_validation_is_deferred_until_spawn_without_running_invalid_images() {
    let invalid = [
        r#"(module (memory (export "memory") 1))"#,
        r#"(module (memory (export "memory") 1) (func (export "_start") (param i32)))"#,
        r#"(module (func (export "_start")))"#,
        r#"(module (import "wasi_snapshot_preview1" "fd_read" (func (param i64) (result i32))) (memory (export "memory") 1) (func (export "_start")))"#,
        r#"(module (import "unknown" "call" (func)) (memory (export "memory") 1) (func (export "_start")))"#,
        r#"(module (import "unknown" "memory" (memory 1)) (export "memory" (memory 0)) (func (export "_start")))"#,
        r#"(module (memory (export "memory") 1) (func $init) (start $init) (func (export "_start")))"#,
    ];
    for text in invalid {
        let (mut world, fs) = fixture();
        write(fs.as_ref(), "bad.wasm", &wat::parse_str(text).unwrap());
        world.discover_programs().unwrap();
        assert!(world.has_program("bad"));
        assert!(world.spawn(vec!["bad".into()], vec![]).is_err(), "{text}");
        assert_eq!(world.process_ids().count(), 0);
    }
    let (mut world, fs) = fixture();
    // A valid envelope with an invalid body must fail before allocating a process.
    let invalid_body = wat::parse_str(
        r#"(module (memory (export "memory") 1)
        (func (export "_start") i32.add drop))"#,
    )
    .unwrap();
    write(fs.as_ref(), "bad.wasm", &invalid_body);
    world.discover_programs().unwrap();
    assert!(world.spawn(vec!["bad".into()], vec![]).is_err());
    assert_eq!(world.process_ids().count(), 0);
    fs.unlink("bad.wasm").unwrap();
    let trap = wat::parse_str(
        r#"(module (memory (export "memory") 1) (func (export "_start") unreachable))"#,
    )
    .unwrap();
    write(fs.as_ref(), "traps.wasm", &trap);
    world.discover_programs().unwrap();
    assert!(world.has_program("traps"));
}

#[test]
fn manifest_rejects_ambiguous_paths_names_versions_and_unknown_fields() {
    let base = manifest(r#""first""#);
    Manifest::parse(base.as_bytes()).unwrap();
    for invalid in [
        base.replace("\"version\":1", "\"version\":2"),
        base.replace("images/shared", "../shared"),
        base.replace("images/shared", "/shared"),
        base.replace("images/shared", "images//shared"),
        base.replace("images/shared", "images/./shared"),
        base.replace("images/shared", "bin/shared"),
        base.replace("images/shared", ".efi-agent-fs/shared"),
        base.replace("images/shared", "C:/shared"),
        base.replace("\"first\"", "\"..\""),
        base.replace("\"first\"", "\"a/b\""),
        base.replace("\"first\"", "\"first\",\"first\""),
        base.replace("\"commands\"", "\"typo\""),
        base.replace("[\"first\"]", "[]"),
    ] {
        assert!(Manifest::parse(invalid.as_bytes()).is_err(), "{invalid}");
    }
    let entry = base
        .strip_prefix(r#"{"version":1,"modules":["#)
        .unwrap()
        .strip_suffix("]}")
        .unwrap();
    let duplicate = format!(r#"{{"version":1,"modules":[{entry},{entry}]}}"#);
    assert!(Manifest::parse(duplicate.as_bytes()).is_err());
    assert!(Manifest::parse(&vec![b' '; 65537]).is_err());
}

#[test]
fn manifest_limits_accept_the_boundary_and_reject_the_next_entry() {
    let base = manifest(r#""first""#);
    let mut exact = base.as_bytes().to_vec();
    exact.resize(65536, b' ');
    Manifest::parse(&exact).unwrap();
    exact.push(b' ');
    assert_eq!(
        Manifest::parse(&exact).unwrap_err().kind,
        FileError::FileTooLarge
    );
    let names = (0..512)
        .map(|index| format!("\"command{index}\""))
        .collect::<Vec<_>>()
        .join(",");
    Manifest::parse(manifest(&names).as_bytes()).unwrap();
    assert!(Manifest::parse(manifest(&format!("{names},\"extra\"")).as_bytes()).is_err());
    for (length, valid) in [(127, true), (128, false)] {
        assert_eq!(
            Manifest::parse(manifest(&format!("\"{}\"", "a".repeat(length))).as_bytes()).is_ok(),
            valid
        );
    }
    let mut parsed = Manifest::parse(base.as_bytes()).unwrap();
    let template = parsed.modules[0].clone();
    parsed.modules = (0..128)
        .map(|index| {
            let mut module = template.clone();
            module.path = format!("images/module{index}");
            module.commands = vec![format!("name{index}")];
            module
        })
        .collect();
    parsed.validate().unwrap();
    let mut extra = template;
    extra.path = "images/extra".into();
    extra.commands = vec!["extra".into()];
    parsed.modules.push(extra);
    assert!(parsed.validate().is_err());
}
