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
            (call $equal (i32.load8_u (i32.const 65488)) (i32.const 2))
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
