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
        r#"{{"version":1,"modules":[{{"path":"images/shared","commands":[{commands}],"libraries":["lib"],"licenses":["COPYING"]}}]}}"#
    )
}

fn fixture() -> (World, Rc<Namespace>) {
    let fs = Rc::new(Namespace::temporary(2 * 1024 * 1024, 256, 1, None));
    fs.create_directory("images").unwrap();
    fs.create_directory("lib").unwrap();
    write(fs.as_ref(), "COPYING", b"test fixture license");
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
    fs.unlink("COPYING").unwrap();
    let error = world.discover_programs().unwrap_err();
    assert_eq!(error.path, "COPYING");
    assert_eq!(error.kind, FileError::NotFound);
    assert!(!world.has_program("second"));
    finish(&mut world, "first", 23);
    write(fs.as_ref(), "COPYING", b"license");
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
fn command_validation_checks_bodies_import_signatures_and_entrypoint_without_running() {
    let invalid = [
        (
            r#"(module (memory (export "memory") 1))"#,
            FileError::Invalid,
        ),
        (
            r#"(module (memory (export "memory") 1) (func (export "_start") (param i32)))"#,
            FileError::Invalid,
        ),
        (r#"(module (func (export "_start")))"#, FileError::Invalid),
        (
            r#"(module (import "wasi_snapshot_preview1" "fd_read" (func (param i64) (result i32))) (memory (export "memory") 1) (func (export "_start")))"#,
            FileError::Unsupported,
        ),
        (
            r#"(module (import "unknown" "call" (func)) (memory (export "memory") 1) (func (export "_start")))"#,
            FileError::Unsupported,
        ),
        (
            r#"(module (import "unknown" "memory" (memory 1)) (export "memory" (memory 0)) (func (export "_start")))"#,
            FileError::Unsupported,
        ),
        (
            r#"(module (memory (export "memory") 1) (func $init) (start $init) (func (export "_start")))"#,
            FileError::Invalid,
        ),
    ];
    for (text, expected) in invalid {
        let (mut world, fs) = fixture();
        write(fs.as_ref(), "bad.wasm", &wat::parse_str(text).unwrap());
        assert_eq!(
            world.discover_programs().unwrap_err().kind,
            expected,
            "{text}"
        );
        assert!(!world.has_program("bad"));
    }
    let (mut world, fs) = fixture();
    // A valid binary envelope with a type-invalid body must fail before publish.
    let invalid_body = wat::parse_str(
        r#"(module (memory (export "memory") 1)
        (func (export "_start") i32.add drop))"#,
    )
    .unwrap();
    write(fs.as_ref(), "bad.wasm", &invalid_body);
    assert_eq!(
        world.discover_programs().unwrap_err().kind,
        FileError::Invalid
    );
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
        base.replace("[\"COPYING\"]", "[]"),
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
