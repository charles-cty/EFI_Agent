use efi_agent_wasm::filesystem::{
    DirectoryEntries, File, FileError, FileSystem, Metadata, OpenOptions, namespace::Namespace,
};
use efi_agent_wasm::{Status, World};
use std::{cell::Cell, rc::Rc};

struct CountedFiles {
    namespace: Rc<Namespace>,
    reads: Rc<Cell<usize>>,
    revisions: bool,
}

struct CountedFile {
    file: Box<dyn File>,
    reads: Rc<Cell<usize>>,
    revisions: bool,
}

impl FileSystem for CountedFiles {
    fn open(&self, path: &str, options: OpenOptions) -> Result<Box<dyn File>, FileError> {
        Ok(Box::new(CountedFile {
            file: self.namespace.open(path, options)?,
            reads: self.reads.clone(),
            revisions: self.revisions,
        }))
    }
    fn metadata(&self, path: &str) -> Result<Metadata, FileError> {
        self.namespace.metadata(path)
    }
    fn read_directory(&self, path: &str) -> Result<DirectoryEntries, FileError> {
        self.namespace.read_directory(path)
    }
}

impl File for CountedFile {
    fn content_revision(&mut self) -> Result<Option<u64>, FileError> {
        if self.revisions {
            self.file.content_revision()
        } else {
            Ok(None)
        }
    }
    fn read(&mut self, bytes: &mut [u8]) -> Result<usize, FileError> {
        self.reads.set(self.reads.get() + 1);
        self.file.read(bytes)
    }
    fn write(&mut self, bytes: &[u8]) -> Result<usize, FileError> {
        self.file.write(bytes)
    }
    fn seek(&mut self, offset: i64, whence: u32) -> Result<u64, FileError> {
        self.file.seek(offset, whence)
    }
    fn metadata(&mut self) -> Result<Metadata, FileError> {
        self.file.metadata()
    }
    fn flush(&mut self) -> Result<(), FileError> {
        self.file.flush()
    }
    fn resize(&mut self, size: u64) -> Result<(), FileError> {
        self.file.resize(size)
    }
}

fn module(exit: i32) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module
        (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
        (memory (export "memory") 1)
        (func (export "_start") (call $exit (i32.const {exit}))))"#
    ))
    .unwrap()
}

fn write(namespace: &Namespace, path: &str, bytes: &[u8]) {
    let mut file = namespace
        .open(
            path,
            OpenOptions {
                read: true,
                write: true,
                create: true,
                exclusive: false,
                truncate: true,
                append: false,
            },
        )
        .unwrap();
    assert_eq!(file.write(bytes).unwrap(), bytes.len());
    file.flush().unwrap();
    namespace.set_mode(path, 0o755).unwrap();
}

fn finish(world: &mut World, pid: u32, expected: i32) {
    for _ in 0..100 {
        if !world.status(pid).unwrap().is_live() {
            break;
        }
        world.step();
    }
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(expected));
    world.reap(pid).unwrap();
}

#[test]
fn new_launches_use_current_bytes_while_live_stores_keep_their_image() {
    for revisions in [false, true] {
        let namespace = Rc::new(Namespace::temporary(65536, 32, 41, None));
        let reads = Rc::new(Cell::new(0));
        let mut world = World::new();
        let first = module(7);
        let second = module(9);
        assert_eq!(first.len(), second.len());
        write(&namespace, "tool.wasm", &first);
        namespace
            .hard_link("tool.wasm", "alias.wasm", false)
            .unwrap();
        world
            .set_filesystem(Rc::new(CountedFiles {
                namespace: namespace.clone(),
                reads: reads.clone(),
                revisions,
            }))
            .unwrap();
        world.discover_programs().unwrap();
        let old = world.spawn(vec!["tool".into()], vec![]).unwrap();
        let loaded_reads = reads.get();
        let reused = world.spawn(vec!["tool".into()], vec![]).unwrap();
        assert_eq!(reads.get() == loaded_reads, revisions);
        write(&namespace, "alias.wasm", &second);
        let new = world.spawn(vec!["tool".into()], vec![]).unwrap();
        assert!(world.status(old).unwrap().is_live());
        finish(&mut world, old, 7);
        finish(&mut world, reused, 7);
        finish(&mut world, new, 9);
        let alias = world.spawn(vec!["alias".into()], vec![]).unwrap();
        finish(&mut world, alias, 9);
        write(&namespace, "replacement", &module(13));
        namespace.rename("replacement", "tool.wasm").unwrap();
        let replaced = world.spawn(vec!["tool".into()], vec![]).unwrap();
        finish(&mut world, replaced, 13);
        let alias = world.spawn(vec!["alias".into()], vec![]).unwrap();
        finish(&mut world, alias, 9);
        write(&namespace, "tool.wasm", b"\0asm\x01\0\0\0\xff");
        assert!(world.spawn(vec!["tool".into()], vec![]).is_err());
        write(&namespace, "tool.wasm", &first);
        let repaired = world.spawn(vec!["tool".into()], vec![]).unwrap();
        finish(&mut world, repaired, 7);
        namespace.unlink("tool.wasm").unwrap();
        assert!(world.spawn(vec!["tool".into()], vec![]).is_err());
        world.register("tool", &module(31)).unwrap();
        assert_eq!(
            world
                .filesystem()
                .unwrap()
                .metadata("bin/tool")
                .unwrap()
                .kind,
            efi_agent_wasm::filesystem::EntryType::File
        );
        let explicit = world.spawn(vec!["tool".into()], vec![]).unwrap();
        finish(&mut world, explicit, 31);
        assert!(namespace.take_errors().is_empty());
    }
}

#[test]
fn installed_aliases_reuse_one_current_compilation() {
    let namespace = Rc::new(Namespace::temporary(65536, 32, 41, None));
    let reads = Rc::new(Cell::new(0));
    write(&namespace, "quickjs.wasm", &module(7));
    let mut world = World::new();
    world
        .set_filesystem(Rc::new(CountedFiles {
            namespace: namespace.clone(),
            reads: reads.clone(),
            revisions: true,
        }))
        .unwrap();
    world.discover_programs().unwrap();
    let first = world.spawn(vec!["quickjs".into()], vec![]).unwrap();
    let after_load = reads.get();
    world.discover_programs().unwrap();
    assert_eq!(
        reads.get(),
        after_load,
        "Unchanged installation is not reread"
    );
    let alias = world.spawn(vec!["qjs".into()], vec![]).unwrap();
    assert_eq!(reads.get(), after_load);
    write(&namespace, "quickjs.wasm", &module(9));
    let updated = world.spawn(vec!["qjs".into()], vec![]).unwrap();
    let after_reload = reads.get();
    assert!(after_reload > after_load);
    let updated_main = world.spawn(vec!["quickjs".into()], vec![]).unwrap();
    assert_eq!(reads.get(), after_reload);
    for (pid, expected) in [(first, 7), (alias, 7), (updated, 9), (updated_main, 9)] {
        finish(&mut world, pid, expected);
    }
}
