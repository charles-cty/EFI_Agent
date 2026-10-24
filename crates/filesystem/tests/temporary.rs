use efi_agent_filesystem::{
    EntryType, FileError, FileSystem, OpenOptions, TimeUpdate, namespace::Namespace,
};
use std::{cell::Cell, rc::Rc};

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
fn temporary_names_do_not_reserve_persistent_journal_paths() {
    let filesystem = Namespace::temporary(32, 8, 3, None);
    filesystem.create_directory(".efi-agent-fs").unwrap();
    let mut file = filesystem.open(".efi-agent-fs/data", options()).unwrap();
    file.write(b"ordinary bytes").unwrap();
    assert_eq!(filesystem.metadata(".efi-agent-fs/data").unwrap().size, 14);
    drop(filesystem.open(".efi-process", options()).unwrap());
    assert_eq!(
        filesystem.metadata(".efi-process").unwrap().kind,
        EntryType::File
    );
}

#[test]
fn detached_directory_ancestry_tracks_moves_and_releases_the_full_chain() {
    let filesystem = Namespace::temporary(32, 8, 3, None);
    filesystem.create_directory("parent").unwrap();
    filesystem.create_directory("parent/child").unwrap();
    filesystem.create_directory("elsewhere").unwrap();
    let child = filesystem.directory_view("parent/child").unwrap().unwrap();
    let parent_inode = filesystem.metadata("parent").unwrap().inode;
    let child_inode = child.metadata("").unwrap().inode;
    filesystem.remove_directory("parent/child").unwrap();
    filesystem.rename("parent", "elsewhere/moved").unwrap();
    filesystem.create_directory("parent").unwrap();
    let parent = child.parent_directory().unwrap();
    assert_eq!(parent.metadata("").unwrap().inode, parent_inode);
    assert_eq!(parent.directory_path().unwrap(), "elsewhere/moved");
    drop(parent);

    filesystem.remove_directory("elsewhere/moved").unwrap();
    filesystem.remove_directory("elsewhere").unwrap();
    filesystem.remove_directory("parent").unwrap();
    assert_eq!(child.metadata("").unwrap().inode, child_inode);
    assert_eq!(child.directory_path(), Err(FileError::NotFound));
    let parent = child.parent_directory().unwrap();
    assert_eq!(parent.metadata("").unwrap().inode, parent_inode);
    assert_eq!(parent.directory_path(), Err(FileError::NotFound));
    let ancestor = parent.parent_directory().unwrap();
    assert_eq!(ancestor.directory_path(), Err(FileError::NotFound));
    let root = ancestor.parent_directory().unwrap();
    assert_eq!(root.directory_path().unwrap(), "");
    assert_eq!(
        root.parent_directory().unwrap().directory_path().unwrap(),
        ""
    );
    assert_eq!(child.metadata("..").unwrap_err(), FileError::NotCapable);
    assert_eq!(filesystem.filesystem_statistics("").unwrap().files_free, 4);
    drop(child);
    drop(parent);
    drop(ancestor);
    assert_eq!(filesystem.filesystem_statistics("").unwrap().files_free, 7);
    assert!(filesystem.take_errors().is_empty());
}

#[test]
fn detached_parent_references_are_counted_per_child_and_release_iteratively() {
    let filesystem = Namespace::temporary(32, 128, 3, None);
    filesystem.create_directory("a").unwrap();
    filesystem.create_directory("a/left").unwrap();
    filesystem.create_directory("a/right").unwrap();
    let left = filesystem.directory_view("a/left").unwrap().unwrap();
    let right = filesystem.directory_view("a/right").unwrap().unwrap();
    filesystem.remove_directory("a/left").unwrap();
    filesystem.remove_directory("a/right").unwrap();
    filesystem.remove_directory("a").unwrap();
    drop(left);
    assert_eq!(
        filesystem.filesystem_statistics("").unwrap().files_free,
        125
    );
    assert_eq!(
        right
            .parent_directory()
            .unwrap()
            .metadata("")
            .unwrap()
            .links,
        0
    );
    drop(right);
    assert_eq!(
        filesystem.filesystem_statistics("").unwrap().files_free,
        127
    );

    let mut paths = Vec::new();
    let mut path = String::new();
    for _ in 0..96 {
        if !path.is_empty() {
            path.push('/');
        }
        path.push('d');
        filesystem.create_directory(&path).unwrap();
        paths.push(path.clone());
    }
    let deepest = filesystem.directory_view(&path).unwrap().unwrap();
    for path in paths.iter().rev() {
        filesystem.remove_directory(path).unwrap();
    }
    assert_eq!(filesystem.filesystem_statistics("").unwrap().files_free, 31);
    drop(deepest);
    assert_eq!(
        filesystem.filesystem_statistics("").unwrap().files_free,
        127
    );
    assert!(filesystem.take_errors().is_empty());
}

