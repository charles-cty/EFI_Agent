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

#[test]
fn namespace_statistics_report_capacity_and_retained_entry_quota() {
    let work = Workspace::new();
    let namespace = work.open();
    let initial = namespace.filesystem_statistics("").unwrap();
    assert_eq!(initial.files_free, initial.files - 1);
    let raw = work.raw().capacity().unwrap();
    assert_eq!(initial.capacity.total, raw.total);
    assert_eq!(initial.capacity.block_size, raw.block_size);
    assert!(initial.capacity.available <= initial.capacity.free);
    let handle = namespace.open("file", options()).unwrap();
    namespace.hard_link("file", "alias", false).unwrap();
    assert_eq!(
        namespace.filesystem_statistics("alias").unwrap().files_free,
        initial.files_free - 2
    );
    namespace.unlink("file").unwrap();
    assert_eq!(
        handle.filesystem_statistics().unwrap().files_free,
        initial.files_free - 1
    );
    namespace.unlink("alias").unwrap();
    assert_eq!(
        handle.filesystem_statistics().unwrap().files_free,
        initial.files_free - 1
    );
    assert_eq!(
        namespace.filesystem_statistics("alias").unwrap_err(),
        FileError::NotFound
    );
    drop(handle);
    assert_eq!(
        namespace.filesystem_statistics("").unwrap().files_free,
        initial.files_free
    );
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

#[cfg(unix)]
fn host_allocated(path: &std::path::Path) -> u64 {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(path).unwrap().blocks() * 512
}

#[test]
fn hard_links_share_identity_and_survive_name_mutation_reload_and_last_close() {
    let work = Workspace::new();
    let namespace = work.open();
    namespace.create_directory("a").unwrap();
    namespace.create_directory("b").unwrap();
    write(&namespace, "a/file", b"original");
    let a = namespace.directory_view("a").unwrap().unwrap();
    let b = namespace.directory_view("b").unwrap().unwrap();
    a.hard_link_to("file", b.as_ref(), "alias", false).unwrap();
    namespace.hard_link("a/file", "third", false).unwrap();
    let mut handle = namespace.open("third", options()).unwrap();
    let inode = handle.metadata().unwrap().inode;
    namespace.set_mode("b/alias", 0o640).unwrap();
    namespace
        .set_times("third", true, TimeUpdate::Set(193), TimeUpdate::Set(853))
        .unwrap();
    write(&namespace, "b/alias", b"shared bytes");
    for path in ["a/file", "b/alias", "third"] {
        let info = namespace.metadata(path).unwrap();
        assert_eq!((info.inode, info.links, info.mode), (inode, 3, 0o640));
        assert_eq!(info.allocated, handle.metadata().unwrap().allocated);
        assert_eq!(read(&namespace, path), b"shared bytes");
    }
    // Renaming onto a second name of the same inode must preserve both entries.
    namespace.rename("a/file", "b/alias").unwrap();
    assert_eq!(namespace.metadata("a/file").unwrap().links, 3);
    namespace.rename("b", "moved").unwrap();
    b.rename("alias", "renamed").unwrap();
    namespace.unlink("third").unwrap(); // Remove an additional entry.
    namespace.unlink("a/file").unwrap(); // Remove the primary, promote the alias.
    assert_eq!(read(b.as_ref(), "renamed"), b"shared bytes");
    assert_eq!(handle.metadata().unwrap().links, 1);
    namespace
        .hard_link("moved/renamed", "persisted", false)
        .unwrap();
    namespace.flush().unwrap();
    drop(handle);
    drop(a);
    drop(b);
    drop(namespace);
    let namespace = work.open();
    assert_eq!(namespace.metadata("persisted").unwrap().inode, inode);
    assert_eq!(namespace.metadata("moved/renamed").unwrap().links, 2);
    let snapshot = namespace.snapshot().unwrap();
    let backing = snapshot
        .iter()
        .find(|e| e.path == "persisted")
        .unwrap()
        .backing
        .clone()
        .unwrap();
    assert_eq!(
        snapshot
            .iter()
            .filter(|e| e.metadata.inode == inode)
            .count(),
        2
    );
    let mut handle = namespace.open("persisted", options()).unwrap();
    namespace.unlink("moved/renamed").unwrap();
    namespace.unlink("persisted").unwrap();
    assert_eq!(handle.metadata().unwrap().links, 0);
    assert!(work.0.join(&backing).exists());
    let mut bytes = [0; 12];
    assert_eq!(handle.read(&mut bytes).unwrap(), 12);
    assert_eq!(&bytes, b"shared bytes");
    drop(handle);
    assert!(!work.0.join(backing).exists());
    assert!(namespace.take_errors().is_empty());
}

#[test]
fn hard_link_replacement_retains_other_target_names_and_open_descriptors() {
    let work = Workspace::new();
    let namespace = work.open();
    write(&namespace, "source", b"new");
    write(&namespace, "target", b"old");
    namespace
        .hard_link("source", "source-alias", false)
        .unwrap();
    namespace
        .hard_link("target", "target-alias", false)
        .unwrap();
    let mut target = namespace.open("target", options()).unwrap();
    let target_inode = target.metadata().unwrap().inode;
    namespace.rename("source-alias", "target").unwrap();
    assert_eq!(namespace.metadata("source").unwrap().links, 2);
    assert_eq!(
        namespace.metadata("target-alias").unwrap().inode,
        target_inode
    );
    assert_eq!(target.metadata().unwrap().links, 1);
    assert_eq!(read(&namespace, "target-alias"), b"old");
    assert_eq!(read(&namespace, "target"), b"new");
    namespace.unlink("target-alias").unwrap();
    assert_eq!(target.metadata().unwrap().links, 0);
}

#[test]
fn importing_physical_hard_links_uses_one_backing_and_reclaims_all_raw_names() {
    let work = Workspace::new();
    fs::create_dir(work.0.join("dir")).unwrap();
    fs::write(work.0.join("file"), b"physical").unwrap();
    fs::hard_link(work.0.join("file"), work.0.join("dir/alias")).unwrap();
    let namespace = work.open();
    let info = namespace.metadata("file").unwrap();
    assert_eq!(info.links, 2);
    assert_eq!(namespace.metadata("dir/alias").unwrap().inode, info.inode);
    assert_eq!(read(&namespace, "dir/alias"), b"physical");
    assert_ne!(
        work.0.join("file").exists(),
        work.0.join("dir/alias").exists()
    );
    drop(namespace);
    let namespace = work.open();
    assert_eq!(read(&namespace, "file"), b"physical");
    namespace.unlink("file").unwrap();
    namespace.unlink("dir/alias").unwrap();
    assert!(!work.0.join("file").exists());
    assert!(!work.0.join("dir/alias").exists());
}

#[test]
fn structured_hard_link_mutations_match_independent_groups_across_restarts() {
    let work = Workspace::new();
    let mut namespace = work.open();
    let mut names = BTreeMap::<String, u64>::new();
    let mut contents = BTreeMap::<u64, Vec<u8>>::new();
    let mut random = 731u64;
    let mut next_group = 1;
    for step in 0..180 {
        random = random.wrapping_mul(6364136223846793005).wrapping_add(193);
        let source = format!("name-{}", (random >> 24) % 11);
        let target = format!("name-{}", (random >> 36) % 11);
        match (random >> 48) % 4 {
            0 => {
                let group = *names.entry(source.clone()).or_insert_with(|| {
                    next_group += 1;
                    next_group
                });
                let bytes = format!("step-{step}-group-{group}").into_bytes();
                write(&namespace, &source, &bytes);
                contents.insert(group, bytes);
            }
            1 => {
                if let Some(&group) = names.get(&source) {
                    let result = namespace.hard_link(&source, &target, false);
                    if let std::collections::btree_map::Entry::Vacant(entry) = names.entry(target) {
                        result.unwrap();
                        entry.insert(group);
                    } else {
                        assert_eq!(result, Err(FileError::Exists));
                    }
                }
            }
            2 => {
                if let Some(&group) = names.get(&source) {
                    namespace.rename(&source, &target).unwrap();
                    if names.get(&target) != Some(&group) {
                        names.remove(&source);
                        names.insert(target, group);
                    }
                }
            }
            _ => {
                if names.remove(&source).is_some() {
                    namespace.unlink(&source).unwrap();
                }
            }
        }
        if step % 7 == 0 {
            namespace.flush().unwrap();
            drop(namespace);
            namespace = work.open();
        }
        let mut identities = BTreeMap::new();
        for (name, &group) in &names {
            let info = namespace.metadata(name).unwrap();
            if let Some(inode) = identities.insert(group, info.inode) {
                assert_eq!(inode, info.inode, "step {step}: {name}");
            }
            assert_eq!(
                info.links,
                names.values().filter(|&&id| id == group).count() as u64
            );
            assert_eq!(
                read(&namespace, name),
                contents[&group],
                "step {step}: {name}"
            );
        }
        let unique: std::collections::BTreeSet<_> = identities.values().collect();
        assert_eq!(unique.len(), identities.len());
        assert_eq!(
            namespace
                .snapshot()
                .unwrap()
                .iter()
                .filter(|entry| !entry.path.is_empty())
                .count(),
            names.len()
        );
    }
}

#[test]
fn hard_links_to_symlinks_resolve_relative_to_each_entry_and_validate_before_effects() {
    let work = Workspace::new();
    let namespace = work.open();
    namespace.create_directory("a").unwrap();
    namespace.create_directory("b").unwrap();
    write(&namespace, "a/file", b"A");
    write(&namespace, "b/file", b"B");
    namespace.symlink("file", "a/link").unwrap();
    namespace.hard_link("a/link", "b/link", false).unwrap();
    assert_eq!(read(&namespace, "a/link"), b"A");
    assert_eq!(read(&namespace, "b/link"), b"B");
    assert_eq!(namespace.symlink_metadata("b/link").unwrap().links, 2);
    namespace.hard_link("b/link", "followed", true).unwrap();
    assert_eq!(read(&namespace, "followed"), b"B");
    assert_eq!(namespace.metadata("b/file").unwrap().links, 2);
    namespace.symlink("missing", "dangling").unwrap();
    namespace
        .hard_link("dangling", "dangling-alias", false)
        .unwrap();
    assert_eq!(namespace.read_link("dangling-alias").unwrap(), "missing");
    for (source, destination, follow, expected) in [
        ("dangling", "absent", true, FileError::NotFound),
        ("a", "directory-link", false, FileError::NotPermitted),
        ("a/file", "b/file", false, FileError::Exists),
        ("a/file", "bad/", false, FileError::NotDirectory),
        ("a/file", "missing/file", false, FileError::NotFound),
    ] {
        assert_eq!(
            namespace.hard_link(source, destination, follow),
            Err(expected)
        );
    }
    namespace.set_mode("b", 0o555).unwrap();
    assert_eq!(
        namespace.hard_link("a/file", "b/denied", false),
        Err(FileError::Access)
    );
    let other = Workspace::new().open();
    assert_eq!(
        namespace.hard_link_to("a/file", &other, "cross", false),
        Err(FileError::CrossDevice)
    );
    assert_eq!(namespace.metadata("a/file").unwrap().links, 1);
    assert_eq!(namespace.metadata("b/file").unwrap().links, 2);
}

#[cfg(windows)]
fn host_allocated(path: &std::path::Path) -> u64 {
    use std::os::windows::ffi::OsStrExt;
    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut high = 0;
    // Query by path, independently of the provider's handle information query.
    let low = unsafe {
        windows_sys::Win32::Storage::FileSystem::GetCompressedFileSizeW(path.as_ptr(), &mut high)
    };
    assert_ne!(low, u32::MAX, "{}", std::io::Error::last_os_error());
    (u64::from(high) << 32) | u64::from(low)
}

#[test]
fn allocation_preserves_host_measurements_across_mutations_and_open_unlink() {
    assert_eq!(
        efi_agent_filesystem::Metadata::new(EntryType::File, 8193).allocated,
        None
    );
    let workspace = Workspace::new();
    let namespace = workspace.open();
    namespace.create_directory("data").unwrap();
    namespace.symlink("data/input", "alias").unwrap();
    assert_eq!(namespace.metadata("").unwrap().links, 3);
    assert_eq!(namespace.metadata("data").unwrap().links, 2);
    assert_eq!(namespace.metadata("data").unwrap().allocated, Some(0));
    assert_eq!(
        namespace.symlink_metadata("alias").unwrap().allocated,
        Some(0)
    );
    let mut handle = namespace.open("data/input", options()).unwrap();
    for size in [0, 1, 511, 512, 513, 4095, 4096, 4097, 65537] {
        handle.resize(0).unwrap();
        handle.seek(0, 0).unwrap();
        assert_eq!(handle.write(&vec![193; size]).unwrap(), size);
        handle.flush().unwrap();
        let snapshot = efi_agent_filesystem::namespace::read_snapshot(workspace.raw().as_ref())
            .unwrap()
            .unwrap();
        let entry = snapshot
            .iter()
            .find(|entry| entry.path == "data/input")
            .unwrap();
        let expected = host_allocated(&workspace.0.join(entry.backing.as_ref().unwrap()));
        assert_eq!(entry.metadata.allocated, Some(expected), "size {size}");
        assert_eq!(
            namespace.metadata("alias").unwrap().allocated,
            Some(expected)
        );
        assert_eq!(handle.metadata().unwrap().allocated, Some(expected));
    }
    let allocated = handle.metadata().unwrap().allocated;
    namespace.rename("data", "renamed").unwrap();
    namespace.unlink("renamed/input").unwrap();
    assert_eq!(handle.metadata().unwrap().allocated, allocated);
    assert_eq!(handle.metadata().unwrap().links, 0);
    handle.resize(0).unwrap();
    handle.flush().unwrap();
    assert_eq!(handle.metadata().unwrap().allocated, Some(0));
    namespace.create_directory("renamed/child").unwrap();
    assert_eq!(namespace.metadata("renamed").unwrap().links, 3);
    namespace.rename("renamed/child", "child").unwrap();
    assert_eq!(namespace.metadata("renamed").unwrap().links, 2);
    assert_eq!(namespace.metadata("").unwrap().links, 4);
    namespace.remove_directory("child").unwrap();
    assert_eq!(namespace.metadata("").unwrap().links, 3);
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
fn directory_paths_follow_inode_identity_through_ancestor_moves_and_replacement() {
    let work = Workspace::new();
    let namespace = work.open();
    namespace.create_directory("parent").unwrap();
    namespace.create_directory("parent/child").unwrap();
    namespace.create_directory("other").unwrap();
    namespace.symlink("parent/child", "alias").unwrap();
    let directory = namespace.directory_view("alias").unwrap().unwrap();
    let inode = directory.metadata("").unwrap().inode;
    assert_eq!(namespace.directory_path().unwrap(), "");
    assert_eq!(directory.directory_path().unwrap(), "parent/child");

    namespace.rename("parent", "other/moved").unwrap();
    namespace.create_directory("parent").unwrap();
    namespace.create_directory("parent/child").unwrap();
    assert_eq!(directory.directory_path().unwrap(), "other/moved/child");
    assert_eq!(directory.metadata("").unwrap().inode, inode);
    assert_ne!(namespace.metadata("parent/child").unwrap().inode, inode);
    assert_eq!(directory.metadata("..").unwrap_err(), FileError::NotCapable);

    // Replacing the retained directory detaches that inode, even though a
    // different directory now has the exact path it previously reported.
    namespace
        .rename("parent/child", "other/moved/child")
        .unwrap();
    assert_eq!(directory.directory_path(), Err(FileError::NotFound));
    assert_eq!(directory.metadata("").unwrap().inode, inode);
    assert_eq!(directory.metadata("").unwrap().links, 0);
    namespace.remove_directory("other/moved/child").unwrap();
    namespace.create_directory("other/moved/child").unwrap();
    assert_eq!(directory.directory_path(), Err(FileError::NotFound));
    drop(directory);
    namespace.flush().unwrap();
    assert!(namespace.take_errors().is_empty());
    drop(namespace);
    assert!(
        work.open()
            .metadata("other/moved/child")
            .unwrap()
            .is_directory()
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

#[test]
fn hard_link_import_and_link_creation_recover_after_failed_commits() {
    let work = Workspace::new();
    fs::write(work.0.join("source"), b"preserved").unwrap();
    fs::hard_link(work.0.join("source"), work.0.join("alias")).unwrap();
    let faults = Rc::new(Faults {
        remaining: Cell::new(None),
        flush: Cell::new(true),
    });
    let raw: Rc<dyn FileSystem> = Rc::new(FaultFs {
        raw: work.raw(),
        faults: faults.clone(),
    });
    // Initialization wrote a complete generation but its flush failed. Physical
    // aliases must remain until recovery confirms their committed logical names.
    assert_eq!(
        Namespace::open(raw.clone(), None).err(),
        Some(FileError::Io)
    );
    assert!(work.0.join("source").exists() && work.0.join("alias").exists());
    let namespace = Namespace::open(raw.clone(), None).unwrap();
    assert_eq!(namespace.metadata("source").unwrap().links, 2);
    assert_eq!(read(&namespace, "alias"), b"preserved");
    assert_ne!(
        work.0.join("source").exists(),
        work.0.join("alias").exists()
    );
    faults.remaining.set(Some(32));
    assert_eq!(
        namespace.hard_link("source", "incomplete", false),
        Err(FileError::Io)
    );
    drop(namespace);
    faults.remaining.set(None);
    let namespace = Namespace::open(raw, None).unwrap();
    assert_eq!(
        namespace.metadata("incomplete").unwrap_err(),
        FileError::NotFound
    );
    assert_eq!(namespace.metadata("source").unwrap().links, 2);
    assert_eq!(read(&namespace, "alias"), b"preserved");
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
