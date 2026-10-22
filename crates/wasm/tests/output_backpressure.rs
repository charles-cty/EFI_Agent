use efi_agent_wasm::{MAX_OUTPUT_BYTES, Status, World};

#[test]
fn oversized_stream_writes_validate_all_vectors_and_resume_after_host_drain() {
    for fd in [1, 2] {
        let module = wat::parse_str(format!(
            r#"(module
              (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
              (memory (export "memory") 10)
              (func $equal (param $a i32) (param $b i32)
                (if (i32.ne (local.get $a) (local.get $b)) (then unreachable)))
              (func (export "_start")
                (memory.fill (i32.const 1024) (i32.const 65) (i32.const 600000))
                (i32.store (i32.const 64) (i32.const 1024))
                (i32.store (i32.const 68) (i32.const 600000))
                (i32.store (i32.const 72) (i32.const 655360))
                (i32.store (i32.const 76) (i32.const 1))
                (i32.store (i32.const 32) (i32.const 731))
                (call $equal (call $write (i32.const {fd}) (i32.const 64) (i32.const 2) (i32.const 32)) (i32.const 21))
                (call $equal (i32.load (i32.const 32)) (i32.const 731))
                (call $equal (call $write (i32.const {fd}) (i32.const 64) (i32.const 1) (i32.const 32)) (i32.const 0))
                (call $equal (i32.load (i32.const 32)) (i32.const 524288))
                (i32.store (i32.const 64) (i32.const 525312))
                (i32.store (i32.const 68) (i32.const 0))
                (call $equal (call $write (i32.const {fd}) (i32.const 64) (i32.const 1) (i32.const 32)) (i32.const 0))
                (call $equal (i32.load (i32.const 32)) (i32.const 0))
                (i32.store (i32.const 68) (i32.const 75712))
                (call $equal (call $write (i32.const {fd}) (i32.const 64) (i32.const 1) (i32.const 32)) (i32.const 0))
                (call $equal (i32.load (i32.const 32)) (i32.const 75712))))"#
        ))
        .unwrap();
        let mut world = World::new();
        world.register("output", &module).unwrap();
        let pid = world.spawn(vec!["output".into()], vec![]).unwrap();
        for _ in 0..2000 {
            world.step();
            if matches!(world.status(pid).unwrap(), Status::Writing(value) if *value == fd) {
                break;
            }
        }
        assert_eq!(world.status(pid).unwrap(), &Status::Writing(fd));
        let (stdout, stderr) = world.take_output(pid).unwrap();
        let (queued, other) = if fd == 1 {
            (stdout, stderr)
        } else {
            (stderr, stdout)
        };
        assert_eq!(queued, vec![b'A'; MAX_OUTPUT_BYTES]);
        assert!(other.is_empty());
        for _ in 0..2000 {
            world.step();
            if !world.status(pid).unwrap().is_live() {
                break;
            }
        }
        assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
        let (stdout, stderr) = world.take_output(pid).unwrap();
        let (remaining, other) = if fd == 1 {
            (stdout, stderr)
        } else {
            (stderr, stdout)
        };
        assert_eq!(remaining, vec![b'A'; 600000 - MAX_OUTPUT_BYTES]);
        assert!(other.is_empty());
    }
}
