use efi_agent_wasm::{Status, World};

fn world() -> World {
    let mut world = World::new();
    let copy = wat::parse_str(r#"(module
        (import "wasi_snapshot_preview1" "fd_read" (func $read (param i32 i32 i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 1)
        (func (export "_start")
            (i32.store (i32.const 0) (i32.const 64))
            (i32.store (i32.const 4) (i32.const 17))
            (loop $copy
                (if (call $read (i32.const 0) (i32.const 0) (i32.const 1) (i32.const 8)) (then unreachable))
                (if (i32.load (i32.const 8)) (then
                    (i32.store (i32.const 4) (i32.load (i32.const 8)))
                    (if (call $write (i32.const 1) (i32.const 0) (i32.const 1) (i32.const 12)) (then unreachable))
                    (br $copy))))))"#).unwrap();
    world.register("copy", &copy).unwrap();
    world
}

#[test]
fn live_input_blocks_until_supplied_and_terminal_capture_survives_reap() {
    for terminal in [false, true] {
        let mut world = world();
        let pid = world.spawn(vec!["copy".into()], vec![]).unwrap();
        let input = world.process_input(pid, terminal).unwrap();
        for _ in 0..10 {
            world.step();
        }
        assert!(world.status(pid).unwrap().is_live());
        assert!(world.take_output(pid).unwrap().0.is_empty());
        let expected = b"one\0\xfftwo\nthree";
        input.write(&expected[..7]).unwrap();
        for _ in 0..10 {
            world.step();
        }
        input.write(&expected[7..]).unwrap();
        input.close();
        assert!(input.write(b"after EOF").is_err());
        for _ in 0..100 {
            if !world.status(pid).unwrap().is_live() {
                break;
            }
            world.step();
        }
        assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
        let output = world.take_output(pid).unwrap();
        world.reap(pid).unwrap();
        let actual = if terminal {
            input.take_output()
        } else {
            output.0
        };
        assert_eq!(actual, expected);
    }
}

#[test]
fn cancellation_of_one_root_preserves_an_independent_input_session() {
    let mut world = world();
    let first = world.spawn(vec!["copy".into()], vec![]).unwrap();
    let second = world.spawn(vec!["copy".into()], vec![]).unwrap();
    let _first_input = world.process_input(first, false).unwrap();
    let second_input = world.process_input(second, false).unwrap();
    for _ in 0..10 {
        world.step();
    }
    world.cancel(first).unwrap();
    world.reap(first).unwrap();
    second_input.write(b"survived").unwrap();
    second_input.close();
    for _ in 0..100 {
        if !world.status(second).unwrap().is_live() {
            break;
        }
        world.step();
    }
    assert_eq!(world.status(second).unwrap(), &Status::Exited(0));
    assert_eq!(world.take_output(second).unwrap().0, b"survived");
}
