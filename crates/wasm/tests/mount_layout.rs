use efi_agent_wasm::{
    Status, World,
    filesystem::{
        DirectoryEntries, EntryType, File, FileError, FileSystem, Metadata, MountError,
        OpenOptions, VirtualFileSystem, namespace::Namespace, text,
    },
};
use std::{cell::RefCell, rc::Rc};

fn workspace() -> Rc<Namespace> {
    Rc::new(Namespace::temporary(1024 * 1024, 128, 0, None))
}

#[test]
fn reserved_mount_collisions_preserve_entries_and_the_previous_root() {
    for name in ["dev", "tmp", "proc"] {
        for kind in ["file", "empty-directory", "directory", "dangling-link"] {
            let previous = workspace();
            let mut world = World::new();
            world.set_filesystem(previous).unwrap();
            let root = world.filesystem().unwrap();
            text::write(root.as_ref(), "tmp/retained", "old temporary bytes").unwrap();
            let candidate = workspace();
            match kind {
                "file" => text::write(candidate.as_ref(), name, "workspace bytes").unwrap(),
                "empty-directory" => candidate.create_directory(name).unwrap(),
                "directory" => {
                    candidate.create_directory(name).unwrap();
                    text::write(candidate.as_ref(), &format!("{name}/child"), "child bytes")
                        .unwrap();
                }
                "dangling-link" => candidate.symlink("absent", name).unwrap(),
                _ => unreachable!(),
            }
            let before = candidate.symlink_metadata(name).unwrap();
            assert_eq!(
                world.set_filesystem(candidate.clone()),
                Err(MountError {
                    path: format!("/{name}"),
                    error: FileError::Exists
                }),
                "{name} {kind}"
            );
            assert!(Rc::ptr_eq(&root, &world.filesystem().unwrap()));
            assert_eq!(
                text::read(root.as_ref(), "tmp/retained").unwrap(),
                "old temporary bytes"
            );
            let after = candidate.symlink_metadata(name).unwrap();
            assert_eq!(
                (
                    after.inode,
                    after.kind,
                    after.size,
                    after.mode,
                    after.links,
                    after.modified
                ),
                (
                    before.inode,
                    before.kind,
                    before.size,
                    before.mode,
                    before.links,
                    before.modified
                ),
            );
            match kind {
                "file" => assert_eq!(
                    text::read(candidate.as_ref(), name).unwrap(),
                    "workspace bytes"
                ),
                "directory" => assert_eq!(
                    text::read(candidate.as_ref(), &format!("{name}/child")).unwrap(),
                    "child bytes"
                ),
                "dangling-link" => assert_eq!(candidate.read_link(name).unwrap(), "absent"),
                _ => (),
            }
        }
    }
}

#[test]
fn bin_merges_workspace_files_and_rejects_conflicts_before_replacing_a_root() {
    let module =
        wat::parse_str("(module (memory (export \"memory\") 1) (func (export \"_start\")))")
            .unwrap();
    let mut world = World::new();
    world.register("custom", &module).unwrap();
    for kind in ["file", "link", "command"] {
        let candidate = workspace();
        let expected = match kind {
            "file" => {
                text::write(candidate.as_ref(), "bin", "keep").unwrap();
                "/bin"
            }
            "link" => {
                candidate.create_directory("real-bin").unwrap();
                candidate.symlink("real-bin", "bin").unwrap();
                "/bin"
            }
            "command" => {
                candidate.create_directory("bin").unwrap();
                text::write(candidate.as_ref(), "bin/custom", "keep").unwrap();
                "/bin/custom"
            }
            _ => unreachable!(),
        };
        let error = world.set_filesystem(candidate).unwrap_err();
        assert_eq!(error.path, expected);
        assert_eq!(error.error, FileError::Exists);
        assert_eq!(error.to_string(), format!("Mount '{expected}': Exists"));
        assert!(world.filesystem().is_none());
        assert!(world.has_program("custom"));
    }
    let candidate = workspace();
    candidate.create_directory("bin").unwrap();
    text::write(candidate.as_ref(), "bin/local", "local script").unwrap();
    for name in ["TMP", "Dev", "tmp-data", "process"] {
        text::write(candidate.as_ref(), name, "ordinary workspace entry").unwrap();
    }
    world.set_filesystem(candidate.clone()).unwrap();
    let root = world.filesystem().unwrap();
    assert_eq!(
        text::read(root.as_ref(), "bin/local").unwrap(),
        "local script"
    );
    let listing = root.read_directory("bin").unwrap();
    assert!(listing.iter().any(|(name, _)| name == "local"));
    assert!(listing.iter().any(|(name, _)| name == "custom"));
    assert_eq!(root.metadata("bin/custom").unwrap().mode, 0o555);
    assert_eq!(
        candidate.metadata("bin/custom").err(),
        Some(FileError::NotFound)
    );
    for name in ["TMP", "Dev", "tmp-data", "process"] {
        assert_eq!(
            text::read(root.as_ref(), name).unwrap(),
            "ordinary workspace entry"
        );
    }
}

struct FailingLayout {
    path: &'static str,
    error: FileError,
    queried: RefCell<Vec<String>>,
}

