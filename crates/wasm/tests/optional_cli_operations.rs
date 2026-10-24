use efi_agent_wasm::{
    Status, World,
    filesystem::{DirectoryEntries, File, FileError, FileSystem, Metadata, OpenOptions},
};
use std::{cell::RefCell, rc::Rc};

struct UnsupportedMutations;
impl FileSystem for UnsupportedMutations {
    fn symlink_metadata(&self, path: &str) -> Result<Metadata, FileError> {
        if matches!(path, "bin" | "dev" | "tmp" | "proc") {
            return Err(FileError::NotFound);
        }
        self.metadata(path)
    }
    fn open(&self, _: &str, _: OpenOptions) -> Result<Box<dyn File>, FileError> {
        panic!("Denied CLI operation must not open a file")
    }
    fn metadata(&self, path: &str) -> Result<Metadata, FileError> {
        match path {
            "" => Ok(Metadata::new(
                efi_agent_wasm::filesystem::EntryType::Directory,
                0,
            )),
            "new.txt" => Ok(Metadata::new(
                efi_agent_wasm::filesystem::EntryType::File,
                0,
            )),
            other => panic!("Unexpected provider lookup: {other}"),
        }
    }
    fn read_directory(&self, _: &str) -> Result<DirectoryEntries, FileError> {
        panic!("Denied CLI operation must not enumerate files")
    }
}

#[derive(Default)]
struct Links(RefCell<Vec<(String, String, bool)>>);
impl FileSystem for Links {
    fn open(&self, _: &str, _: OpenOptions) -> Result<Box<dyn File>, FileError> {
        panic!("path_link must not open regular files")
    }
    fn metadata(&self, path: &str) -> Result<Metadata, FileError> {
        if path.is_empty() || path == "dir" {
            Ok(Metadata::new(
                efi_agent_wasm::filesystem::EntryType::Directory,
                0,
            ))
        } else if path == "source" || path == "dir/source" {
            Ok(Metadata::new(
                efi_agent_wasm::filesystem::EntryType::File,
                0,
            ))
        } else {
            Err(FileError::NotFound)
        }
    }
    fn read_directory(&self, _: &str) -> Result<DirectoryEntries, FileError> {
        panic!("path_link must not enumerate directories")
    }
    fn hard_link(&self, source: &str, destination: &str, follow: bool) -> Result<(), FileError> {
        self.0
            .borrow_mut()
            .push((source.into(), destination.into(), follow));
        if destination == "exists" {
            Err(FileError::Exists)
        } else {
            Ok(())
        }
    }
}

