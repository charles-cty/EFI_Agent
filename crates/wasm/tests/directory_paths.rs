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
        assert_eq!(
            directory
                .parent_directory()
                .unwrap()
                .directory_path()
                .unwrap(),
            ""
        );
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
        assert_eq!(
            directory
                .parent_directory()
                .unwrap()
                .directory_path()
                .unwrap(),
            moved
        );

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

#[test]
fn runtime_parent_views_keep_mount_routing_after_detached_ancestry() {
    let root = VirtualFileSystem::new(Rc::new(Namespace::temporary(1024, 32, 0, None))).unwrap();
    for prefix in ["", "tmp/"] {
        let parent_path = format!("{prefix}parent");
        let child_path = format!("{parent_path}/child");
        root.create_directory(&parent_path).unwrap();
        root.create_directory(&child_path).unwrap();
        let child = root.directory_view(&child_path).unwrap().unwrap();
        let parent_inode = root.metadata(&parent_path).unwrap().inode;
        root.remove_directory(&child_path).unwrap();
        root.remove_directory(&parent_path).unwrap();
        root.create_directory(&parent_path).unwrap();
        let parent = child.parent_directory().unwrap();
        assert_eq!(parent.metadata("").unwrap().inode, parent_inode);
        assert_eq!(parent.directory_path(), Err(FileError::NotFound));
        let mount = parent.parent_directory().unwrap();
        assert_eq!(
            mount.directory_path().unwrap(),
            prefix.trim_end_matches('/')
        );
        let returned_root = mount.parent_directory().unwrap();
        assert_eq!(returned_root.directory_path().unwrap(), "");
        assert!(returned_root.metadata("dev/null").is_ok());
        assert!(returned_root.metadata("tmp").unwrap().is_directory());
        assert_eq!(parent.metadata("..").unwrap_err(), FileError::NotCapable);
    }
}
