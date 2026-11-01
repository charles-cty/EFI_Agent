use efi_agent_wasm::{Status, World};

#[test]
fn stream_metadata_and_directory_errors() {
    let module = wat::parse_str(
        r#"(module
        (import "wasi_snapshot_preview1" "fd_filestat_get" (func $stat (param i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "fd_fdstat_set_flags" (func $flags (param i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "fd_prestat_get" (func $prestat (param i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "fd_readdir" (func $dir (param i32 i32 i32 i64 i32) (result i32)))
        (memory (export "memory") 1)
        (func $equal (param $actual i32) (param $expected i32)
            (if (i32.ne (local.get $actual) (local.get $expected)) (then unreachable)))
        (func (export "_start")
            (call $equal (call $stat (i32.const 1) (i32.const 65500)) (i32.const 21))
            (call $equal (call $stat (i32.const 127) (i32.const 0)) (i32.const 8))
            (call $equal (call $stat (i32.const 1) (i32.const 65472)) (i32.const 0))
            (call $equal (i32.load8_u (i32.const 65488)) (i32.const 0))
            (call $equal (i32.load (i32.const 65496)) (i32.const 1))
            (call $equal (i32.load (i32.const 65504)) (i32.const 0))
            (call $equal (call $flags (i32.const 1) (i32.const 0)) (i32.const 0))
            (call $equal (call $flags (i32.const 1) (i32.const 4)) (i32.const 58))
            (call $equal (call $flags (i32.const 1) (i32.const 32)) (i32.const 28))
            (call $equal (call $flags (i32.const 127) (i32.const 0)) (i32.const 8))
            (call $equal (call $prestat (i32.const 1) (i32.const 0)) (i32.const 8))
            (call $equal (call $dir (i32.const 1) (i32.const 0) (i32.const 0) (i64.const 0) (i32.const 0)) (i32.const 54))
            (call $equal (call $dir (i32.const 127) (i32.const 0) (i32.const 0) (i64.const 0) (i32.const 0)) (i32.const 8))))"#,
    )
    .unwrap();
    let mut world = World::new();
    world.register("metadata", &module).unwrap();
    let pid = world.spawn(vec!["metadata".into()], vec![]).unwrap();
    for _ in 0..100 {
        if !world.status(pid).unwrap().is_live() {
            break;
        }
        world.step();
    }
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
}

#[test]
fn named_file_nonblocking_changes_are_shared_and_preserve_append_and_rights() {
    use efi_agent_wasm::filesystem::namespace::Namespace;
    use std::rc::Rc;

    for initial in [0, 1] {
        let module = format!(
            r#"(module
            (import "wasi_snapshot_preview1" "path_open" (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_fdstat_get" (func $stat (param i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_fdstat_set_flags" (func $flags (param i32 i32) (result i32)))
            (import "efi_process" "dup2" (func $dup (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 0) "file")
            (func $eq (param $a i32) (param $b i32)
                (if (i32.ne (local.get $a) (local.get $b)) (then unreachable)))
            (func (export "_start") (local $fd i32) (local $limited i32)
                (call $eq (call $open (i32.const 3) (i32.const 1) (i32.const 0) (i32.const 4)
                    (i32.const 1) (i64.const 74) (i64.const 0) (i32.const {initial}) (i32.const 8)) (i32.const 0))
                (local.set $fd (i32.load (i32.const 8)))
                (call $eq (call $dup (local.get $fd) (i32.const 20)) (i32.const 20))
                (call $eq (call $flags (i32.const 20) (i32.const {nonblock})) (i32.const 0))
                (call $eq (call $stat (local.get $fd) (i32.const 32)) (i32.const 0))
                (call $eq (i32.load16_u (i32.const 34)) (i32.const {nonblock}))
                (call $eq (call $flags (local.get $fd) (i32.const 8)) (i32.const 58))
                (call $eq (call $flags (local.get $fd) (i32.const {changed_append})) (i32.const 58))
                (call $eq (call $stat (i32.const 20) (i32.const 32)) (i32.const 0))
                (call $eq (i32.load16_u (i32.const 34)) (i32.const {nonblock}))
                (call $eq (call $flags (local.get $fd) (i32.const {initial})) (i32.const 0))
                (call $eq (call $stat (i32.const 20) (i32.const 32)) (i32.const 0))
                (call $eq (i32.load16_u (i32.const 34)) (i32.const {initial}))
                (call $eq (call $open (i32.const 3) (i32.const 1) (i32.const 0) (i32.const 4)
                    (i32.const 0) (i64.const 2) (i64.const 0) (i32.const 0) (i32.const 8)) (i32.const 0))
                (local.set $limited (i32.load (i32.const 8)))
                (call $eq (call $flags (local.get $limited) (i32.const 0)) (i32.const 0))
                (call $eq (call $flags (local.get $limited) (i32.const 4)) (i32.const 76))))"#,
            nonblock = initial | 4,
            changed_append = (initial | 4) ^ 1,
        );
        let mut world = World::new();
        world
            .set_filesystem(Rc::new(Namespace::temporary(65536, 32, 1, None)))
            .unwrap();
        world
            .register("flags", &wat::parse_str(module).unwrap())
            .unwrap();
        let pid = world.spawn(vec!["flags".into()], vec![]).unwrap();
        for _ in 0..100 {
            world.step();
        }
        assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
    }
}
