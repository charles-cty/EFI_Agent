use efi_agent_wasm::filesystem::{
    EntryType, FileError, FileSystem, OpenOptions, TimeUpdate, VirtualFileSystem,
    namespace::Namespace, native::NativeFileSystem, text,
};
use std::{
    fs,
    path::PathBuf,
    rc::Rc,
    time::{SystemTime, UNIX_EPOCH},
};

struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "efi-mount-paths-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn filesystem(&self) -> Rc<VirtualFileSystem> {
        VirtualFileSystem::new(Rc::new(
            Namespace::open(Rc::new(NativeFileSystem(self.0.clone())), None).unwrap(),
        ))
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn read(filesystem: &dyn FileSystem, path: &str) -> Result<String, FileError> {
    let mut file = filesystem.open(
        path,
        OpenOptions {
            read: true,
            write: false,
            create: false,
            exclusive: false,
            truncate: false,
            append: false,
        },
    )?;
    let mut output = Vec::new();
    loop {
        let mut buffer = [0; 193];
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        output.extend_from_slice(&buffer[..count]);
    }
    Ok(String::from_utf8(output).unwrap())
}

#[test]
fn links_cross_mounts_and_parent_components_follow_the_resolved_directory() {
    let work = Workspace::new();
    let filesystem = work.filesystem();
    filesystem.create_directory("dir").unwrap();
    filesystem.create_directory("tmp/inside").unwrap();
    text::write(filesystem.as_ref(), "dir/data", "workspace").unwrap();
    text::write(filesystem.as_ref(), "tmp/data", "temporary").unwrap();
    filesystem.symlink("/tmp/inside", "absolute").unwrap();
    filesystem.symlink("../tmp/inside", "dir/relative").unwrap();
    filesystem.symlink("/dir", "tmp/absolute").unwrap();
    filesystem.symlink("../dir", "tmp/relative").unwrap();
    for path in ["absolute/../data", "dir/relative/../data"] {
        assert_eq!(read(filesystem.as_ref(), path).unwrap(), "temporary");
        assert_eq!(filesystem.filesystem_statistics(path).unwrap().id, 3);
    }
    for path in ["tmp/absolute/data", "tmp/relative/data", "tmp/../dir/data"] {
        assert_eq!(read(filesystem.as_ref(), path).unwrap(), "workspace");
        assert_eq!(filesystem.filesystem_statistics(path).unwrap().id, 0);
    }
    filesystem.symlink("/", "tmp/root").unwrap();
    assert_eq!(
        read(filesystem.as_ref(), "tmp/root/dir/data").unwrap(),
        "workspace"
    );
    assert_eq!(
        filesystem.metadata("tmp/root").unwrap().inode,
        filesystem.metadata("").unwrap().inode
    );
    assert_eq!(
        filesystem.symlink_metadata("tmp/root").unwrap().kind,
        EntryType::Symlink
    );
    assert_eq!(
        filesystem.metadata("dir/data/../data").err(),
        Some(FileError::NotDirectory)
    );
    assert_eq!(
        filesystem.metadata("missing/../dir/data").err(),
        Some(FileError::NotFound)
    );
    filesystem.create_directory("denied").unwrap();
    filesystem.set_mode("denied", 0o600).unwrap();
    assert_eq!(
        filesystem.metadata("denied/../dir/data").err(),
        Some(FileError::Access)
    );
    filesystem.set_mode("denied", 0o700).unwrap();
    let root = filesystem.directory_view(".").unwrap().unwrap();
    assert_eq!(read(root.as_ref(), "tmp/data").unwrap(), "temporary");
    assert_eq!(
        filesystem.metadata("../dir/data").err(),
        Some(FileError::NotCapable)
    );
}

#[test]
fn mutation_follows_parent_links_and_preserves_the_final_directory_entry() {
    let work = Workspace::new();
    let filesystem = work.filesystem();
    filesystem.create_directory("dir").unwrap();
    filesystem.symlink("/tmp", "to-tmp").unwrap();
    filesystem.symlink("/dir", "tmp/to-work").unwrap();
    text::write(filesystem.as_ref(), "to-tmp/data", "original").unwrap();
    filesystem.symlink("/tmp/data", "alias").unwrap();
    filesystem.set_mode("alias", 0o640).unwrap();
    filesystem
        .set_times("alias", true, TimeUpdate::Set(19), TimeUpdate::Set(43))
        .unwrap();
    filesystem
        .set_times("alias", false, TimeUpdate::Set(71), TimeUpdate::Set(93))
        .unwrap();
    assert_eq!(filesystem.metadata("tmp/data").unwrap().mode, 0o640);
    assert_eq!(filesystem.metadata("tmp/data").unwrap().modified, 43);
    assert_eq!(filesystem.symlink_metadata("alias").unwrap().modified, 93);
    filesystem.hard_link("alias", "alias-copy", false).unwrap();
    assert_eq!(filesystem.read_link("alias-copy").unwrap(), "/tmp/data");
    assert_eq!(
        filesystem.hard_link("alias", "cross", true),
        Err(FileError::CrossDevice)
    );
    assert_eq!(
        filesystem.rename("to-tmp/data", "dir/cross"),
        Err(FileError::CrossDevice)
    );
    filesystem.rename("alias", "renamed-alias").unwrap();
    assert_eq!(filesystem.read_link("renamed-alias").unwrap(), "/tmp/data");
    filesystem.unlink("renamed-alias").unwrap();
    assert_eq!(read(filesystem.as_ref(), "tmp/data").unwrap(), "original");
    filesystem.symlink("/tmp/missing", "dangling").unwrap();
    text::write(filesystem.as_ref(), "dangling", "created").unwrap();
    assert_eq!(read(filesystem.as_ref(), "tmp/missing").unwrap(), "created");
    filesystem.create_directory("tmp/to-work/child").unwrap();
    text::write(filesystem.as_ref(), "tmp/to-work/child/value", "saved").unwrap();
    assert_eq!(
        read(filesystem.as_ref(), "dir/child/value").unwrap(),
        "saved"
    );
    assert_eq!(
        filesystem.remove_directory("tmp/to-work/child"),
        Err(FileError::NotEmpty)
    );
    filesystem.unlink("tmp/to-work/child/value").unwrap();
    filesystem.remove_directory("tmp/to-work/child").unwrap();
}

