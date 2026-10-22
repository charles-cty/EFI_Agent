use efi_agent_filesystem::{
    EntryType, File, FileError, FileSystem, OpenOptions, TimeUpdate, namespace::Namespace,
    native::NativeFileSystem,
};
use std::{cell::Cell, collections::BTreeMap, fs, path::PathBuf, rc::Rc, time::SystemTime};

struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("efi-namespace-{}-{nonce}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn raw(&self) -> Rc<dyn FileSystem> {
        Rc::new(NativeFileSystem(self.0.clone()))
    }
    fn open(&self) -> Namespace {
        Namespace::open(self.raw(), Some(Rc::new(|| Ok(731)))).unwrap()
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn options() -> OpenOptions {
    OpenOptions {
        read: true,
        write: true,
        create: true,
        exclusive: false,
        truncate: false,
        append: false,
    }
}
fn write(fs: &dyn FileSystem, path: &str, bytes: &[u8]) {
    let mut file = fs
        .open(
            path,
            OpenOptions {
                truncate: true,
                ..options()
            },
        )
        .unwrap();
    assert_eq!(file.write(bytes).unwrap(), bytes.len());
    file.flush().unwrap();
}
fn read(fs: &dyn FileSystem, path: &str) -> Vec<u8> {
    let mut file = fs
        .open(
            path,
            OpenOptions {
                write: false,
                create: false,
                ..options()
            },
        )
        .unwrap();
    let mut result = Vec::new();
    loop {
        let mut bytes = [0; 193];
        let count = file.read(&mut bytes).unwrap();
        if count == 0 {
            break;
        }
        result.extend_from_slice(&bytes[..count]);
    }
    result
}

#[test]
fn model_files_share_guest_namespace_and_validate_before_creating_parents() {
    use efi_agent_filesystem::text;
    let work = Workspace::new();
    let namespace = work.open();
    text::write(&namespace, "scripts/deep/input.py", "print(731)\n").unwrap();
    assert_eq!(read(&namespace, "scripts/deep/input.py"), b"print(731)\n");
    namespace.rename("scripts", "renamed").unwrap();
    assert_eq!(
        text::read(&namespace, "/renamed/deep/input.py").unwrap(),
        "print(731)\n"
    );
    namespace.symlink("renamed/deep/input.py", "link").unwrap();
    text::write(&namespace, "link", "replacement").unwrap();
    assert_eq!(read(&namespace, "renamed/deep/input.py"), b"replacement");
    namespace.unlink("renamed/deep/input.py").unwrap();
    assert!(
        text::read(&namespace, "link")
            .unwrap_err()
            .contains("NotFound")
    );
    assert!(text::write(&namespace, "new/../escape", "bad").is_err());
    assert_eq!(namespace.metadata("new").unwrap_err(), FileError::NotFound);
    assert!(text::write(&namespace, "new/deep/input", &"x".repeat(524289)).is_err());
    assert_eq!(namespace.metadata("new").unwrap_err(), FileError::NotFound);
    let limit = "x".repeat(524288);
    text::write(&namespace, "limit", &limit).unwrap();
    assert_eq!(text::read(&namespace, "limit").unwrap(), limit);
    assert!(text::read_bytes(&namespace, "limit", 524287).is_err());
    assert_eq!(text::read_or_list(&namespace, "renamed/deep").unwrap(), "");
    assert!(
        text::read_or_list(&namespace, ".")
            .unwrap()
            .contains("renamed\n")
    );
}

#[test]
fn packaging_metadata_restores_links_without_following_and_rejects_partial_batches() {
    use efi_agent_filesystem::Metadata;
    let work = Workspace::new();
    let namespace = work.open();
    write(&namespace, "target", b"bytes");
    namespace.symlink("target", "link").unwrap();
    let mut link = Metadata::new(EntryType::Symlink, 0);
    link.mode = 0o123;
    link.accessed = 135;
    link.modified = 246;
    link.created = 357;
    let batch = [("link".into(), link), ("absent".into(), link)];
    assert_eq!(namespace.restore_metadata(&batch), Err(FileError::NotFound));
    assert_ne!(namespace.symlink_metadata("link").unwrap().mode, link.mode);
    namespace.restore_metadata(&batch[..1]).unwrap();
    assert_eq!(namespace.metadata("target").unwrap().mode, 0o644);
    drop(namespace);
    let namespace = work.open();
    let restored = namespace.symlink_metadata("link").unwrap();
    assert_eq!(
        (
            restored.mode,
            restored.accessed,
            restored.modified,
            restored.created
        ),
        (0o123, 135, 246, 357)
    );
    assert_eq!(read(&namespace, "target"), b"bytes");
    let mut invalid = link;
    invalid.kind = EntryType::File;
    assert_eq!(
        namespace.restore_metadata(&[("link".into(), invalid)]),
        Err(FileError::Invalid)
    );
}

#[test]
fn unlink_and_replacement_retain_open_file_identity_and_collect_after_last_close() {
    let work = Workspace::new();
    fs::write(work.0.join("source"), b"SOURCE").unwrap();
    fs::write(work.0.join("destination"), b"TARGET").unwrap();
    let namespace = work.open();
    let mut source = namespace.open("source", options()).unwrap();
    let mut target = namespace.open("destination", options()).unwrap();
    let source_id = source.metadata().unwrap().inode;
    let target_id = target.metadata().unwrap().inode;
    assert_ne!(source_id, target_id);
    namespace.rename("source", "destination").unwrap();
    assert_eq!(
        namespace.metadata("source").unwrap_err(),
        FileError::NotFound
    );
    assert_eq!(namespace.metadata("destination").unwrap().inode, source_id);
    assert_eq!(target.metadata().unwrap().links, 0);
    target.write(b"OLD!!!").unwrap();
    source.write(b"NEW!!!").unwrap();
    assert_eq!(read(&namespace, "destination"), b"NEW!!!");
    assert!(work.0.join("destination").exists());
    drop(target);
    assert!(!work.0.join("destination").exists());
    namespace.unlink("destination").unwrap();
    assert_eq!(source.metadata().unwrap().links, 0);
    source.seek(0, 0).unwrap();
    let mut bytes = [0; 6];
    assert_eq!(source.read(&mut bytes).unwrap(), 6);
    assert_eq!(&bytes, b"NEW!!!");
    assert!(work.0.join("source").exists());
    drop(source);
    assert!(!work.0.join("source").exists());
    assert!(namespace.take_errors().is_empty());
    drop(namespace);
    let namespace = work.open();
    assert_eq!(
        namespace.metadata("destination").unwrap_err(),
        FileError::NotFound
    );
}

#[test]
fn directory_views_survive_rename_and_removed_views_cannot_escape_their_anchor() {
    let work = Workspace::new();
    let namespace = work.open();
    namespace.create_directory("a").unwrap();
    namespace.create_directory("b").unwrap();
    namespace.create_directory("a/deep").unwrap();
    write(&namespace, "a/deep/file", b"kept");
    let directory = namespace.directory_view("a").unwrap().unwrap();
    let target = namespace.directory_view("b").unwrap().unwrap();
    namespace.rename("a", "moved").unwrap();
    assert_eq!(read(directory.as_ref(), "deep/file"), b"kept");
    directory
        .rename_to("deep/file", target.as_ref(), "file")
        .unwrap();
    assert_eq!(read(&namespace, "b/file"), b"kept");
    assert_eq!(
        directory.metadata("../b").unwrap_err(),
        FileError::NotCapable
    );
    directory.remove_directory("deep").unwrap();
    namespace.remove_directory("moved").unwrap();
    assert_eq!(directory.metadata("").unwrap().links, 0);
    assert_eq!(
        directory.create_directory("child"),
        Err(FileError::NotFound)
    );
    assert_eq!(
        namespace.metadata("moved").unwrap_err(),
        FileError::NotFound
    );
}

#[test]
fn rename_type_rules_nonempty_directories_and_cycles_preserve_the_tree() {
    let work = Workspace::new();
    let namespace = work.open();
    namespace.create_directory("a").unwrap();
    namespace.create_directory("a/child").unwrap();
    namespace.create_directory("b").unwrap();
    write(&namespace, "file", b"original");
    assert_eq!(
        namespace.rename("a", "a/child/new"),
        Err(FileError::Invalid)
    );
    assert_eq!(namespace.rename("file", "b"), Err(FileError::IsDirectory));
    assert_eq!(namespace.rename("b", "file"), Err(FileError::NotDirectory));
    assert_eq!(namespace.rename("b", "a"), Err(FileError::NotEmpty));
    assert_eq!(namespace.remove_directory("a"), Err(FileError::NotEmpty));
    assert_eq!(namespace.unlink("a"), Err(FileError::IsDirectory));
    assert_eq!(
        namespace.remove_directory("file"),
        Err(FileError::NotDirectory)
    );
    namespace.rename("a", "a").unwrap();
    namespace.rename("a", "b").unwrap();
    assert!(namespace.metadata("b/child").unwrap().is_directory());
    assert_eq!(read(&namespace, "file"), b"original");
}

#[test]
fn symlinks_follow_components_before_parent_steps_and_preserve_lstat() {
    let work = Workspace::new();
    let namespace = work.open();
    namespace.create_directory("a").unwrap();
    namespace.create_directory("b").unwrap();
    namespace.create_directory("b/deep").unwrap();
    write(&namespace, "a/file", b"wrong");
    write(&namespace, "b/file", b"correct");
    namespace.symlink("../b/deep", "a/link").unwrap();
    assert_eq!(read(&namespace, "a/link/../file"), b"correct");
    namespace.symlink("/b/file", "absolute").unwrap();
    assert_eq!(read(&namespace, "absolute"), b"correct");
    assert_eq!(
        namespace.symlink_metadata("absolute").unwrap().kind,
        EntryType::Symlink
    );
    assert_eq!(
        namespace.metadata("absolute").unwrap().kind,
        EntryType::File
    );
    assert_eq!(namespace.read_link("absolute").unwrap(), "/b/file");
    namespace.symlink("missing", "dangling").unwrap();
    assert_eq!(
        namespace.metadata("dangling").unwrap_err(),
        FileError::NotFound
    );
    assert_eq!(
        namespace.symlink_metadata("dangling").unwrap().kind,
        EntryType::Symlink
    );
    namespace.symlink("cycle2", "cycle1").unwrap();
    namespace.symlink("cycle1", "cycle2").unwrap();
    assert_eq!(namespace.metadata("cycle1").unwrap_err(), FileError::Loop);
    namespace.unlink("absolute").unwrap();
    assert_eq!(read(&namespace, "b/file"), b"correct");
    drop(namespace);
    let namespace = work.open();
    assert_eq!(namespace.read_link("a/link").unwrap(), "../b/deep");
}

#[test]
fn modes_times_and_empty_directories_survive_reload_and_enforce_permissions() {
    let work = Workspace::new();
    let namespace = work.open();
    namespace.create_directory("empty").unwrap();
    write(&namespace, "file", b"data");
    let mut file = namespace.open("file", options()).unwrap();
    namespace.set_mode("file", 0o400).unwrap();
    assert_eq!(
        namespace.open("file", options()).err(),
        Some(FileError::Access)
    );
    // An existing writable descriptor keeps its access after chmod.
    assert_eq!(file.write(b"OPEN").unwrap(), 4);
    file.set_times(TimeUpdate::Set(193), TimeUpdate::Set(853))
        .unwrap();
    namespace
        .set_times("empty", true, TimeUpdate::Now, TimeUpdate::Keep)
        .unwrap();
    assert_eq!(namespace.metadata("empty").unwrap().accessed, 731);
    namespace.set_mode("empty", 0o500).unwrap();
    assert_eq!(
        namespace.create_directory("empty/child"),
        Err(FileError::Access)
    );
    drop(file);
    drop(namespace);
    let namespace = work.open();
    let info = namespace.metadata("file").unwrap();
    assert_eq!((info.mode, info.accessed, info.modified), (0o400, 193, 853));
    assert_eq!(namespace.metadata("empty").unwrap().mode, 0o500);
    assert_eq!(read(&namespace, "file"), b"OPEN");
    let stat = info.wasi();
    assert_eq!(u64::from_le_bytes(stat[40..48].try_into().unwrap()), 193);
    assert_eq!(u64::from_le_bytes(stat[48..56].try_into().unwrap()), 853);
}

#[test]
fn private_files_cannot_be_opened_listed_or_replaced_by_guests() {
    let work = Workspace::new();
    let namespace = work.open();
    let names: Vec<_> = namespace
        .read_directory("")
        .unwrap()
        .iter()
        .map(|(name, _)| name.to_owned())
        .collect();
    assert_eq!(names.len(), 2);
    assert_eq!(
        namespace.metadata(".efi-agent-fs").unwrap_err(),
        FileError::NotFound
    );
    assert_eq!(
        namespace.open(".efi-agent-fs/state1", options()).err(),
        Some(FileError::NotFound)
    );
    assert_eq!(
        namespace.create_directory(".efi-agent-fs"),
        Err(FileError::Access)
    );
    assert_eq!(
        namespace.create_directory(".EFI-AGENT-FS"),
        Err(FileError::Access)
    );
    assert_eq!(
        namespace.create_directory(".efi-process"),
        Err(FileError::Access)
    );
    assert!(namespace.capacity().unwrap().total >= namespace.capacity().unwrap().available);
}

struct Faults {
    remaining: Cell<Option<usize>>,
    flush: Cell<bool>,
}
struct FaultFile {
    file: Box<dyn File>,
    faults: Rc<Faults>,
}
impl File for FaultFile {
    fn read(&mut self, bytes: &mut [u8]) -> Result<usize, FileError> {
        self.file.read(bytes)
    }
    fn write(&mut self, bytes: &[u8]) -> Result<usize, FileError> {
        if let Some(left) = self.faults.remaining.get() {
            if left == 0 {
                return Err(FileError::Io);
            }
            let count = self.file.write(&bytes[..bytes.len().min(left)])?;
            self.faults.remaining.set(Some(left - count));
            Ok(count)
        } else {
            self.file.write(bytes)
        }
    }
    fn seek(&mut self, offset: i64, whence: u32) -> Result<u64, FileError> {
        self.file.seek(offset, whence)
    }
    fn metadata(&mut self) -> Result<efi_agent_filesystem::Metadata, FileError> {
        self.file.metadata()
    }
    fn flush(&mut self) -> Result<(), FileError> {
        self.file.flush()?;
        if self.faults.flush.replace(false) {
            Err(FileError::Io)
        } else {
            Ok(())
        }
    }
    fn resize(&mut self, size: u64) -> Result<(), FileError> {
        self.file.resize(size)
    }
}
struct FaultFs {
    raw: Rc<dyn FileSystem>,
    faults: Rc<Faults>,
}
impl FileSystem for FaultFs {
    fn open(&self, path: &str, options: OpenOptions) -> Result<Box<dyn File>, FileError> {
        let file = self.raw.open(path, options)?;
        if path.starts_with(".efi-agent-fs/state") {
            Ok(Box::new(FaultFile {
                file,
                faults: self.faults.clone(),
            }))
        } else {
            Ok(file)
        }
    }
    fn metadata(&self, path: &str) -> Result<efi_agent_filesystem::Metadata, FileError> {
        self.raw.metadata(path)
    }
    fn read_directory(
        &self,
        path: &str,
    ) -> Result<efi_agent_filesystem::DirectoryEntries, FileError> {
        self.raw.read_directory(path)
    }
    fn create_directory(&self, path: &str) -> Result<(), FileError> {
        self.raw.create_directory(path)
    }
    fn unlink(&self, path: &str) -> Result<(), FileError> {
        self.raw.unlink(path)
    }
}

#[test]
fn interrupted_journal_write_recovers_the_previous_complete_generation() {
    let work = Workspace::new();
    fs::write(work.0.join("file"), b"preserved").unwrap();
    let faults = Rc::new(Faults {
        remaining: Cell::new(None),
        flush: Cell::new(false),
    });
    let raw: Rc<dyn FileSystem> = Rc::new(FaultFs {
        raw: work.raw(),
        faults: faults.clone(),
    });
    let namespace = Namespace::open(raw.clone(), None).unwrap();
    namespace.set_mode("file", 0o600).unwrap();
    faults.remaining.set(Some(32));
    assert_eq!(namespace.rename("file", "renamed"), Err(FileError::Io));
    assert_eq!(namespace.metadata("file").unwrap_err(), FileError::Io);
    assert_eq!(namespace.create_directory("later"), Err(FileError::Io));
    faults.remaining.set(None);
    drop(namespace);
    let namespace = Namespace::open(raw, None).unwrap();
    assert_eq!(namespace.metadata("file").unwrap().mode, 0o600);
    assert_eq!(read(&namespace, "file"), b"preserved");
    assert_eq!(
        namespace.metadata("renamed").unwrap_err(),
        FileError::NotFound
    );
}

#[test]
fn failed_flush_can_recover_a_complete_newer_generation() {
    let work = Workspace::new();
    let faults = Rc::new(Faults {
        remaining: Cell::new(None),
        flush: Cell::new(false),
    });
    let raw: Rc<dyn FileSystem> = Rc::new(FaultFs {
        raw: work.raw(),
        faults: faults.clone(),
    });
    let namespace = Namespace::open(raw.clone(), None).unwrap();
    faults.flush.set(true);
    assert_eq!(namespace.create_directory("committed"), Err(FileError::Io));
    assert_eq!(namespace.metadata("committed").unwrap_err(), FileError::Io);
    drop(namespace);
    let namespace = Namespace::open(raw, None).unwrap();
    assert!(namespace.metadata("committed").unwrap().is_directory());
}

#[test]
fn corrupt_metadata_and_reserved_collisions_preserve_user_bytes() {
    let work = Workspace::new();
    fs::write(work.0.join("file"), b"untouched").unwrap();
    let namespace = work.open();
    namespace.set_mode("file", 0o600).unwrap();
    drop(namespace);
    fs::write(work.0.join(".efi-agent-fs/state0"), b"bad").unwrap();
    fs::write(work.0.join(".efi-agent-fs/state1"), b"bad").unwrap();
    assert_eq!(
        Namespace::open(work.raw(), None).err(),
        Some(FileError::Corrupt)
    );
    assert_eq!(fs::read(work.0.join("file")).unwrap(), b"untouched");
    let other = Workspace::new();
    fs::write(other.0.join(".efi-agent-fs"), b"user content").unwrap();
    assert_eq!(
        Namespace::open(other.raw(), None).err(),
        Some(FileError::Exists)
    );
    assert_eq!(
        fs::read(other.0.join(".efi-agent-fs")).unwrap(),
        b"user content"
    );
}

#[test]
fn structured_file_mutations_match_an_independent_map_across_restarts() {
    let work = Workspace::new();
    let mut namespace = work.open();
    let mut expected = BTreeMap::<String, Vec<u8>>::new();
    let mut random = 0x731853193u64;
    for step in 0..160 {
        random = random
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let path = format!("file-{}", (random >> 8) % 12);
        match random % 3 {
            0 => {
                let bytes: Vec<_> = (0..(random >> 16) as usize % 853)
                    .map(|i| (i + step) as u8)
                    .collect();
                write(&namespace, &path, &bytes);
                expected.insert(path, bytes);
            }
            1 if !expected.is_empty() => {
                let source = expected
                    .keys()
                    .nth((random >> 24) as usize % expected.len())
                    .unwrap()
                    .clone();
                namespace.rename(&source, &path).unwrap();
                let bytes = expected.remove(&source).unwrap();
                expected.insert(path, bytes);
            }
            _ if expected.contains_key(&path) => {
                namespace.unlink(&path).unwrap();
                expected.remove(&path);
            }
            _ => (),
        }
        if step % 11 == 0 {
            drop(namespace);
            namespace = work.open();
        }
        let actual: BTreeMap<_, _> = namespace
            .snapshot()
            .unwrap()
            .into_iter()
            .filter(|entry| entry.metadata.kind == EntryType::File)
            .map(|entry| {
                let bytes = read(&namespace, &entry.path);
                (entry.path, bytes)
            })
            .collect();
        assert_eq!(actual, expected, "step {step}");
        assert!(namespace.take_errors().is_empty());
    }
}

#[test]
fn recognized_initialization_without_a_generation_resumes_without_importing_private_files() {
    for objects_exist in [false, true] {
        let work = Workspace::new();
        fs::write(work.0.join("original"), b"preserved").unwrap();
        fs::create_dir(work.0.join(".efi-agent-fs")).unwrap();
        fs::write(
            work.0.join(".efi-agent-fs/format"),
            b"EFI Agent filesystem 1\n",
        )
        .unwrap();
        if objects_exist {
            fs::create_dir(work.0.join(".efi-agent-fs/objects")).unwrap();
        }
        let namespace = work.open();
        assert_eq!(read(&namespace, "original"), b"preserved");
        assert_eq!(
            namespace.metadata(".efi-agent-fs").unwrap_err(),
            FileError::NotFound
        );
        write(&namespace, "new", b"created");
        drop(namespace);
        let namespace = work.open();
        assert_eq!(read(&namespace, "new"), b"created");
    }
}

#[test]
fn creation_through_dangling_links_preserves_links_and_checks_directory_suffixes() {
    let work = Workspace::new();
    let namespace = work.open();
    namespace.create_directory("d").unwrap();
    namespace.symlink("d/target", "link").unwrap();
    assert_eq!(
        namespace
            .open(
                "link",
                OpenOptions {
                    exclusive: true,
                    ..options()
                }
            )
            .err(),
        Some(FileError::Exists)
    );
    write(&namespace, "link", b"target bytes");
    assert_eq!(namespace.read_link("link").unwrap(), "d/target");
    assert_eq!(read(&namespace, "d/target"), b"target bytes");
    namespace.symlink("d/missing/", "directory-link").unwrap();
    assert_eq!(
        namespace.open("directory-link", options()).err(),
        Some(FileError::IsDirectory)
    );
    for path in ["missing/", "link/", "d/target/", "d/"] {
        assert_eq!(
            namespace.open(path, options()).err(),
            Some(FileError::IsDirectory),
            "{path}"
        );
    }
    assert_eq!(
        namespace.metadata("d/target/.").unwrap_err(),
        FileError::NotDirectory
    );
    assert_eq!(namespace.unlink("link/"), Err(FileError::NotDirectory));
    assert_eq!(
        namespace.rename("link/", "new"),
        Err(FileError::NotDirectory)
    );
    assert_eq!(
        namespace.rename("d/target", "new/"),
        Err(FileError::NotDirectory)
    );
    namespace.rename("d", "moved/").unwrap();
    assert_eq!(read(&namespace, "moved/target"), b"target bytes");
}

#[test]
fn failed_clock_prevents_resize_and_reads_before_effects() {
    let work = Workspace::new();
    let failed = Rc::new(Cell::new(false));
    let clock_flag = failed.clone();
    let namespace = Namespace::open(
        work.raw(),
        Some(Rc::new(move || {
            if clock_flag.get() {
                Err(FileError::Io)
            } else {
                Ok(193)
            }
        })),
    )
    .unwrap();
    write(&namespace, "file", b"unchanged");
    let mut file = namespace.open("file", options()).unwrap();
    failed.set(true);
    assert_eq!(file.resize(0), Err(FileError::Io));
    assert_eq!(file.metadata().unwrap().size, 9);
    let mut bytes = [0xA5; 9];
    assert_eq!(file.read(&mut bytes), Err(FileError::Io));
    assert_eq!(bytes, [0xA5; 9]);
    assert_eq!(file.seek(0, 1).unwrap(), 0);
    failed.set(false);
    assert_eq!(file.read(&mut bytes).unwrap(), 9);
    assert_eq!(&bytes, b"unchanged");
    assert_eq!(file.metadata().unwrap().accessed, 193);
}

#[test]
fn read_close_defers_access_time_but_explicit_flush_persists_it() {
    use efi_agent_filesystem::namespace::read_snapshot;
    let work = Workspace::new();
    fs::write(work.0.join("file"), b"bytes").unwrap();
    let time = Rc::new(Cell::new(193));
    let clock = time.clone();
    let namespace = Namespace::open(work.raw(), Some(Rc::new(move || Ok(clock.get())))).unwrap();
    let saved_access = || {
        read_snapshot(work.raw().as_ref())
            .unwrap()
            .unwrap()
            .into_iter()
            .find(|entry| entry.path == "file")
            .unwrap()
            .metadata
            .accessed
    };
    let original = saved_access();
    time.set(original + 731);
    assert_eq!(read(&namespace, "file"), b"bytes");
    assert_eq!(namespace.metadata("file").unwrap().accessed, original + 731);
    assert_eq!(saved_access(), original);
    let mut file = namespace
        .open(
            "file",
            OpenOptions {
                write: false,
                create: false,
                ..options()
            },
        )
        .unwrap();
    file.flush().unwrap();
    assert_eq!(saved_access(), original + 731);
    time.set(original + 937);
    assert_eq!(read(&namespace, "file"), b"bytes");
    namespace.flush().unwrap();
    assert_eq!(saved_access(), original + 937);
}

#[test]
fn failed_writable_close_is_reported_and_requires_recovery() {
    let work = Workspace::new();
    let faults = Rc::new(Faults {
        remaining: Cell::new(None),
        flush: Cell::new(false),
    });
    let namespace = Namespace::open(
        Rc::new(FaultFs {
            raw: work.raw(),
            faults: faults.clone(),
        }),
        Some(Rc::new(|| Ok(731))),
    )
    .unwrap();
    let mut file = namespace.open("file", options()).unwrap();
    file.write(b"saved bytes").unwrap();
    faults.flush.set(true);
    drop(file);
    assert!(namespace.take_errors().contains(&FileError::Io));
    assert_eq!(namespace.flush(), Err(FileError::Io));
    let recovered = work.open();
    assert_eq!(read(&recovered, "file"), b"saved bytes");
}

#[test]
fn committed_export_uses_logical_paths_without_collecting_or_changing_raw_storage() {
    use efi_agent_filesystem::namespace::read_snapshot;
    let work = Workspace::new();
    fs::write(work.0.join("original"), b"original bytes").unwrap();
    let namespace = work.open();
    let held = namespace.open("original", options()).unwrap();
    namespace.rename("original", "renamed").unwrap();
    namespace.create_directory("empty").unwrap();
    namespace.symlink("renamed", "link").unwrap();
    write(&namespace, "deleted", b"detached bytes");
    let detached = namespace.open("deleted", options()).unwrap();
    namespace.unlink("deleted").unwrap();
    let slots =
        ["state0", "state1"].map(|name| fs::read(work.0.join(".efi-agent-fs").join(name)).unwrap());
    let entries = read_snapshot(work.raw().as_ref()).unwrap().unwrap();
    let names: Vec<_> = entries.iter().map(|entry| entry.path.as_str()).collect();
    assert_eq!(names, ["", "renamed", "empty", "link"]);
    let file = entries
        .iter()
        .find(|entry| entry.path == "renamed")
        .unwrap();
    assert_eq!(file.backing.as_deref(), Some("original"));
    assert_eq!(
        fs::read(work.0.join(file.backing.as_ref().unwrap())).unwrap(),
        b"original bytes"
    );
    let link = entries.iter().find(|entry| entry.path == "link").unwrap();
    assert_eq!(link.target.as_deref(), Some("renamed"));
    assert_eq!(
        slots,
        ["state0", "state1"].map(|name| fs::read(work.0.join(".efi-agent-fs").join(name)).unwrap())
    );
    assert_eq!(
        fs::read_dir(work.0.join(".efi-agent-fs/objects"))
            .unwrap()
            .count(),
        1
    );
    drop(detached);
    drop(held);
    assert!(namespace.take_errors().is_empty());
}
