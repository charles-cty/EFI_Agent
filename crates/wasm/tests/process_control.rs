use efi_agent_wasm::filesystem::{
    DirectoryEntries, File, FileError, FileSystem, Metadata, OpenOptions,
};
use efi_agent_wasm::{Status, World};
use std::rc::Rc;

struct Unused;
impl FileSystem for Unused {
    fn symlink_metadata(&self, path: &str) -> Result<Metadata, FileError> {
        if path == "bin" {
            return Err(FileError::NotFound);
        }
        self.metadata(path)
    }
    fn open(&self, _: &str, _: OpenOptions) -> Result<Box<dyn File>, FileError> {
        panic!("Virtual endpoint must not open host files")
    }
    fn metadata(&self, path: &str) -> Result<Metadata, FileError> {
        if path.is_empty() || path == "dir" {
            return Ok(Metadata::new(
                efi_agent_wasm::filesystem::EntryType::Directory,
                0,
            ));
        }
        panic!("Virtual endpoint must not inspect host files")
    }
    fn read_directory(&self, _: &str) -> Result<DirectoryEntries, FileError> {
        panic!("Virtual endpoint must not enumerate files")
    }
}

fn run(body: &str) {
    run_with(body, Rc::new(Unused));
}

fn run_with(body: &str, filesystem: Rc<dyn FileSystem>) {
    let module = wat::parse_str(format!(r#"(module
      (import "wasi_snapshot_preview1" "path_open" (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_read" (func $read (param i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_close" (func $close (param i32) (result i32)))
      (memory (export "memory") 1)
      (data (i32.const 64) ".efi-process")
      (data (i32.const 80) "dir")
      (func $equal (param $a i32) (param $b i32)
        (if (i32.ne (local.get $a) (local.get $b)) (then unreachable)))
      (func $send (param $size i32) (result i32)
        (i32.store (i32.const 128) (i32.const 256))
        (i32.store (i32.const 132) (local.get $size))
        (call $write (i32.load (i32.const 32)) (i32.const 128) (i32.const 1) (i32.const 48)))
      (func $response (result i32)
        (i32.store (i32.const 128) (i32.const 512))
        (i32.store (i32.const 132) (i32.const 8))
        (call $equal (call $read (i32.load (i32.const 32)) (i32.const 128) (i32.const 1) (i32.const 48)) (i32.const 0))
        (call $equal (i32.load (i32.const 48)) (i32.const 8))
        (i32.load (i32.const 512)))
      (func (export "_start")
        (call $equal (call $open (i32.const 3) (i32.const 0) (i32.const 64) (i32.const 12) (i32.const 0) (i64.const 66) (i64.const 0) (i32.const 0) (i32.const 32)) (i32.const 0))
        {body}))"#)).unwrap();
    let mut world = World::new();
    world.set_filesystem(filesystem);
    world.register("control", &module).unwrap();
    let pid = world.spawn(vec!["control".into()], vec![]).unwrap();
    for _ in 0..1000 {
        if !world.status(pid).unwrap().is_live() {
            break;
        }
        world.step();
    }
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
}

#[test]
fn directory_snapshot_rejects_bad_fds_types_and_missing_rights_before_enumeration() {
    run(r#"
      (i32.store (i32.const 256) (i32.const 18))
      (call $equal (call $send (i32.const 4)) (i32.const 0))
      (call $equal (call $response) (i32.const -22))
      (i32.store (i32.const 260) (i32.const 128))
      (call $equal (call $send (i32.const 8)) (i32.const 0))
      (call $equal (call $response) (i32.const 8))
      (i32.store (i32.const 260) (i32.const 1))
      (call $equal (call $send (i32.const 8)) (i32.const 0))
      (call $equal (call $response) (i32.const 54))
      (call $equal (call $open (i32.const 3) (i32.const 0) (i32.const 80) (i32.const 3) (i32.const 2) (i64.const 0) (i64.const 0) (i32.const 0) (i32.const 40)) (i32.const 0))
      (i32.store (i32.const 260) (i32.load (i32.const 40)))
      (call $equal (call $send (i32.const 8)) (i32.const 0))
      (call $equal (call $response) (i32.const 76))
      (call $equal (call $close (i32.load (i32.const 40))) (i32.const 0))
      (i32.store (i32.const 256) (i32.const 1))
      (call $equal (call $send (i32.const 4)) (i32.const 0))
      (call $equal (call $response) (i32.const 5))
    "#);
}

#[test]
fn directory_snapshot_encodes_utf8_bytes_and_releases_private_descriptors() {
    struct Names;
    impl FileSystem for Names {
        fn symlink_metadata(&self, path: &str) -> Result<Metadata, FileError> {
            if path == "bin" {
                return Err(FileError::NotFound);
            }
            self.metadata(path)
        }
        fn open(&self, _: &str, _: OpenOptions) -> Result<Box<dyn File>, FileError> {
            panic!("Snapshot does not open files")
        }
        fn metadata(&self, path: &str) -> Result<Metadata, FileError> {
            assert!(path.is_empty() || path == "dir");
            Ok(Metadata::new(
                efi_agent_wasm::filesystem::EntryType::Directory,
                0,
            ))
        }
        fn read_directory(&self, path: &str) -> Result<DirectoryEntries, FileError> {
            assert_eq!(path, "dir");
            let mut entries = DirectoryEntries::new();
            entries.push("a".into(), efi_agent_wasm::filesystem::EntryType::File)?;
            entries.push(
                "中".into(),
                efi_agent_wasm::filesystem::EntryType::Directory,
            )?;
            Ok(entries)
        }
    }
    run_with(
        r#"
      (call $equal (call $open (i32.const 3) (i32.const 0) (i32.const 80) (i32.const 3) (i32.const 2) (i64.const 16384) (i64.const 0) (i32.const 0) (i32.const 40)) (i32.const 0))
      (i32.store (i32.const 256) (i32.const 18))
      (i32.store (i32.const 260) (i32.load (i32.const 40)))
      (call $equal (call $send (i32.const 8)) (i32.const 0))
      (call $equal (call $response) (i32.const 0))
      (call $equal (i32.load (i32.const 516)) (i32.const 6))
      (i32.store (i32.const 256) (i32.const 4))
      (i32.store (i32.const 260) (i32.const 6))
      (call $equal (call $send (i32.const 8)) (i32.const 0))
      (call $equal (call $response) (i32.const 1))
      (i32.store (i32.const 128) (i32.const 768))
      (i32.store (i32.const 132) (i32.const 32))
      (call $equal (call $read (i32.const 6) (i32.const 128) (i32.const 1) (i32.const 48)) (i32.const 0))
      (call $equal (i32.load (i32.const 48)) (i32.const 12))
      (call $equal (i32.load (i32.const 768)) (i32.const 1))
      (call $equal (i32.load8_u (i32.const 772)) (i32.const 97))
      (call $equal (i32.load (i32.const 773)) (i32.const 3))
      (call $equal (i32.load8_u (i32.const 777)) (i32.const 228))
      (call $equal (i32.load8_u (i32.const 778)) (i32.const 184))
      (call $equal (i32.load8_u (i32.const 779)) (i32.const 173))
      (call $equal (call $read (i32.const 6) (i32.const 128) (i32.const 1) (i32.const 48)) (i32.const 0))
      (call $equal (i32.load (i32.const 48)) (i32.const 0))
      (call $equal (call $close (i32.const 6)) (i32.const 0))
      (i32.store (i32.const 256) (i32.const 1))
      (call $equal (call $send (i32.const 4)) (i32.const 0))
      (call $equal (call $response) (i32.const 6))
    "#,
        Rc::new(Names),
    );
}

#[test]
fn malformed_packets_preserve_pid_and_endpoint_and_responses_are_bounded() {
    run(r#"
      (call $equal (call $send (i32.const 0)) (i32.const 0))
      (call $equal (call $response) (i32.const -22))
      (i32.store (i32.const 256) (i32.const 6))
      (call $equal (call $send (i32.const 4)) (i32.const 0))
      (call $equal (call $send (i32.const 4)) (i32.const 6))
      (call $equal (call $response) (i32.const 1))
      (i32.store (i32.const 256) (i32.const 3))
      (i32.store (i32.const 260) (i32.const 1))
      (i32.store (i32.const 264) (i32.load (i32.const 32)))
      (call $equal (call $send (i32.const 12)) (i32.const 0))
      (call $equal (call $response) (i32.const -22))
      (i32.store (i32.const 256) (i32.const 6))
      (call $equal (call $send (i32.const 4)) (i32.const 0))
      (call $equal (call $response) (i32.const 1))
    "#);
}

#[test]
fn invalid_spawn_frames_fail_before_allocating_a_child_or_descriptors() {
    run(r#"
      (i32.store (i32.const 256) (i32.const 9))
      (i32.store (i32.const 260) (i32.const -1))
      (call $equal (call $send (i32.const 16)) (i32.const 0))
      (call $equal (call $response) (i32.const -7))
      (i32.store (i32.const 260) (i32.const 0))
      (i32.store (i32.const 268) (i32.const 1))
      (i32.store (i32.const 272) (i32.const 3))
      (i32.store (i32.const 276) (i32.const 1))
      (call $equal (call $send (i32.const 24)) (i32.const 0))
      (call $equal (call $response) (i32.const -9))
      (i32.store (i32.const 256) (i32.const 1))
      (call $equal (call $send (i32.const 4)) (i32.const 0))
      (call $equal (call $response) (i32.const 5))
      (call $equal (i32.load (i32.const 516)) (i32.const 6))
    "#);
}

#[test]
fn mode_packets_validate_layout_flags_and_descriptors_before_provider_effects() {
    run(r#"
      (i32.store (i32.const 256) (i32.const 11))
      (call $equal (call $send (i32.const 4)) (i32.const 0))
      (call $equal (call $response) (i32.const -22))
      (i32.store (i32.const 260) (i32.const 3))
      (i32.store (i32.const 264) (i32.const 1))
      (call $equal (call $send (i32.const 12)) (i32.const 0))
      (call $equal (call $response) (i32.const -22))
      (i32.store8 (i32.const 268) (i32.const 120))
      (i32.store (i32.const 264) (i32.const 2))
      (call $equal (call $send (i32.const 13)) (i32.const 0))
      (call $equal (call $response) (i32.const 28))
      (i32.store (i32.const 264) (i32.const 1))
      (i32.store (i32.const 260) (i32.const 0))
      (call $equal (call $send (i32.const 13)) (i32.const 0))
      (call $equal (call $response) (i32.const 54))
      (i32.store (i32.const 256) (i32.const 14))
      (i32.store (i32.const 260) (i32.const 128))
      (i32.store (i32.const 264) (i32.const 384))
      (call $equal (call $send (i32.const 12)) (i32.const 0))
      (call $equal (call $response) (i32.const 8))
      (i32.store (i32.const 260) (i32.const 3))
      (i32.store (i32.const 264) (i32.const 4096))
      (call $equal (call $send (i32.const 12)) (i32.const 0))
      (call $equal (call $response) (i32.const 28))
      (i32.store (i32.const 256) (i32.const 15))
      (i32.store (i32.const 260) (i32.const 63))
      (call $equal (call $send (i32.const 8)) (i32.const 0))
      (call $equal (call $response) (i32.const 18))
      (i32.store (i32.const 260) (i32.const 18))
      (call $equal (call $send (i32.const 8)) (i32.const 0))
      (call $equal (call $response) (i32.const 63))
    "#);
}

#[test]
fn creation_mode_scope_preserves_previous_values_and_rejects_invalid_bits() {
    run(r#"
      (i32.store (i32.const 256) (i32.const 19))
      (i32.store (i32.const 260) (i32.const 4096))
      (call $equal (call $send (i32.const 8)) (i32.const 0))
      (call $equal (call $response) (i32.const -22))
      (i32.store (i32.const 260) (i32.const 256))
      (call $equal (call $send (i32.const 8)) (i32.const 0))
      (call $equal (call $response) (i32.const 0))
      (call $equal (i32.load (i32.const 516)) (i32.const -1))
      (i32.store (i32.const 260) (i32.const 416))
      (call $equal (call $send (i32.const 8)) (i32.const 0))
      (call $equal (call $response) (i32.const 0))
      (call $equal (i32.load (i32.const 516)) (i32.const 256))
      (i32.store (i32.const 260) (i32.const -1))
      (call $equal (call $send (i32.const 8)) (i32.const 0))
      (call $equal (call $response) (i32.const 0))
      (call $equal (i32.load (i32.const 516)) (i32.const 416))
      (call $equal (call $send (i32.const 8)) (i32.const 0))
      (call $equal (call $response) (i32.const 0))
      (call $equal (i32.load (i32.const 516)) (i32.const -1))
    "#);
}

#[test]
fn cwd_control_snapshot_is_private_and_rejects_invalid_paths_before_filesystem_access() {
    run(r#"
      (i32.store (i32.const 256) (i32.const 16))
      (call $equal (call $send (i32.const 4)) (i32.const 0))
      (call $equal (call $response) (i32.const 5))
      (i32.store (i32.const 256) (i32.const 4))
      (i32.store (i32.const 260) (i32.const 5))
      (call $equal (call $send (i32.const 8)) (i32.const 0))
      (call $equal (call $response) (i32.const 1))
      (i32.store (i32.const 256) (i32.const 17))
      (call $equal (call $send (i32.const 4)) (i32.const 0))
      (call $equal (call $response) (i32.const -22))
      (i32.store (i32.const 260) (i32.const 120))
      (call $equal (call $send (i32.const 5)) (i32.const 0))
      (call $equal (call $response) (i32.const -22))
      (i32.store (i32.const 260) (i32.const 47))
      (call $equal (call $send (i32.const 6)) (i32.const 0))
      (call $equal (call $response) (i32.const -22))
    "#);
}
