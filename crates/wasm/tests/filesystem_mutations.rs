#[path = "../examples/support/native_fs.rs"]
mod native_fs;

use efi_agent_wasm::{Status, World};
use std::{fs, path::PathBuf, rc::Rc, time::SystemTime};

struct Workspace(PathBuf);

impl Workspace {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("efi-filesystem-{}-{nonce}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn run(&self, wat: &str) {
        let mut world = World::new();
        world.set_filesystem(Rc::new(native_fs::NativeFileSystem(self.0.clone())));
        world
            .register("files", &wat::parse_str(wat).unwrap())
            .unwrap();
        let pid = world.spawn(vec!["files".into()], vec![]).unwrap();
        for _ in 0..10000 {
            if !world.status(pid).unwrap().is_live() {
                break;
            }
            world.step();
        }
        assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn persistent_namespace_wasi_mutations_keep_directory_and_file_identity() {
    use efi_agent_wasm::filesystem::{FileSystem, namespace::Namespace};
    let workspace = Workspace::new();
    let namespace = Rc::new(
        Namespace::open(
            Rc::new(native_fs::NativeFileSystem(workspace.0.clone())),
            Some(Rc::new(|| Ok(731))),
        )
        .unwrap(),
    );
    let module = wat::parse_str(r#"(module
      (import "wasi_snapshot_preview1" "path_create_directory" (func $mkdir (param i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "path_open" (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "path_rename" (func $rename (param i32 i32 i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "path_symlink" (func $symlink (param i32 i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "path_readlink" (func $readlink (param i32 i32 i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "path_unlink_file" (func $unlink (param i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "path_remove_directory" (func $rmdir (param i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "path_filestat_get" (func $stat (param i32 i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "path_filestat_set_times" (func $times (param i32 i32 i32 i32 i64 i64 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_filestat_get" (func $fstat (param i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_filestat_set_times" (func $ftimes (param i32 i64 i64 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_close" (func $close (param i32) (result i32)))
      (memory (export "memory") 1)
      (data (i32.const 100) "a")
      (data (i32.const 110) "moved")
      (data (i32.const 120) "f")
      (data (i32.const 130) "l")
      (data (i32.const 140) "a/f")
      (func $ok (param $error i32) (if (local.get $error) (then unreachable)))
      (func $equal (param $a i32) (param $b i32)
        (if (i32.ne (local.get $a) (local.get $b)) (then unreachable)))
      (func (export "_start") (local $dir i32) (local $file i32)
        (call $ok (call $mkdir (i32.const 3) (i32.const 100) (i32.const 1)))
        (call $ok (call $open (i32.const 3) (i32.const 1) (i32.const 100) (i32.const 1) (i32.const 2) (i64.const 268428919) (i64.const 268428919) (i32.const 0) (i32.const 0)))
        (local.set $dir (i32.load (i32.const 0)))
        (call $ok (call $symlink (i32.const 140) (i32.const 3) (i32.const 3) (i32.const 130) (i32.const 1)))
        (call $equal (call $open (i32.const 3) (i32.const 0) (i32.const 130) (i32.const 1) (i32.const 1) (i64.const 66) (i64.const 0) (i32.const 0) (i32.const 0)) (i32.const 32))
        (call $ok (call $open (i32.const 3) (i32.const 1) (i32.const 130) (i32.const 1) (i32.const 1) (i64.const 10485826) (i64.const 0) (i32.const 0) (i32.const 0)))
        (local.set $file (i32.load (i32.const 0)))
        (call $ok (call $stat (i32.const 3) (i32.const 0) (i32.const 130) (i32.const 1) (i32.const 512)))
        (call $equal (i32.load8_u (i32.const 528)) (i32.const 7))
        (call $ok (call $times (i32.const 3) (i32.const 1) (i32.const 130) (i32.const 1) (i64.const 193) (i64.const 853) (i32.const 5)))
        (call $ok (call $stat (i32.const 3) (i32.const 1) (i32.const 130) (i32.const 1) (i32.const 512)))
        (call $equal (i32.load (i32.const 552)) (i32.const 193))
        (call $equal (i32.load (i32.const 560)) (i32.const 853))
        (call $ok (call $readlink (i32.const 3) (i32.const 130) (i32.const 1) (i32.const 600) (i32.const 2) (i32.const 604)))
        (call $equal (i32.load16_u (i32.const 600)) (i32.const 12129))
        (call $equal (i32.load (i32.const 604)) (i32.const 2))
        (call $ok (call $rename (i32.const 3) (i32.const 100) (i32.const 1) (i32.const 3) (i32.const 110) (i32.const 5)))
        (call $ok (call $stat (local.get $dir) (i32.const 1) (i32.const 120) (i32.const 1) (i32.const 512)))
        (call $ok (call $unlink (local.get $dir) (i32.const 120) (i32.const 1)))
        (call $ok (call $ftimes (local.get $file) (i64.const 17) (i64.const 29) (i32.const 5)))
        (call $ok (call $fstat (local.get $file) (i32.const 512)))
        (call $equal (i32.load (i32.const 536)) (i32.const 0))
        (call $equal (i32.load (i32.const 552)) (i32.const 17))
        (call $ok (call $rmdir (i32.const 3) (i32.const 110) (i32.const 5)))
        (call $ok (call $close (local.get $file)))
        (call $ok (call $close (local.get $dir)))))"#).unwrap();
    let mut world = World::new();
    world.set_filesystem(namespace.clone());
    world.register("mutations", &module).unwrap();
    let pid = world.spawn(vec!["mutations".into()], vec![]).unwrap();
    for _ in 0..1000 {
        if !world.status(pid).unwrap().is_live() {
            break;
        }
        world.step();
    }
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
    assert_eq!(namespace.read_link("l").unwrap(), "a/f");
    assert!(namespace.metadata("moved").is_err());
    assert!(namespace.take_errors().is_empty());
}

#[test]
fn virtual_mounts_supply_devices_and_ephemeral_tmp_files() {
    use efi_agent_wasm::filesystem::{FileSystem, OpenOptions, VirtualFileSystem};
    let workspace = Workspace::new();
    let virtual_fs =
        VirtualFileSystem::new(Rc::new(native_fs::NativeFileSystem(workspace.0.clone())));
    assert!(virtual_fs.metadata("bin").unwrap().is_directory());
    assert!(virtual_fs.metadata("dev").unwrap().is_directory());
    assert!(virtual_fs.metadata("proc").unwrap().is_directory());
    assert!(virtual_fs.metadata("tmp").unwrap().is_directory());

    let mut null = virtual_fs
        .open(
            "dev/null",
            OpenOptions {
                read: false,
                write: true,
                create: false,
                exclusive: false,
                truncate: false,
                append: false,
            },
        )
        .unwrap();
    assert_eq!(null.write(b"discarded").unwrap(), 9);

    let mut tmp = virtual_fs
        .open(
            "tmp/value",
            OpenOptions {
                read: true,
                write: true,
                create: true,
                exclusive: true,
                truncate: false,
                append: false,
            },
        )
        .unwrap();
    assert_eq!(tmp.write(b"temporary").unwrap(), 9);
    tmp.seek(0, 0).unwrap();
    let mut bytes = [0; 9];
    assert_eq!(tmp.read(&mut bytes).unwrap(), 9);
    assert_eq!(&bytes, b"temporary");
    assert_eq!(virtual_fs.metadata("tmp/value").unwrap().size, 9);
    virtual_fs.rename("tmp/value", "tmp/moved").unwrap();
    virtual_fs.unlink("tmp/moved").unwrap();
    assert!(virtual_fs.metadata("tmp/moved").is_err());
    // Unlink preserves open handles, including reads after a seek beyond EOF.
    tmp.seek(32, 0).unwrap();
    assert_eq!(tmp.read(&mut bytes).unwrap(), 0);
    assert!(tmp.resize(48 * 1024 * 1024 + 1).is_err());
    assert!(tmp.write(b"x").is_ok());
    tmp.seek(48 * 1024 * 1024, 0).unwrap();
    assert!(tmp.write(b"overflow").is_err());
    tmp.seek(0, 2).unwrap();
    assert_eq!(tmp.read(&mut bytes).unwrap(), 0);
    let mut uptime = virtual_fs
        .open(
            "proc/uptime",
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
    uptime.seek(4096, 0).unwrap();
    assert_eq!(uptime.read(&mut bytes).unwrap(), 0);
    assert!(!workspace.0.join("tmp").exists());
}

#[test]
fn virtual_wrapper_preserves_symlink_walk_and_anchored_cross_directory_rename() {
    use efi_agent_wasm::filesystem::{FileSystem, VirtualFileSystem, namespace::Namespace};
    let workspace = Workspace::new();
    let namespace = Rc::new(
        Namespace::open(
            Rc::new(native_fs::NativeFileSystem(workspace.0.clone())),
            Some(Rc::new(|| Ok(731))),
        )
        .unwrap(),
    );
    namespace.create_directory("a").unwrap();
    namespace.create_directory("b").unwrap();
    namespace.create_directory("b/deep").unwrap();
    namespace.create_directory("a/tmp").unwrap();
    efi_agent_filesystem::text::write(namespace.as_ref(), "a/tmp/local", "nested").unwrap();
    efi_agent_filesystem::text::write(namespace.as_ref(), "b/value", "payload").unwrap();
    namespace.symlink("../b/deep", "a/link").unwrap();
    let fs = VirtualFileSystem::new(namespace.clone());
    let path = fs.resolve_path("", "a/link/../value").unwrap();
    assert_eq!(
        fs.metadata(&path).unwrap().inode,
        namespace.metadata("b/value").unwrap().inode
    );
    let from = fs.directory_view("b").unwrap().unwrap();
    let to = fs.directory_view("a").unwrap().unwrap();
    assert_eq!(
        efi_agent_filesystem::text::read(to.as_ref(), "tmp/local").unwrap(),
        "nested"
    );
    assert!(
        !from
            .read_directory("")
            .unwrap()
            .iter()
            .any(|(name, _)| name == "dev")
    );
    namespace.rename("a", "renamed").unwrap();
    from.rename_to("value", to.as_ref(), "result").unwrap();
    assert_eq!(
        efi_agent_filesystem::text::read(namespace.as_ref(), "renamed/result").unwrap(),
        "payload"
    );
    assert!(namespace.metadata("b/value").is_err());
    efi_agent_filesystem::text::write(namespace.as_ref(), "root-file", "root bytes").unwrap();
    fs.rename_to("root-file", to.as_ref(), "root-moved")
        .unwrap();
    assert_eq!(
        efi_agent_filesystem::text::read(namespace.as_ref(), "renamed/root-moved").unwrap(),
        "root bytes"
    );
}

#[test]
fn workspace_modules_load_on_demand_and_aliases_share_the_compiled_image() {
    let workspace = Workspace::new();
    let module =
        wat::parse_str("(module (memory (export \"memory\") 1) (func (export \"_start\")))")
            .unwrap();
    fs::write(workspace.0.join("quickjs.wasm"), module).unwrap();
    let mut world = World::new();
    world.set_filesystem(Rc::new(native_fs::NativeFileSystem(workspace.0.clone())));
    world.discover_programs().unwrap();
    assert!(world.has_program("quickjs"));
    assert!(world.has_program("qjs"));
    assert!(!world.has_program("node"));
    let lookup = wat::parse_str(r#"(module
      (import "wasi_snapshot_preview1" "path_open" (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_read" (func $read (param i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (data (i32.const 100) ".efi-process")
      (data (i32.const 204) "qjs")
      (func (export "_start") (local $fd i32)
        (if (call $open (i32.const 3) (i32.const 0) (i32.const 100) (i32.const 12) (i32.const 0) (i64.const 66) (i64.const 0) (i32.const 0) (i32.const 0)) (then unreachable))
        (local.set $fd (i32.load (i32.const 0)))
        (i32.store (i32.const 200) (i32.const 8))
        (i32.store (i32.const 16) (i32.const 200))
        (i32.store (i32.const 20) (i32.const 7))
        (if (call $write (local.get $fd) (i32.const 16) (i32.const 1) (i32.const 32)) (then unreachable))
        (i32.store (i32.const 16) (i32.const 300))
        (i32.store (i32.const 20) (i32.const 8))
        (if (call $read (local.get $fd) (i32.const 16) (i32.const 1) (i32.const 32)) (then unreachable))
        (if (i32.ne (i32.load (i32.const 300)) (i32.const 1)) (then unreachable))))"#).unwrap();
    world.register("lookup", &lookup).unwrap();
    let lookup = world.spawn(vec!["lookup".into()], vec![]).unwrap();
    for _ in 0..100 {
        if !world.status(lookup).unwrap().is_live() {
            break;
        }
        world.step();
    }
    assert_eq!(world.status(lookup).unwrap(), &Status::Exited(0));
    let first = world.spawn(vec!["quickjs".into()], vec![]).unwrap();
    world.step();
    assert_eq!(world.status(first).unwrap(), &Status::Exited(0));
    fs::write(workspace.0.join("quickjs.wasm"), b"changed on disk").unwrap();
    world.discover_programs().unwrap();
    let alias = world.spawn(vec!["qjs".into()], vec![]).unwrap();
    world.step();
    assert_eq!(world.status(alias).unwrap(), &Status::Exited(0));
    fs::write(workspace.0.join("bad.wasm"), b"invalid").unwrap();
    assert_eq!(
        world.discover_programs(),
        Err(efi_agent_wasm::filesystem::FileError::Invalid)
    );
    assert!(!world.has_program("bad"));
}

#[test]
fn native_rename_preserves_both_open_files_when_replacing_a_name() {
    let workspace = Workspace::new();
    fs::create_dir(workspace.0.join("left")).unwrap();
    fs::create_dir(workspace.0.join("right")).unwrap();
    fs::write(workspace.0.join("left/source"), b"source").unwrap();
    fs::write(workspace.0.join("right/destination"), b"target").unwrap();
    workspace.run(r#"(module
      (import "wasi_snapshot_preview1" "path_open" (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "path_rename" (func $rename (param i32 i32 i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_read" (func $read (param i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_seek" (func $seek (param i32 i64 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (data (i32.const 100) "left/source")
      (data (i32.const 120) "right/destination")
      (data (i32.const 160) "SOURCE")
      (data (i32.const 170) "TARGET")
      (func $equal (param $a i32) (param $b i32)
        (if (i32.ne (local.get $a) (local.get $b)) (then unreachable)))
      (func (export "_start") (local $source i32) (local $target i32)
        (call $equal (call $open (i32.const 3) (i32.const 0) (i32.const 100) (i32.const 11) (i32.const 0) (i64.const 70) (i64.const 0) (i32.const 0) (i32.const 0)) (i32.const 0))
        (local.set $source (i32.load (i32.const 0)))
        (call $equal (call $open (i32.const 3) (i32.const 0) (i32.const 120) (i32.const 17) (i32.const 0) (i64.const 70) (i64.const 0) (i32.const 0) (i32.const 0)) (i32.const 0))
        (local.set $target (i32.load (i32.const 0)))
        (call $equal (call $rename (i32.const 3) (i32.const 100) (i32.const 11) (i32.const 3) (i32.const 65535) (i32.const 2)) (i32.const 21))
        (call $equal (call $rename (i32.const 3) (i32.const 100) (i32.const 11) (i32.const 3) (i32.const 120) (i32.const 17)) (i32.const 0))
        (i32.store (i32.const 16) (i32.const 160))
        (i32.store (i32.const 20) (i32.const 6))
        (call $equal (call $write (local.get $source) (i32.const 16) (i32.const 1) (i32.const 32)) (i32.const 0))
        (i32.store (i32.const 16) (i32.const 170))
        (call $equal (call $write (local.get $target) (i32.const 16) (i32.const 1) (i32.const 32)) (i32.const 0))
        (call $equal (call $seek (local.get $target) (i64.const 0) (i32.const 0) (i32.const 32)) (i32.const 0))
        (i32.store (i32.const 16) (i32.const 200))
        (call $equal (call $read (local.get $target) (i32.const 16) (i32.const 1) (i32.const 32)) (i32.const 0))
        (call $equal (i32.load (i32.const 200)) (i32.const 1196573012))))"#);
    assert!(!workspace.0.join("left/source").exists());
    assert_eq!(
        fs::read(workspace.0.join("right/destination")).unwrap(),
        b"SOURCE"
    );
}

#[test]
fn mkdir_creates_nested_directories_and_validates_before_effects() {
    let workspace = Workspace::new();
    fs::write(workspace.0.join("file"), b"original").unwrap();
    workspace.run(r#"(module
      (import "wasi_snapshot_preview1" "path_create_directory" (func $mkdir (param i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "path_open" (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (data (i32.const 100) "a")
      (data (i32.const 110) "a/b/")
      (data (i32.const 120) "missing/child")
      (data (i32.const 140) "file/child")
      (data (i32.const 160) "../escape")
      (data (i32.const 180) "denied")
      (func $equal (param $a i32) (param $b i32)
        (if (i32.ne (local.get $a) (local.get $b)) (then unreachable)))
      (func (export "_start")
        (call $equal (call $mkdir (i32.const 3) (i32.const 100) (i32.const 1)) (i32.const 0))
        (call $equal (call $mkdir (i32.const 3) (i32.const 100) (i32.const 1)) (i32.const 20))
        (call $equal (call $mkdir (i32.const 3) (i32.const 110) (i32.const 4)) (i32.const 0))
        (call $equal (call $mkdir (i32.const 3) (i32.const 110) (i32.const 4)) (i32.const 20))
        (call $equal (call $mkdir (i32.const 3) (i32.const 120) (i32.const 13)) (i32.const 44))
        (call $equal (call $mkdir (i32.const 3) (i32.const 140) (i32.const 10)) (i32.const 54))
        (call $equal (call $mkdir (i32.const 3) (i32.const 160) (i32.const 9)) (i32.const 76))
        (call $equal (call $mkdir (i32.const 3) (i32.const 65535) (i32.const 2)) (i32.const 21))
        (call $equal (call $mkdir (i32.const 127) (i32.const 180) (i32.const 6)) (i32.const 8))
        (call $equal (call $open (i32.const 3) (i32.const 0) (i32.const 100) (i32.const 1) (i32.const 2) (i64.const 0) (i64.const 0) (i32.const 0) (i32.const 0)) (i32.const 0))
        (call $equal (call $mkdir (i32.load (i32.const 0)) (i32.const 180) (i32.const 6)) (i32.const 76))))"#);
    assert!(workspace.0.join("a/b").is_dir());
    assert!(!workspace.0.join("missing").exists());
    assert!(!workspace.0.join("a/denied").exists());
    assert_eq!(fs::read(workspace.0.join("file")).unwrap(), b"original");
}

#[test]
fn file_writes_accept_eight_mib_and_reject_a_late_invalid_vector_without_effects() {
    let workspace = Workspace::new();
    let pattern: String = (0..=255).map(|byte| format!("\\{byte:02x}")).collect();
    workspace.run(&format!(r#"(module
      (import "wasi_snapshot_preview1" "path_open" (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 130)
      (data (i32.const 64) "large.bin")
      (data (i32.const 65536) "{pattern}")
      (func $equal (param $a i32) (param $b i32)
        (if (i32.ne (local.get $a) (local.get $b)) (then unreachable)))
      (func (export "_start") (local $size i32) (local $fd i32)
        (call $equal (call $open (i32.const 3) (i32.const 0) (i32.const 64) (i32.const 9) (i32.const 1) (i64.const 64) (i64.const 0) (i32.const 0) (i32.const 0)) (i32.const 0))
        (local.set $fd (i32.load (i32.const 0)))
        (local.set $size (i32.const 256))
        (loop $fill
          (memory.copy (i32.add (i32.const 65536) (local.get $size)) (i32.const 65536) (local.get $size))
          (local.set $size (i32.mul (local.get $size) (i32.const 2)))
          (br_if $fill (i32.lt_u (local.get $size) (i32.const 8388608))))
        (i32.store (i32.const 16) (i32.const 65536))
        (i32.store (i32.const 20) (i32.const 524289))
        (i32.store (i32.const 24) (i32.const 8519680))
        (i32.store (i32.const 28) (i32.const 1))
        (i32.store (i32.const 32) (i32.const 731))
        (call $equal (call $write (local.get $fd) (i32.const 16) (i32.const 2) (i32.const 32)) (i32.const 21))
        (call $equal (i32.load (i32.const 32)) (i32.const 731))
        (i32.store (i32.const 20) (i32.const 524289))
        (i32.store (i32.const 24) (i32.const 589825))
        (i32.store (i32.const 28) (i32.const 7864319))
        (call $equal (call $write (local.get $fd) (i32.const 16) (i32.const 2) (i32.const 32)) (i32.const 0))
        (call $equal (i32.load (i32.const 32)) (i32.const 8388608))))"#));
    let bytes = fs::read(workspace.0.join("large.bin")).unwrap();
    assert_eq!(bytes.len(), 8 * 1024 * 1024);
    for (index, byte) in bytes.iter().enumerate() {
        assert_eq!(*byte, index as u8);
    }
}