impl FileSystem for FailingLayout {
    fn open(&self, _: &str, _: OpenOptions) -> Result<Box<dyn File>, FileError> {
        panic!("Mount validation must not open provider files")
    }
    fn metadata(&self, _: &str) -> Result<Metadata, FileError> {
        panic!("Mount validation must inspect entries without following links")
    }
    fn symlink_metadata(&self, path: &str) -> Result<Metadata, FileError> {
        self.queried.borrow_mut().push(path.into());
        Err(if path == self.path {
            self.error
        } else {
            FileError::NotFound
        })
    }
    fn read_directory(&self, _: &str) -> Result<DirectoryEntries, FileError> {
        panic!("Mount validation must not enumerate provider directories")
    }
}

#[test]
fn validation_preserves_provider_failures_and_stops_before_publication() {
    for path in ["dev", "tmp", "proc", "bin"] {
        for error in [FileError::Access, FileError::Io, FileError::Corrupt] {
            let provider = Rc::new(FailingLayout {
                path,
                error,
                queried: RefCell::new(Vec::new()),
            });
            let mut world = World::new();
            let result = world.set_filesystem(provider.clone()).unwrap_err();
            assert_eq!(
                (result.path.as_str(), result.error),
                (format!("/{path}").as_str(), error)
            );
            assert!(world.filesystem().is_none());
            assert_eq!(provider.queried.borrow().last().unwrap(), path);
        }
    }
}

#[test]
fn shared_file_tools_and_guest_execution_use_one_temporary_mount() {
    let workspace = workspace();
    let mut world = World::new();
    assert!(world.filesystem().is_none());
    world.set_filesystem(workspace.clone()).unwrap();
    let root = world.filesystem().unwrap();
    text::write(root.as_ref(), "/tmp/tools/nested/input", "shared bytes").unwrap();
    assert_eq!(
        text::read(root.as_ref(), "/tmp/tools/nested/input").unwrap(),
        "shared bytes"
    );
    assert_eq!(workspace.metadata("tmp").err(), Some(FileError::NotFound));
    assert!(
        text::read_or_list(root.as_ref(), "/")
            .unwrap()
            .contains("tmp\n")
    );
    let module = wat::parse_str(r#"(module
        (import "wasi_snapshot_preview1" "path_open" (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "fd_read" (func $read (param i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 1)
        (data (i32.const 100) "tmp/tools/nested/input")
        (func (export "_start")
            (if (call $open (i32.const 3) (i32.const 1) (i32.const 100) (i32.const 22) (i32.const 0) (i64.const 2) (i64.const 0) (i32.const 0) (i32.const 0)) (then unreachable))
            (i32.store (i32.const 16) (i32.const 256))
            (i32.store (i32.const 20) (i32.const 32))
            (if (call $read (i32.load (i32.const 0)) (i32.const 16) (i32.const 1) (i32.const 24)) (then unreachable))
            (if (i32.ne (i32.load (i32.const 24)) (i32.const 12)) (then unreachable))
            (if (i32.ne (i32.load (i32.const 256)) (i32.const 1918986355)) (then unreachable))))"#).unwrap();
    let mut binary = root
        .open(
            "tmp/probe.wasm",
            OpenOptions {
                read: false,
                write: true,
                create: true,
                exclusive: true,
                truncate: false,
                append: false,
            },
        )
        .unwrap();
    assert_eq!(binary.write(&module).unwrap(), module.len());
    binary.flush().unwrap();
    drop(binary);
    let loaded = text::read_bytes(root.as_ref(), "/tmp/probe.wasm", 48 * 1024 * 1024).unwrap();
    assert_eq!(loaded, module);
    world.register("probe", &loaded).unwrap();
    let pid = world.spawn(vec!["probe".into()], vec![]).unwrap();
    for _ in 0..100 {
        if !world.status(pid).unwrap().is_live() {
            break;
        }
        world.step();
    }
    assert_eq!(world.status(pid).unwrap(), &Status::Exited(0));
    assert_eq!(
        text::read(root.as_ref(), "tmp/tools/nested/input").unwrap(),
        "shared bytes"
    );
}

#[test]
fn readonly_mount_operations_never_fall_through_to_backing_storage() {
    let workspace = workspace();
    let root = VirtualFileSystem::new(workspace.clone()).unwrap();
    text::write(root.as_ref(), "source", "keep").unwrap();
    for mount in ["dev", "proc"] {
        let path = format!("{mount}/new");
        assert_eq!(root.symlink("/source", &path), Err(FileError::ReadOnly));
        assert_eq!(root.create_directory(&path), Err(FileError::ReadOnly));
        assert_eq!(root.rename("source", &path), Err(FileError::CrossDevice));
        assert_eq!(
            root.hard_link("source", &path, false),
            Err(FileError::CrossDevice)
        );
        assert!(
            text::write(root.as_ref(), &path, "hidden")
                .unwrap_err()
                .contains("ReadOnly")
        );
        assert_eq!(
            root.symlink_metadata(&path).err(),
            Some(FileError::NotFound)
        );
        assert_eq!(workspace.metadata(mount).err(), Some(FileError::NotFound));
        assert_eq!(text::read(root.as_ref(), "source").unwrap(), "keep");
    }
    let local = root.directory_view("tmp").unwrap().unwrap();
    local.create_directory("dev").unwrap();
    assert_eq!(local.metadata("dev").unwrap().kind, EntryType::Directory);
}
