use efi_agent_wasm::{Status, World};

#[test]
fn socket_close_on_exec_obeys_spawn_duplication_and_renumber() {
    // Put the socket at fd 0 so default standard-descriptor inheritance must
    // actually honor the flag. An unconnected socket needs no network provider.
    for (flags, action, explicit, expected) in [
        (0, "", 0, 0),
        (0x10000, "", 0, 8),
        (0x10004, "", 0, 8),
        (0x10000, "", 1, 0),
        (0x10000, "", 2, 0),
        (
            0x10000,
            "(drop (call $dup (i32.const 0) (i32.const 0)))",
            0,
            8,
        ),
        (
            0x10000,
            "(drop (call $dup (i32.const 0) (i32.const 7))) (drop (call $close (i32.const 0))) (drop (call $move (i32.const 7) (i32.const 0)))",
            0,
            0,
        ),
        (
            0x10000,
            "(drop (call $dupfd (i32.const 0) (i32.const 7))) (drop (call $close (i32.const 0))) (drop (call $move (i32.const 7) (i32.const 0)))",
            0,
            0,
        ),
        (
            0x10000,
            "(drop (call $move (i32.const 0) (i32.const 7))) (drop (call $move (i32.const 7) (i32.const 0)))",
            0,
            8,
        ),
        // Duplicating to another fd must not clear the original descriptor flag.
        (
            0x10000,
            "(drop (call $dup (i32.const 0) (i32.const 7)))",
            0,
            8,
        ),
    ] {
        let mut world = World::new();
        world.register("child", &wat::parse_str(r#"(module
          (import "wasi_snapshot_preview1" "fd_fdstat_get" (func $stat (param i32 i32) (result i32)))
          (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
          (memory (export "memory") 1)
          (func (export "_start") (call $exit (call $stat (i32.const 0) (i32.const 0)))))"#).unwrap()).unwrap();
        let spawn = if explicit == 2 {
            "(call $spawn_mapped (i32.const 32) (i32.const 6) (i32.const 0) (i32.const 0) (i32.const 64) (i32.const 3))"
        } else if explicit == 1 {
            "(call $spawn_fds (i32.const 32) (i32.const 6) (i32.const 0) (i32.const 1) (i32.const 2))"
        } else {
            "(call $spawn (i32.const 32) (i32.const 6))"
        };
        let parent = format!(
            r#"(module
          (import "efi_net" "tcp_open" (func $open (param i32 i32) (result i32)))
          (import "efi_process" "spawn" (func $spawn (param i32 i32) (result i32)))
          (import "efi_process" "spawn_fds" (func $spawn_fds (param i32 i32 i32 i32 i32) (result i32)))
          (import "efi_process" "spawn_mapped_fds" (func $spawn_mapped (param i32 i32 i32 i32 i32 i32) (result i32)))
          (import "efi_process" "waitpid" (func $wait (param i32 i32) (result i32)))
          (import "efi_process" "dup2" (func $dup (param i32 i32) (result i32)))
          (import "efi_process" "dupfd" (func $dupfd (param i32 i32) (result i32)))
          (import "wasi_snapshot_preview1" "fd_close" (func $close (param i32) (result i32)))
          (import "wasi_snapshot_preview1" "fd_renumber" (func $move (param i32 i32) (result i32)))
          (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
          (memory (export "memory") 1)
          (data (i32.const 32) "child\00")
          (data (i32.const 64) "\00\00\00\00\00\00\00\00\01\00\00\00\01\00\00\00\02\00\00\00\02\00\00\00")
          (func (export "_start") (local $pid i32)
            (if (call $close (i32.const 0)) (then unreachable))
            (if (call $open (i32.const {flags}) (i32.const 16)) (then unreachable))
            (if (i32.load (i32.const 16)) (then unreachable))
            {action}
            (local.set $pid {spawn})
            (if (i32.ne (call $wait (local.get $pid) (i32.const 20)) (local.get $pid)) (then unreachable))
            (call $exit (i32.load (i32.const 20)))))"#
        );
        world
            .register("parent", &wat::parse_str(parent).unwrap())
            .unwrap();
        let pid = world.spawn(vec!["parent".into()], vec![]).unwrap();
        for _ in 0..1000 {
            if !world.status(pid).unwrap().is_live() {
                break;
            }
            assert!(world.step().is_some());
        }
        assert_eq!(
            world.status(pid).unwrap(),
            &Status::Exited(expected),
            "flags={flags}, action={action}, explicit={explicit}"
        );
    }
}

#[test]
fn close_on_exec_filters_all_stream_kinds_and_close_reuse_discards_flags() {
    for (kind, setup) in [
        ("console input", ""),
        (
            "console output",
            "(drop (call $dup (i32.const 1) (i32.const 0)))",
        ),
        (
            "snapshot",
            "(drop (call $close (i32.const 0))) (drop (call $input (i32.const 0) (i32.const 0)))",
        ),
        (
            "pipe",
            "(drop (call $close (i32.const 0))) (drop (call $pipe (i32.const 24)))",
        ),
    ] {
        for clear in [false, true] {
            let mut world = World::new();
            let child = r#"(module
              (import "efi_process" "get_fd_flags" (func $get (param i32) (result i32)))
              (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
              (memory (export "memory") 1)
              (func (export "_start") (call $exit (call $get (i32.const 0)))))"#;
            world
                .register("child", &wat::parse_str(child).unwrap())
                .unwrap();
            let clear_flag = if clear {
                "(if (call $set (i32.const 0) (i32.const 0)) (then unreachable))"
            } else {
                ""
            };
            let parent = format!(
                r#"(module
              (import "efi_process" "get_fd_flags" (func $get (param i32) (result i32)))
              (import "efi_process" "set_fd_flags" (func $set (param i32 i32) (result i32)))
              (import "efi_process" "dup2" (func $dup (param i32 i32) (result i32)))
              (import "efi_process" "pipe" (func $pipe (param i32) (result i32)))
              (import "efi_process" "input_snapshot" (func $input (param i32 i32) (result i32)))
              (import "efi_process" "spawn" (func $spawn (param i32 i32) (result i32)))
              (import "efi_process" "waitpid" (func $wait (param i32 i32) (result i32)))
              (import "wasi_snapshot_preview1" "fd_close" (func $close (param i32) (result i32)))
              (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
              (memory (export "memory") 1)
              (data (i32.const 64) "child\00")
              (func (export "_start") (local $pid i32)
                {setup}
                (if (call $get (i32.const 0)) (then unreachable))
                (if (call $set (i32.const 0) (i32.const 1)) (then unreachable))
                (if (i32.ne (call $set (i32.const 0) (i32.const 2)) (i32.const -22)) (then unreachable))
                (if (i32.ne (call $get (i32.const 0)) (i32.const 1)) (then unreachable))
                {clear_flag}
                (local.set $pid (call $spawn (i32.const 64) (i32.const 6)))
                (if (i32.ne (call $wait (local.get $pid) (i32.const 16)) (local.get $pid)) (then unreachable))
                (if (call $close (i32.const 0)) (then unreachable))
                (if (i32.ne (call $get (i32.const 0)) (i32.const -9)) (then unreachable))
                (if (call $input (i32.const 0) (i32.const 0)) (then unreachable))
                (if (call $get (i32.const 0)) (then unreachable))
                (call $exit (i32.load (i32.const 16)))))"#
            );
            world
                .register("parent", &wat::parse_str(parent).unwrap())
                .unwrap();
            let pid = world.spawn(vec!["parent".into()], vec![]).unwrap();
            for _ in 0..1000 {
                if !world.status(pid).unwrap().is_live() {
                    break;
                }
                assert!(world.step().is_some());
            }
            assert_eq!(
                world.status(pid).unwrap(),
                &Status::Exited(if clear { 0 } else { -9 }),
                "{kind}, clear={clear}"
            );
        }
    }
}
