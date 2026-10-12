(module
  (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 2)
  (func (export "_start") (local $block i32) (local $cursor i32) (local $remaining i32)
    ;; A nonuniform payload makes duplication, loss and reordering detectable.
    (loop $fill
      (i32.store8 (i32.add (i32.const 1024) (local.get $cursor))
        (i32.rem_u (local.get $cursor) (i32.const 251)))
      (local.set $cursor (i32.add (local.get $cursor) (i32.const 1)))
      (br_if $fill (i32.lt_u (local.get $cursor) (i32.const 65536))))
    (loop $blocks
      (local.set $cursor (i32.const 1024))
      (local.set $remaining (i32.const 65536))
      (loop $write_all
        (i32.store (i32.const 0) (local.get $cursor))
        (i32.store (i32.const 4) (local.get $remaining))
        (if (call $write (i32.const 1) (i32.const 0) (i32.const 1) (i32.const 8)) (then unreachable))
        (if (i32.eqz (i32.load (i32.const 8))) (then unreachable))
        (local.set $cursor (i32.add (local.get $cursor) (i32.load (i32.const 8))))
        (local.set $remaining (i32.sub (local.get $remaining) (i32.load (i32.const 8))))
        (br_if $write_all (local.get $remaining)))
      (local.set $block (i32.add (local.get $block) (i32.const 1)))
      (br_if $blocks (i32.lt_u (local.get $block) (i32.const 3))))))