#[test]
fn directory_views_retain_bounds_when_global_links_leave_their_mount() {
    let work = Workspace::new();
    let filesystem = work.filesystem();
    filesystem.create_directory("tmp/dir").unwrap();
    text::write(filesystem.as_ref(), "outside", "untouched").unwrap();
    filesystem.symlink("/outside", "tmp/dir/absolute").unwrap();
    filesystem
        .symlink("../../outside", "tmp/dir/relative")
        .unwrap();
    let view = filesystem.directory_view("tmp/dir").unwrap().unwrap();
    filesystem.rename("tmp/dir", "tmp/renamed").unwrap();
    assert_eq!(
        read(filesystem.as_ref(), "tmp/renamed/absolute").unwrap(),
        "untouched"
    );
    for path in ["absolute", "relative", "../outside"] {
        assert_eq!(view.metadata(path).err(), Some(FileError::NotCapable));
    }
    let tmp_root = filesystem.directory_view("tmp").unwrap().unwrap();
    assert_eq!(
        tmp_root.metadata("renamed/absolute").err(),
        Some(FileError::NotCapable)
    );
    filesystem.create_directory("local").unwrap();
    filesystem.create_directory("local/tmp").unwrap();
    text::write(filesystem.as_ref(), "local/tmp/data", "local").unwrap();
    let local = filesystem.directory_view("local").unwrap().unwrap();
    assert_eq!(read(local.as_ref(), "tmp/data").unwrap(), "local");
}

#[test]
fn link_limit_is_shared_across_mounts_and_preserves_trailing_directory_requirements() {
    let work = Workspace::new();
    let filesystem = work.filesystem();
    text::write(filesystem.as_ref(), "end", "leaf").unwrap();
    let name = |index| {
        if index % 2 == 0 {
            format!("link-{index}")
        } else {
            format!("tmp/link-{index}")
        }
    };
    for index in (0..41).rev() {
        let target = if index == 40 {
            "end".into()
        } else {
            name(index + 1)
        };
        filesystem
            .symlink(&format!("/{target}"), &name(index))
            .unwrap();
    }
    assert_eq!(read(filesystem.as_ref(), &name(1)).unwrap(), "leaf");
    assert_eq!(filesystem.metadata(&name(0)).err(), Some(FileError::Loop));
    assert_eq!(
        filesystem.symlink_metadata(&name(0)).unwrap().kind,
        EntryType::Symlink
    );
    filesystem.symlink("/end/", "trailing-target").unwrap();
    assert_eq!(
        filesystem.metadata("trailing-target").err(),
        Some(FileError::NotDirectory)
    );
    assert_eq!(
        filesystem.symlink_metadata("tmp/link-1/").err(),
        Some(FileError::NotDirectory)
    );
    filesystem.symlink("/tmp", "directory-link").unwrap();
    assert_eq!(
        filesystem.symlink_metadata("directory-link/").unwrap().kind,
        EntryType::Directory
    );
    assert_eq!(filesystem.read_link("directory-link").unwrap(), "/tmp");
}

#[test]
fn expanded_paths_accept_the_exact_limit_and_reject_the_next_byte() {
    let work = Workspace::new();
    let filesystem = work.filesystem();
    let mut parent = String::new();
    for byte in ['a', 'b', 'c'] {
        if !parent.is_empty() {
            parent.push('/');
        }
        parent.extend(std::iter::repeat_n(byte, 1024));
        filesystem.create_directory(&parent).unwrap();
    }
    let path = format!("{parent}/{}", "d".repeat(1021));
    assert_eq!(path.len(), 4096);
    text::write(filesystem.as_ref(), &path, "boundary").unwrap();
    filesystem.symlink(&path, "long-link").unwrap();
    assert_eq!(read(filesystem.as_ref(), "long-link").unwrap(), "boundary");
    let oversized = format!("{path}x");
    assert_eq!(
        filesystem.metadata(&oversized).err(),
        Some(FileError::ResourceLimit)
    );
    assert_eq!(
        filesystem.symlink(&oversized, "oversized-link"),
        Err(FileError::ResourceLimit)
    );
    assert_eq!(
        filesystem.metadata("oversized-link").err(),
        Some(FileError::NotFound)
    );
}
