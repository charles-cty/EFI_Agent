use efi_agent_wasm::{
    Status, World,
    filesystem::{DirectoryEntries, File, FileError, FileSystem, Metadata, OpenOptions},
};
use std::{cell::RefCell, rc::Rc};

struct Untouched;
impl FileSystem for Untouched {
    fn open(&self, _: &str, _: OpenOptions) -> Result<Box<dyn File>, FileError> {
        panic!("Denied CLI operation must not open a file")
    }
    fn metadata(&self, _: &str) -> Result<Metadata, FileError> {
        panic!("Denied CLI operation must not inspect a file")
    }
    fn read_directory(&self, _: &str) -> Result<DirectoryEntries, FileError> {
        panic!("Denied CLI operation must not enumerate files")
    }
}

struct NonLinks(RefCell<Vec<String>>);
impl FileSystem for NonLinks {
    fn open(&self, _: &str, _: OpenOptions) -> Result<Box<dyn File>, FileError> {
        panic!("readlink must not open files")
    }
    fn metadata(&self, path: &str) -> Result<Metadata, FileError> {
        self.0.borrow_mut().push(path.into());
        match path {
            "file" | "dir" => Ok(Metadata {
                directory: path == "dir",
                size: 0,
            }),
            "missing" => Err(FileError::NotFound),
            _ => panic!("Unexpected provider path: {path}"),
        }
    }
    fn read_directory(&self, _: &str) -> Result<DirectoryEntries, FileError> {
        panic!("readlink must not enumerate directories")
    }
}

