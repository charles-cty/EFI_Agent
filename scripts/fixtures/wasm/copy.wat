(module
  (import "wasi_snapshot_preview1" "fd_read" (func $read (param i32 i32 i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (func (export "_start") (local $cursor i32) (local $remaining i32)
    (block $eof
      (loop $copy
        (i32.store (i32.const 0) (i32.const 128))
        (i32.store (i32.const 4) (i32.const 997))
        (if (call $read (i32.const 0) (i32.const 0) (i32.const 1) (i32.const 8)) (then unreachable))
        (local.set $remaining (i32.load (i32.const 8)))
        (br_if $eof (i32.eqz (local.get $remaining)))
        (local.set $cursor (i32.const 128))
        (loop $write_all
          (i32.store (i32.const 0) (local.get $cursor))
          (i32.store (i32.const 4) (local.get $remaining))
          (if (call $write (i32.const 1) (i32.const 0) (i32.const 1) (i32.const 8)) (then unreachable))
          (if (i32.eqz (i32.load (i32.const 8))) (then unreachable))
          (local.set $cursor (i32.add (local.get $cursor) (i32.load (i32.const 8))))
          (local.set $remaining (i32.sub (local.get $remaining) (i32.load (i32.const 8))))
          (br_if $write_all (local.get $remaining)))
        (br $copy)))))