#[test]
fn temporary_tree_preserves_identity_metadata_and_renamed_directory_anchors() {
    let filesystem = Namespace::temporary(128, 16, 3, Some(Rc::new(|| Ok(731))));
    filesystem.create_directory("dir").unwrap();
    let view = filesystem.directory_view("dir").unwrap().unwrap();
    let mut file = view.open("data", options()).unwrap();
    file.write(b"before").unwrap();
    let identity = file.metadata().unwrap();
    assert_eq!(identity.device, 3);
    assert_ne!(identity.inode, 0);
    assert_eq!(identity.modified, 731);
    filesystem.rename("dir", "renamed").unwrap();
    assert_eq!(filesystem.metadata("dir").unwrap_err(), FileError::NotFound);
    view.hard_link("data", "alias", false).unwrap();
    view.symlink("alias", "link").unwrap();
    assert_eq!(view.read_link("link").unwrap(), "alias");
    assert_eq!(
        view.symlink_metadata("link").unwrap().kind,
        EntryType::Symlink
    );
    assert_eq!(
        filesystem.metadata("renamed/link").unwrap().inode,
        identity.inode
    );
    view.set_mode("data", 0o600).unwrap();
    file.set_times(TimeUpdate::Set(19), TimeUpdate::Set(43))
        .unwrap();
    let linked = filesystem.metadata("renamed/alias").unwrap();
    assert_eq!(
        (linked.links, linked.mode, linked.accessed, linked.modified),
        (2, 0o600, 19, 43)
    );
    filesystem.unlink("renamed/data").unwrap();
    filesystem.unlink("renamed/alias").unwrap();
    assert_eq!(file.metadata().unwrap().links, 0);
    assert_eq!(view.metadata("link").unwrap_err(), FileError::NotFound);
    file.seek(0, 0).unwrap();
    let mut bytes = [0; 6];
    assert_eq!(file.read(&mut bytes).unwrap(), 6);
    assert_eq!(&bytes, b"before");
    assert_eq!(
        filesystem.remove_directory("renamed"),
        Err(FileError::NotEmpty)
    );
    view.unlink("link").unwrap();
    filesystem.remove_directory("renamed").unwrap();
    assert_eq!(view.metadata("").unwrap().links, 0);
    assert_eq!(view.open("new", options()).err(), Some(FileError::NotFound));
    drop(file);
    drop(view);
    let stats = filesystem.filesystem_statistics("").unwrap();
    assert_eq!((stats.files_free, stats.capacity.available), (15, 128));
}

#[test]
fn slots_include_directories_links_and_detached_open_inodes() {
    let filesystem = Namespace::temporary(32, 5, 3, None);
    filesystem.create_directory("dir").unwrap();
    let file = filesystem.open("dir/file", options()).unwrap();
    filesystem.hard_link("dir/file", "alias", false).unwrap();
    filesystem.symlink("dir/file", "link").unwrap();
    assert_eq!(filesystem.filesystem_statistics("").unwrap().files_free, 0);
    assert_eq!(filesystem.create_directory("full"), Err(FileError::NoSpace));
    assert_eq!(filesystem.symlink("dir", "full"), Err(FileError::NoSpace));
    assert_eq!(
        filesystem.hard_link("alias", "full", false),
        Err(FileError::NoSpace)
    );
    assert_eq!(
        filesystem.open("full", options()).err(),
        Some(FileError::NoSpace)
    );
    assert_eq!(
        filesystem.metadata("full").unwrap_err(),
        FileError::NotFound
    );
    filesystem.unlink("alias").unwrap();
    filesystem.unlink("dir/file").unwrap();
    assert_eq!(file.filesystem_statistics().unwrap().files_free, 1);
    drop(file);
    assert_eq!(filesystem.filesystem_statistics("").unwrap().files_free, 2);
    drop(filesystem.open("full", options()).unwrap());
    assert_eq!(filesystem.filesystem_statistics("").unwrap().files_free, 1);
}

