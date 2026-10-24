use efi_agent_wasm::filesystem::{FileError, FileSystem, VirtualFileSystem, namespace::Namespace};
use std::rc::Rc;

#[test]
fn mounted_directory_paths_follow_anchors_without_losing_mount_prefixes() {
    let base = Rc::new(Namespace::temporary(1024, 32, 0, None));
    let root = VirtualFileSystem::new(base).unwrap();
    assert_eq!(root.directory_path().unwrap(), "");
    for path in ["bin", "dev", "proc", "tmp"] {
        let directory = root.directory_view(path).unwrap().unwrap();
        assert_eq!(directory.directory_path().unwrap(), path);
    }
    for prefix in ["", "tmp/"] {
        let parent = format!("{prefix}parent");
        let child = format!("{parent}/child");
        let moved = format!("{prefix}moved");
        root.create_directory(&parent).unwrap();
        root.create_directory(&child).unwrap();
        let directory = root.directory_view(&child).unwrap().unwrap();
        let inode = directory.metadata("").unwrap().inode;
        assert_eq!(directory.directory_path().unwrap(), child);

        root.rename(&parent, &moved).unwrap();
        root.create_directory(&parent).unwrap();
        root.create_directory(&child).unwrap();
        assert_eq!(
            directory.directory_path().unwrap(),
            format!("{moved}/child")
        );
        assert_eq!(directory.metadata("").unwrap().inode, inode);
        assert_eq!(directory.metadata("..").unwrap_err(), FileError::NotCapable);

        // Names that match mounts below an ordinary anchor stay ordinary.
        directory.create_directory("tmp").unwrap();
        let nested = directory.directory_view("tmp").unwrap().unwrap();
        assert_eq!(
            nested.directory_path().unwrap(),
            format!("{moved}/child/tmp")
        );
        directory.remove_directory("tmp").unwrap();
        assert_eq!(nested.directory_path(), Err(FileError::NotFound));
        root.remove_directory(&format!("{moved}/child")).unwrap();
        assert_eq!(directory.directory_path(), Err(FileError::NotFound));
        assert_eq!(directory.metadata("").unwrap().inode, inode);
        assert_ne!(root.metadata(&child).unwrap().inode, inode);
    }
}