#[test]
fn hard_link_validates_both_paths_rights_flags_and_ranges_before_provider_effects() {
    let module = wat::parse_str(r#"(module
      (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
      (import "wasi_snapshot_preview1" "path_link" (func $link (param i32 i32 i32 i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "path_open" (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_read" (func $read (param i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (data (i32.const 64) "sourcedestexistsdir../bad\00")
      (data (i32.const 400) ".efi-process")
      (global $channel (mut i32) (i32.const 0))
      (global $case (mut i32) (i32.const 0))
      (func $equal (param $a i32) (param $b i32)
        (global.set $case (i32.add (global.get $case) (i32.const 1)))
        (if (i32.ne (local.get $a) (local.get $b))
          (then (call $exit (i32.add (i32.mul (global.get $case) (i32.const 100)) (local.get $a))))))
      (func $control (param $source_fd i32) (param $destination_fd i32) (param $flags i32)
                     (param $source_len i32) (param $length i32) (param $error i32)
        (i32.store (i32.const 128) (i32.const 25))
        (i32.store (i32.const 132) (local.get $source_fd))
        (i32.store (i32.const 136) (local.get $destination_fd))
        (i32.store (i32.const 140) (local.get $flags))
        (i32.store (i32.const 144) (local.get $source_len))
        (memory.copy (i32.const 148) (i32.const 64) (i32.const 10))
        (i32.store (i32.const 16) (i32.const 128))
        (i32.store (i32.const 20) (local.get $length))
        (call $equal (call $write (global.get $channel) (i32.const 16) (i32.const 1) (i32.const 24)) (i32.const 0))
        (i32.store (i32.const 16) (i32.const 192))
        (i32.store (i32.const 20) (i32.const 8))
        (call $equal (call $read (global.get $channel) (i32.const 16) (i32.const 1) (i32.const 24)) (i32.const 0))
        (call $equal (i32.load (i32.const 192)) (local.get $error))
        (call $equal (i32.load (i32.const 196)) (i32.const 0)))
      (func (export "_start")
        (call $equal (call $link (i32.const 3) (i32.const 0) (i32.const 64) (i32.const 6) (i32.const 3) (i32.const 70) (i32.const 4)) (i32.const 0))
        (call $equal (call $link (i32.const 3) (i32.const 1) (i32.const 64) (i32.const 6) (i32.const 3) (i32.const 74) (i32.const 6)) (i32.const 20))
        (call $equal (call $link (i32.const 3) (i32.const 2) (i32.const 64) (i32.const 6) (i32.const 3) (i32.const 70) (i32.const 4)) (i32.const 28))
        (call $equal (call $link (i32.const 127) (i32.const 0) (i32.const 64) (i32.const 6) (i32.const 3) (i32.const 70) (i32.const 4)) (i32.const 8))
        (call $equal (call $link (i32.const 3) (i32.const 0) (i32.const 64) (i32.const 6) (i32.const 127) (i32.const 70) (i32.const 4)) (i32.const 8))
        (call $equal (call $link (i32.const 3) (i32.const 0) (i32.const 65535) (i32.const 2) (i32.const 3) (i32.const 70) (i32.const 4)) (i32.const 21))
        (call $equal (call $link (i32.const 3) (i32.const 0) (i32.const 64) (i32.const 6) (i32.const 3) (i32.const 65535) (i32.const 2)) (i32.const 21))
        (call $equal (call $link (i32.const 3) (i32.const 0) (i32.const 64) (i32.const 6) (i32.const 3) (i32.const 83) (i32.const 6)) (i32.const 76))
        (call $equal (call $link (i32.const 3) (i32.const 0) (i32.const 64) (i32.const 6) (i32.const 3) (i32.const 89) (i32.const 1)) (i32.const 76))
        ;; A directory with source-only rights cannot act as the destination.
        (call $equal (call $open (i32.const 3) (i32.const 0) (i32.const 80) (i32.const 3) (i32.const 2) (i64.const 2048) (i64.const 0) (i32.const 0) (i32.const 320)) (i32.const 0))
        (call $equal (call $link (i32.const 3) (i32.const 0) (i32.const 64) (i32.const 6) (i32.load (i32.const 320)) (i32.const 70) (i32.const 4)) (i32.const 76))
        (call $equal (call $link (i32.load (i32.const 320)) (i32.const 0) (i32.const 64) (i32.const 6) (i32.const 3) (i32.const 70) (i32.const 4)) (i32.const 0))
        ;; Destination-only rights cannot authorize the source.
        (call $equal (call $open (i32.const 3) (i32.const 0) (i32.const 80) (i32.const 3) (i32.const 2) (i64.const 4096) (i64.const 0) (i32.const 0) (i32.const 324)) (i32.const 0))
        (call $equal (call $link (i32.load (i32.const 324)) (i32.const 0) (i32.const 64) (i32.const 6) (i32.const 3) (i32.const 70) (i32.const 4)) (i32.const 76))
        (call $equal (call $link (i32.const 3) (i32.const 1) (i32.const 64) (i32.const 6) (i32.load (i32.const 324)) (i32.const 70) (i32.const 4)) (i32.const 0))
        (call $equal (call $open (i32.const 3) (i32.const 0) (i32.const 400) (i32.const 12) (i32.const 0) (i64.const 66) (i64.const 0) (i32.const 0) (i32.const 328)) (i32.const 0))
        (global.set $channel (i32.load (i32.const 328)))
        (call $control (i32.const 3) (i32.const 3) (i32.const 1) (i32.const 6) (i32.const 30) (i32.const 0))
        (call $control (i32.const 127) (i32.const 3) (i32.const 0) (i32.const 6) (i32.const 30) (i32.const 8))
        (call $control (i32.const 3) (i32.const 127) (i32.const 0) (i32.const 6) (i32.const 30) (i32.const 8))
        (call $control (i32.const 3) (i32.const 3) (i32.const 2) (i32.const 6) (i32.const 30) (i32.const 28))
        (call $control (i32.load (i32.const 324)) (i32.const 3) (i32.const 0) (i32.const 6) (i32.const 30) (i32.const 76))
        (call $control (i32.const 3) (i32.load (i32.const 320)) (i32.const 0) (i32.const 6) (i32.const 30) (i32.const 76))
        (call $control (i32.const 3) (i32.const 3) (i32.const 0) (i32.const 11) (i32.const 30) (i32.const -22))
        (call $control (i32.const 3) (i32.const 3) (i32.const 0) (i32.const -1) (i32.const 30) (i32.const -22))
        (call $control (i32.const 3) (i32.const 3) (i32.const 0) (i32.const 6) (i32.const 19) (i32.const -22))
        (call $control (i32.const 3) (i32.const 3) (i32.const 0) (i32.const 6) (i32.const 31) (i32.const 76))))"#).unwrap();
    let provider = Rc::new(Links::default());
    let mut world = World::new();
    world.set_filesystem(provider.clone()).unwrap();
    world.register("links", &module).unwrap();
    let pid = world.spawn(vec!["links".into()], vec![]).unwrap();
    for _ in 0..100 {
        if !world.status(pid).unwrap().is_live() {
            break;
        }
        world.step();
    }
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
    assert_eq!(
        *provider.0.borrow(),
        [
            ("source".into(), "dest".into(), false),
            ("source".into(), "exists".into(), true),
            ("dir/source".into(), "dest".into(), false),
            ("source".into(), "dir/dest".into(), true),
            ("source".into(), "dest".into(), true),
        ]
    );
}

#[test]
fn unavailable_wasi_socket_operations_validate_arguments_and_preserve_outputs() {
    let module = wat::parse_str(r#"(module
      (import "efi_net" "tcp_open" (func $open (param i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "sock_recv" (func $recv (param i32 i32 i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "sock_send" (func $send (param i32 i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "sock_shutdown" (func $shutdown (param i32 i32) (result i32)))
      (memory (export "memory") 1)
      (func $equal (param $a i32) (param $b i32)
        (if (i32.ne (local.get $a) (local.get $b)) (then unreachable)))
      (func (export "_start") (local $fd i32)
        (call $equal (call $open (i32.const 0) (i32.const 0)) (i32.const 0))
        (local.set $fd (i32.load (i32.const 0)))
        (i32.store (i32.const 65532) (i32.const 731))
        (i32.store16 (i32.const 65530) (i32.const 193))
        (call $equal (call $recv (local.get $fd) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 65532) (i32.const 65530)) (i32.const 58))
        (call $equal (call $recv (i32.const 127) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 65532) (i32.const 65530)) (i32.const 8))
        (call $equal (call $recv (i32.const 1) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 65532) (i32.const 65530)) (i32.const 57))
        (call $equal (call $recv (local.get $fd) (i32.const 0) (i32.const 0) (i32.const 4) (i32.const 65532) (i32.const 65530)) (i32.const 28))
        (call $equal (call $recv (local.get $fd) (i32.const 65536) (i32.const 1) (i32.const 0) (i32.const 65532) (i32.const 65530)) (i32.const 21))
        (call $equal (call $recv (local.get $fd) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 65532) (i32.const 65535)) (i32.const 21))
        (call $equal (call $send (local.get $fd) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 65532)) (i32.const 58))
        (call $equal (call $send (local.get $fd) (i32.const 0) (i32.const 0) (i32.const 1) (i32.const 65532)) (i32.const 28))
        (call $equal (call $send (local.get $fd) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 65533)) (i32.const 21))
        (call $equal (call $shutdown (local.get $fd) (i32.const 0)) (i32.const 28))
        (call $equal (call $shutdown (local.get $fd) (i32.const 3)) (i32.const 58))
        (call $equal (call $shutdown (i32.const 1) (i32.const 3)) (i32.const 57))
        (call $equal (i32.load (i32.const 65532)) (i32.const 731))
        (call $equal (i32.load16_u (i32.const 65530)) (i32.const 193))))"#).unwrap();
    let mut world = World::new();
    world.register("socket-errors", &module).unwrap();
    let pid = world.spawn(vec!["socket-errors".into()], vec![]).unwrap();
    for _ in 0..100 {
        if !world.status(pid).unwrap().is_live() {
            break;
        }
        world.step();
    }
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
}

struct NonLinks(RefCell<Vec<String>>);
impl FileSystem for NonLinks {
    fn symlink_metadata(&self, path: &str) -> Result<Metadata, FileError> {
        if matches!(path, "bin" | "dev" | "tmp" | "proc") {
            return Err(FileError::NotFound);
        }
        self.metadata(path)
    }
    fn open(&self, _: &str, _: OpenOptions) -> Result<Box<dyn File>, FileError> {
        panic!("readlink must not open files")
    }
    fn metadata(&self, path: &str) -> Result<Metadata, FileError> {
        self.0.borrow_mut().push(path.into());
        match path {
            "" | "file" | "dir" => Ok(Metadata::new(
                if path.is_empty() || path == "dir" {
                    efi_agent_wasm::filesystem::EntryType::Directory
                } else {
                    efi_agent_wasm::filesystem::EntryType::File
                },
                0,
            )),
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
    world.set_filesystem(provider.clone()).unwrap();
    world.register("readlink", &module).unwrap();
    let pid = world.spawn(vec!["readlink".into()], vec![]).unwrap();
    for _ in 0..100 {
        if !world.status(pid).unwrap().is_live() {
            break;
        }
        world.step();
    }
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
    let inspected = provider.0.borrow();
    for path in ["file", "dir", "missing"] {
        assert!(inspected.iter().any(|entry| entry == path));
    }
    assert!(
        inspected
            .iter()
            .all(|path| ["", "file", "dir", "missing"].contains(&path.as_str()))
    );
}

#[test]
fn unavailable_cli_mutations_and_accept_fail_without_provider_effects_or_output_changes() {
    let module = wat::parse_str(r#"(module
      (import "wasi_snapshot_preview1" "path_create_directory" (func $mkdir (param i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "path_unlink_file" (func $unlink (param i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "path_remove_directory" (func $rmdir (param i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "path_symlink" (func $symlink (param i32 i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "path_link" (func $link (param i32 i32 i32 i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_advise" (func $advise (param i32 i64 i64 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_filestat_set_times" (func $fd_times (param i32 i64 i64 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_pread" (func $pread (param i32 i32 i32 i64 i32) (result i32)))
      (import "wasi_snapshot_preview1" "fd_pwrite" (func $pwrite (param i32 i32 i32 i64 i32) (result i32)))
      (import "wasi_snapshot_preview1" "path_rename" (func $rename (param i32 i32 i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "path_filestat_set_times" (func $times (param i32 i32 i32 i32 i64 i64 i32) (result i32)))
      (import "wasi_snapshot_preview1" "sock_accept" (func $accept (param i32 i32 i32) (result i32)))
      (import "efi_net" "tcp_open" (func $open (param i32 i32) (result i32)))
      (memory (export "memory") 1)
      (data (i32.const 64) "new.txt")
      (func (export "_start")
        (if (i32.ne (call $mkdir (i32.const 3) (i32.const 64) (i32.const 7)) (i32.const 58)) (then unreachable))
        (if (i32.ne (call $unlink (i32.const 3) (i32.const 64) (i32.const 7)) (i32.const 58)) (then unreachable))
        (if (i32.ne (call $rename (i32.const 3) (i32.const 64) (i32.const 7) (i32.const 3) (i32.const 64) (i32.const 7)) (i32.const 58)) (then unreachable))
        (if (i32.ne (call $times (i32.const 3) (i32.const 0) (i32.const 64) (i32.const 7) (i64.const 0) (i64.const 0) (i32.const 0)) (i32.const 58)) (then unreachable))
        (if (i32.ne (call $mkdir (i32.const 127) (i32.const 64) (i32.const 7)) (i32.const 8)) (then unreachable))
        (if (i32.ne (call $unlink (i32.const 1) (i32.const 64) (i32.const 7)) (i32.const 54)) (then unreachable))
        (i32.store (i32.const 65532) (i32.const 71731))
        (if (i32.ne (call $rmdir (i32.const 3) (i32.const 64) (i32.const 7)) (i32.const 58)) (then unreachable))
        (if (i32.ne (call $symlink (i32.const 64) (i32.const 7) (i32.const 3) (i32.const 64) (i32.const 7)) (i32.const 58)) (then unreachable))
        (if (i32.ne (call $link (i32.const 3) (i32.const 0) (i32.const 64) (i32.const 7) (i32.const 3) (i32.const 64) (i32.const 7)) (i32.const 58)) (then unreachable))
        (if (i32.ne (call $advise (i32.const 127) (i64.const 0) (i64.const 0) (i32.const 0)) (i32.const 8)) (then unreachable))
        (if (i32.ne (call $advise (i32.const 1) (i64.const 0) (i64.const 0) (i32.const 6)) (i32.const 28)) (then unreachable))
        (if (i32.ne (call $fd_times (i32.const 3) (i64.const 0) (i64.const 0) (i32.const 0)) (i32.const 58)) (then unreachable))
        (if (i32.ne (call $fd_times (i32.const 3) (i64.const 0) (i64.const 0) (i32.const 3)) (i32.const 28)) (then unreachable))
        (if (i32.ne (call $pread (i32.const 1) (i32.const 0) (i32.const 0) (i64.const 0) (i32.const 65532)) (i32.const 70)) (then unreachable))
        (if (i32.ne (call $pwrite (i32.const 127) (i32.const 0) (i32.const 0) (i64.const 0) (i32.const 65532)) (i32.const 8)) (then unreachable))
        (if (i32.ne (call $accept (i32.const 127) (i32.const 0) (i32.const 65532)) (i32.const 8)) (then unreachable))
        (if (i32.ne (call $accept (i32.const 1) (i32.const 0) (i32.const 65532)) (i32.const 57)) (then unreachable))
        (if (call $open (i32.const 0) (i32.const 0)) (then unreachable))
        (if (i32.ne (call $accept (i32.load (i32.const 0)) (i32.const 0) (i32.const 65532)) (i32.const 58)) (then unreachable))
        (if (i32.ne (call $accept (i32.load (i32.const 0)) (i32.const 1) (i32.const 65532)) (i32.const 28)) (then unreachable))
        (if (i32.ne (call $accept (i32.load (i32.const 0)) (i32.const 0) (i32.const 65533)) (i32.const 21)) (then unreachable))
        (if (i32.ne (i32.load (i32.const 65532)) (i32.const 71731)) (then unreachable))))"#).unwrap();
    let mut world = World::new();
    world.set_filesystem(Rc::new(UnsupportedMutations)).unwrap();
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