#[test]
fn readlink_distinguishes_nonlinks_missing_paths_and_validation_without_output_changes() {
    let module = wat::parse_str(r#"(module
      (import "wasi_snapshot_preview1" "path_readlink" (func $readlink (param i32 i32 i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "path_open" (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (data (i32.const 64) "filedirmissing../file")
      (func (export "_start")
        (i32.store (i32.const 65532) (i32.const 79717))
        (i32.store8 (i32.const 256) (i32.const 193))
        (if (i32.ne (call $readlink (i32.const 3) (i32.const 64) (i32.const 4) (i32.const 256) (i32.const 1) (i32.const 65532)) (i32.const 28)) (then unreachable))
        (if (i32.ne (call $readlink (i32.const 3) (i32.const 68) (i32.const 3) (i32.const 256) (i32.const 0) (i32.const 65532)) (i32.const 28)) (then unreachable))
        (if (i32.ne (call $readlink (i32.const 3) (i32.const 71) (i32.const 7) (i32.const 256) (i32.const 1) (i32.const 65532)) (i32.const 44)) (then unreachable))
        (if (i32.ne (call $readlink (i32.const 127) (i32.const 64) (i32.const 4) (i32.const 256) (i32.const 1) (i32.const 65532)) (i32.const 8)) (then unreachable))
        (if (i32.ne (call $readlink (i32.const 1) (i32.const 64) (i32.const 4) (i32.const 256) (i32.const 1) (i32.const 65532)) (i32.const 54)) (then unreachable))
        (if (i32.ne (call $readlink (i32.const 3) (i32.const 64) (i32.const 4) (i32.const 65535) (i32.const 2) (i32.const 65532)) (i32.const 21)) (then unreachable))
        (if (i32.ne (call $readlink (i32.const 3) (i32.const 64) (i32.const 4) (i32.const 65535) (i32.const 1) (i32.const 65533)) (i32.const 21)) (then unreachable))
        (if (i32.ne (call $readlink (i32.const 3) (i32.const 65536) (i32.const 1) (i32.const 256) (i32.const 1) (i32.const 65532)) (i32.const 21)) (then unreachable))
        (if (i32.ne (call $readlink (i32.const 3) (i32.const 78) (i32.const 7) (i32.const 256) (i32.const 1) (i32.const 65532)) (i32.const 76)) (then unreachable))
        (if (call $open (i32.const 3) (i32.const 0) (i32.const 68) (i32.const 3) (i32.const 2) (i64.const 0) (i64.const 0) (i32.const 0) (i32.const 320)) (then unreachable))
        (if (i32.ne (call $readlink (i32.load (i32.const 320)) (i32.const 64) (i32.const 4) (i32.const 256) (i32.const 1) (i32.const 65532)) (i32.const 76)) (then unreachable))
        (if (i32.ne (i32.load (i32.const 65532)) (i32.const 79717)) (then unreachable))
        (if (i32.ne (i32.load8_u (i32.const 256)) (i32.const 193)) (then unreachable))))"#).unwrap();
    let provider = Rc::new(NonLinks(RefCell::new(Vec::new())));
    let mut world = World::new();
    world.set_filesystem(provider.clone());
    world.register("readlink", &module).unwrap();
    let pid = world.spawn(vec!["readlink".into()], vec![]).unwrap();
    for _ in 0..100 {
        if !world.status(pid).unwrap().is_live() {
            break;
        }
        world.step();
    }
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
    assert_eq!(*provider.0.borrow(), ["file", "dir", "missing", "dir"]);
}

#[test]
fn unavailable_cli_mutations_and_accept_fail_without_provider_effects_or_output_changes() {
    let module = wat::parse_str(r#"(module
      (import "wasi_snapshot_preview1" "path_create_directory" (func $mkdir (param i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "path_unlink_file" (func $unlink (param i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "path_rename" (func $rename (param i32 i32 i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "path_filestat_set_times" (func $times (param i32 i32 i32 i32 i64 i64 i32) (result i32)))
      (import "wasi_snapshot_preview1" "sock_accept" (func $accept (param i32 i32 i32) (result i32)))
      (import "efi_net" "tcp_open" (func $open (param i32 i32) (result i32)))
      (memory (export "memory") 1)
      (data (i32.const 64) "new.txt")
      (func (export "_start")
        (if (i32.ne (call $mkdir (i32.const 3) (i32.const 64) (i32.const 7)) (i32.const 76)) (then unreachable))
        (if (i32.ne (call $unlink (i32.const 3) (i32.const 64) (i32.const 7)) (i32.const 76)) (then unreachable))
        (if (i32.ne (call $rename (i32.const 3) (i32.const 64) (i32.const 7) (i32.const 3) (i32.const 64) (i32.const 7)) (i32.const 76)) (then unreachable))
        (if (i32.ne (call $times (i32.const 3) (i32.const 0) (i32.const 64) (i32.const 7) (i64.const 0) (i64.const 0) (i32.const 0)) (i32.const 76)) (then unreachable))
        (if (i32.ne (call $mkdir (i32.const 127) (i32.const 64) (i32.const 7)) (i32.const 8)) (then unreachable))
        (if (i32.ne (call $unlink (i32.const 1) (i32.const 64) (i32.const 7)) (i32.const 54)) (then unreachable))
        (i32.store (i32.const 65532) (i32.const 71731))
        (if (i32.ne (call $accept (i32.const 127) (i32.const 0) (i32.const 65532)) (i32.const 8)) (then unreachable))
        (if (i32.ne (call $accept (i32.const 1) (i32.const 0) (i32.const 65532)) (i32.const 57)) (then unreachable))
        (if (call $open (i32.const 0) (i32.const 0)) (then unreachable))
        (if (i32.ne (call $accept (i32.load (i32.const 0)) (i32.const 0) (i32.const 65532)) (i32.const 58)) (then unreachable))
        (if (i32.ne (call $accept (i32.load (i32.const 0)) (i32.const 1) (i32.const 65532)) (i32.const 28)) (then unreachable))
        (if (i32.ne (call $accept (i32.load (i32.const 0)) (i32.const 0) (i32.const 65533)) (i32.const 21)) (then unreachable))
        (if (i32.ne (i32.load (i32.const 65532)) (i32.const 71731)) (then unreachable))))"#).unwrap();
    let mut world = World::new();
    world.set_filesystem(Rc::new(Untouched));
    world.register("optional", &module).unwrap();
    let pid = world.spawn(vec!["optional".into()], vec![]).unwrap();
    for _ in 0..100 {
        if !world.status(pid).unwrap().is_live() {
            break;
        }
        world.step();
    }
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
}