#[test]
fn shared_byte_quota_short_writes_holes_and_last_close_match_exact_values() {
    let filesystem = Namespace::temporary(13, 8, 3, None);
    let mut first = filesystem.open("first", options()).unwrap();
    let mut second = filesystem.open("second", options()).unwrap();
    first.write(b"abcde").unwrap();
    second.seek(3, 0).unwrap();
    assert_eq!(second.write(b"0123456789").unwrap(), 5);
    assert_eq!(second.write(b"more"), Err(FileError::NoSpace));
    assert_eq!(second.resize(9), Err(FileError::NoSpace));
    assert_eq!(second.metadata().unwrap().size, 8);
    filesystem.unlink("first").unwrap();
    assert_eq!(
        second.filesystem_statistics().unwrap().capacity.available,
        0
    );
    drop(first);
    second.seek(0, 0).unwrap();
    let mut bytes = [0; 8];
    assert_eq!(second.read(&mut bytes).unwrap(), 8);
    assert_eq!(&bytes, b"\x00\x00\x0001234");
    assert_eq!(
        second.filesystem_statistics().unwrap().capacity.available,
        5
    );
    second.resize(3).unwrap();
    second.seek(900, 0).unwrap();
    assert_eq!(second.write(b""), Ok(0));
    assert_eq!(second.metadata().unwrap().size, 3);
    assert_eq!(filesystem.capacity().unwrap().available, 10);
}

#[test]
fn time_provider_failures_precede_mutation_and_can_be_replaced() {
    let failed = Rc::new(Cell::new(false));
    let filesystem = Namespace::temporary(
        64,
        8,
        3,
        Some(Rc::new({
            let failed = failed.clone();
            move || {
                if failed.get() {
                    Err(FileError::Io)
                } else {
                    Ok(731)
                }
            }
        })),
    );
    let mut file = filesystem.open("file", options()).unwrap();
    file.write(b"safe").unwrap();
    failed.set(true);
    assert_eq!(file.write(b"corrupt"), Err(FileError::Io));
    assert_eq!(file.resize(1), Err(FileError::Io));
    assert_eq!(filesystem.create_directory("bad"), Err(FileError::Io));
    assert_eq!(filesystem.metadata("bad").unwrap_err(), FileError::NotFound);
    assert_eq!(file.metadata().unwrap().size, 4);
    filesystem.set_clock(Some(Rc::new(|| Ok(193))));
    file.set_times(TimeUpdate::Keep, TimeUpdate::Now).unwrap();
    assert_eq!(file.metadata().unwrap().modified, 193);
    filesystem.set_clock(None);
    assert_eq!(
        file.set_times(TimeUpdate::Now, TimeUpdate::Keep),
        Err(FileError::Unsupported)
    );
    assert_eq!(file.metadata().unwrap().accessed, 731);
    assert_eq!(filesystem.take_errors(), []);
}

#[test]
fn structured_sparse_updates_match_independent_bytes_and_capacity() {
    let filesystem = Namespace::temporary(97, 8, 3, None);
    let mut file = filesystem.open("data", options()).unwrap();
    let mut expected = Vec::new();
    let mut random = 0x12345678_u32;
    for _ in 0..300 {
        random = random.wrapping_mul(1664525).wrapping_add(1013904223);
        let position = (random >> 16) as usize % 110;
        let count = (random >> 8) as usize % 23;
        file.seek(position as i64, 0).unwrap();
        let input: Vec<_> = (0..count)
            .map(|index| (index as u8).wrapping_add(random as u8))
            .collect();
        let actual = file.write(&input);
        let accepted = count.min(97_usize.saturating_sub(position));
        if count != 0 && accepted == 0 {
            assert_eq!(actual, Err(FileError::NoSpace));
        } else {
            assert_eq!(actual, Ok(accepted));
            if accepted != 0 {
                expected.resize(expected.len().max(position + accepted), 0);
                expected[position..position + accepted].copy_from_slice(&input[..accepted]);
            }
        }
        if random & 3 == 0 {
            let length = (random >> 3) as usize % 98;
            file.resize(length as u64).unwrap();
            expected.resize(length, 0);
        }
        file.seek(0, 0).unwrap();
        let mut actual = [0; 110];
        assert_eq!(file.read(&mut actual).unwrap(), expected.len());
        assert_eq!(&actual[..expected.len()], expected);
        assert_eq!(
            filesystem.capacity().unwrap().available,
            97 - expected.len() as u64
        );
    }
}
