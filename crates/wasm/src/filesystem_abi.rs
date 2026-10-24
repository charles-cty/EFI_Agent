use crate::installation::HostLinker;
use alloc::{rc::Rc, string::String};
use core::cell::{Cell, RefCell};
use wasmi::Caller;

use crate::{
    Context,
    abi::{memory, range, vectors},
    filesystem::{Directory, EntryType, FileError, OpenOptions, TimeUpdate},
    io::{self, Descriptor},
};

fn time_update(value: u64, flags: u32) -> TimeUpdate {
    match flags {
        0 => TimeUpdate::Keep,
        1 => TimeUpdate::Set(value),
        2 => TimeUpdate::Now,
        _ => unreachable!("Timestamp flags are validated before provider calls"),
    }
}

pub(super) fn control_statistics(
    context: &Context,
    fd: u32,
    path: Option<&str>,
) -> Result<[u8; 96], i32> {
    let descriptor = context.descriptors.get(&fd).ok_or(io::BADF)?;
    let statistics = match (descriptor, path) {
        (Descriptor::Directory(directory), path) => {
            let right = if path.is_some() { 1 << 18 } else { 1 << 21 };
            if directory.rights & right == 0 {
                return Err(76);
            }
            let path = match path {
                Some(path) => directory.resolve(path)?,
                None => directory.path.clone(),
            };
            directory.filesystem.filesystem_statistics(&path)
        }
        (Descriptor::File { handle, rights, .. }, None) => {
            if rights & (1 << 21) == 0 {
                return Err(76);
            }
            handle.borrow().filesystem_statistics()
        }
        (_, Some(_)) => return Err(54),
        _ => return Err(58),
    }
    .map_err(FileError::errno)?;
    statistics.encode().map_err(FileError::errno)
}

pub(super) fn control_mounts(context: &Context) -> Result<alloc::vec::Vec<u8>, i32> {
    let Descriptor::Directory(directory) = context.descriptors.get(&3).ok_or(io::BADF)? else {
        return Err(54);
    };
    let mounts = directory.filesystem.mounts().map_err(FileError::errno)?;
    let mut result = String::new();
    for mount in mounts {
        for field in [&mount.source, &mount.path, &mount.kind] {
            if field.is_empty() || field.contains('\0') {
                return Err(28);
            }
            for ch in field.chars() {
                match ch {
                    ' ' => result.push_str("\\040"),
                    '\t' => result.push_str("\\011"),
                    '\n' => result.push_str("\\012"),
                    '\\' => result.push_str("\\134"),
                    _ => result.push(ch),
                }
            }
            result.push(' ');
        }
        result.push_str(if mount.read_only {
            "ro 0 0\n"
        } else {
            "rw 0 0\n"
        });
        if result.len() > 65536 {
            return Err(48);
        }
    }
    Ok(result.into_bytes())
}

pub(super) fn control_hard_link(
    context: &Context,
    source_fd: u32,
    destination_fd: u32,
    flags: u32,
    source: &str,
    destination: &str,
) -> Result<(), i32> {
    let get_directory = |fd, right| match context.descriptors.get(&fd).ok_or(io::BADF)? {
        Descriptor::Directory(directory) if directory.rights & right == right => Ok(directory),
        Descriptor::Directory(_) => Err(76),
        _ => Err(54),
    };
    let old = get_directory(source_fd, 1 << 11)?;
    let new = get_directory(destination_fd, 1 << 12)?;
    if flags & !1 != 0 {
        return Err(28);
    }
    let source = old.resolve(source)?;
    let destination = new.resolve(destination)?;
    old.filesystem
        .hard_link_to(&source, new.filesystem.as_ref(), &destination, flags == 1)
        .map_err(FileError::errno)
}

/// Allocation is absent from Preview 1 filestat. Preserve provider errors and
/// distinguish unknown allocation from files with no allocated data blocks.
pub(super) fn control_allocation(
    context: &Context,
    fd: u32,
    flags: u32,
    path: &str,
) -> Result<u64, i32> {
    if flags & !1 != 0 {
        return Err(28);
    }
    let descriptor = context.descriptors.get(&fd).ok_or(io::BADF)?;
    let metadata = if path.is_empty() {
        match descriptor {
            Descriptor::File { handle, rights, .. } => {
                if rights & (1 << 21) == 0 {
                    return Err(76);
                }
                handle.borrow_mut().metadata().map_err(FileError::errno)?
            }
            Descriptor::Directory(directory) => {
                if directory.rights & (1 << 21) == 0 {
                    return Err(76);
                }
                directory
                    .filesystem
                    .metadata(&directory.path)
                    .map_err(FileError::errno)?
            }
            _ => return Ok(0),
        }
    } else {
        let Descriptor::Directory(directory) = descriptor else {
            return Err(54);
        };
        if directory.rights & (1 << 18) == 0 {
            return Err(76);
        }
        let path = if directory.preopen && path.trim_matches('/').is_empty() {
            directory.path.clone()
        } else {
            directory.resolve(path)?
        };
        if flags == 1 {
            directory.filesystem.metadata(&path)
        } else {
            directory.filesystem.symlink_metadata(&path)
        }
        .map_err(FileError::errno)?
    };
    metadata.allocated.ok_or(58)
}

/// Private metadata extension for interpreters whose Preview 1 libc has no
/// permission operations. Results use WASI errno values, like file syscalls.
pub(super) fn control_mode(
    context: &Context,
    operation: u32,
    fd: u32,
    flags: u32,
    mode: u32,
    path: &str,
) -> Result<u16, i32> {
    if flags & !1 != 0 || mode & !0o7777 != 0 {
        return Err(28);
    }
    let descriptor = context.descriptors.get(&fd).ok_or(io::BADF)?;
    let set = operation == 12 || operation == 14;
    if operation == 11 || operation == 12 {
        let Descriptor::Directory(directory) = descriptor else {
            return Err(54);
        };
        let right = if set { 1 << 20 } else { 1 << 18 };
        if directory.rights & right == 0 {
            return Err(76);
        }
        let path = if directory.preopen && !path.is_empty() && path.trim_matches('/').is_empty() {
            directory.path.clone()
        } else {
            directory.resolve(path)?
        };
        let metadata = if flags == 1 {
            directory.filesystem.metadata(&path)
        } else {
            directory.filesystem.symlink_metadata(&path)
        }
        .map_err(FileError::errno)?;
        if set {
            if metadata.kind == EntryType::Symlink {
                return Err(58);
            }
            directory
                .filesystem
                .set_mode(&path, mode as u16)
                .map_err(FileError::errno)?;
            Ok(0)
        } else {
            Ok(metadata.mode)
        }
    } else {
        let right = if set { 1 << 23 } else { 1 << 21 };
        match descriptor {
            Descriptor::File { handle, rights, .. } => {
                if rights & right == 0 {
                    return Err(76);
                }
                let mut handle = handle.borrow_mut();
                if set {
                    handle.set_mode(mode as u16).map_err(FileError::errno)?;
                    Ok(0)
                } else {
                    Ok(handle.metadata().map_err(FileError::errno)?.mode)
                }
            }
            Descriptor::Directory(directory) => {
                if directory.rights & right == 0 {
                    return Err(76);
                }
                if set {
                    directory
                        .filesystem
                        .set_mode(&directory.path, mode as u16)
                        .map_err(FileError::errno)?;
                    Ok(0)
                } else {
                    Ok(directory
                        .filesystem
                        .metadata(&directory.path)
                        .map_err(FileError::errno)?
                        .mode)
                }
            }
            _ if !set => Ok(0o666),
            _ => Err(58),
        }
    }
}

fn directory(caller: &Caller<'_, Context>, fd: u32, right: u64) -> Result<Directory, i32> {
    match caller.data().descriptors.get(&fd).ok_or(io::BADF)? {
        Descriptor::Directory(directory) if directory.rights & right == right => {
            Ok(directory.clone())
        }
        Descriptor::Directory(_) => Err(76),
        _ => Err(54),
    }
}

fn path(
    caller: &Caller<'_, Context>,
    directory: &Directory,
    ptr: u32,
    len: u32,
) -> Result<String, i32> {
    let memory = memory(caller)?;
    let bytes = &memory.data(caller)[range(ptr, len as usize, memory.data_size(caller))?];
    let text = core::str::from_utf8(bytes).map_err(|_| 25)?;
    directory.resolve(text)
}

fn readlink(
    mut caller: Caller<'_, Context>,
    fd: u32,
    ptr: u32,
    len: u32,
    buffer: u32,
    size: u32,
    used: u32,
) -> i32 {
    let result: Result<(), i32> = (|| {
        let directory = directory(&caller, fd, 1 << 15)?;
        let memory = memory(&caller)?;
        range(buffer, size as usize, memory.data_size(&caller))?;
        range(used, 4, memory.data_size(&caller))?;
        let path = path(&caller, &directory, ptr, len)?;
        let target = directory
            .filesystem
            .read_link(&path)
            .map_err(FileError::errno)?;
        let count = target.len().min(size as usize);
        memory
            .write(&mut caller, buffer as usize, &target.as_bytes()[..count])
            .map_err(|_| 21)?;
        memory
            .write(&mut caller, used as usize, &(count as u32).to_le_bytes())
            .map_err(|_| 21)
    })();
    result.err().unwrap_or(0)
}

#[allow(clippy::too_many_arguments)]
fn open(
    mut caller: Caller<'_, Context>,
    fd: u32,
    lookup: u32,
    ptr: u32,
    len: u32,
    oflags: u32,
    rights: u64,
    inheriting: u64,
    flags: u32,
    opened: u32,
) -> i32 {
    let result = (|| {
        let directory = directory(&caller, fd, 1 << 13)?;
        if lookup & !1 != 0 || oflags & !15 != 0 || flags & !31 != 0 {
            return Err(28);
        }
        if (rights | inheriting) & !directory.inheriting != 0 {
            return Err(76);
        }
        if (oflags & 1 != 0 && directory.rights & (1 << 10) == 0)
            || (oflags & 8 != 0 && directory.rights & (1 << 19) == 0)
        {
            return Err(76);
        }
        let memory = memory(&caller)?;
        range(opened, 4, memory.data_size(&caller))?;
        let path = path(&caller, &directory, ptr, len)?;
        if directory.preopen && matches!(path.as_str(), ".efi-process" | ".efi-session") {
            if oflags != 0 || flags != 0 || rights & ((1 << 1) | (1 << 6)) != ((1 << 1) | (1 << 6))
            {
                return Err(28);
            }
            let available = (4..io::MAX_DESCRIPTORS)
                .find(|fd| !caller.data().descriptors.contains_key(fd))
                .ok_or(io::MFILE)?;
            let descriptor = if path == ".efi-session" {
                let channel = caller.data().session.clone().ok_or(44)?;
                if channel.borrow().opened {
                    return Err(20);
                }
                channel.borrow_mut().opened = true;
                Descriptor::Session(channel)
            } else {
                Descriptor::Control(Rc::new(RefCell::new(alloc::collections::VecDeque::new())))
            };
            caller.data_mut().descriptors.insert(available, descriptor);
            caller.data_mut().close_on_exec.insert(available);
            memory
                .write(&mut caller, opened as usize, &available.to_le_bytes())
                .map_err(|_| 21)?;
            return Ok(());
        }
        let available = (0..io::MAX_DESCRIPTORS)
            .find(|fd| !caller.data().descriptors.contains_key(fd))
            .ok_or(io::MFILE)?;
        if lookup & 1 == 0 {
            match directory.filesystem.symlink_metadata(&path) {
                Ok(info) if info.kind == EntryType::Symlink => {
                    return Err(if oflags & 5 == 5 { 20 } else { 32 });
                }
                Ok(_) | Err(FileError::NotFound) => (),
                Err(error) => return Err(error.errno()),
            }
        }
        if let Some(filesystem) = (directory.filesystem.as_ref() as &dyn core::any::Any)
            .downcast_ref::<crate::filesystem::VirtualFileSystem>()
            && let Some(mut descriptor) = filesystem
                .descriptor_path(&path)
                .map_err(FileError::errno)?
        {
            if oflags & 5 == 5 {
                return Err(20);
            }
            if oflags & 2 != 0 && !matches!(descriptor, Descriptor::Directory(_)) {
                return Err(54);
            }
            if rights & (1 << 1) != 0
                && !descriptor.readable()
                && !matches!(descriptor, Descriptor::Directory(_))
            {
                return Err(2);
            }
            if rights & (1 << 6) != 0 && !descriptor.writable() {
                return Err(2);
            }
            // Descriptor links have fdescfs duplication semantics. Status flags
            // belong to the existing description; open does not change them.
            if flags & !5 != 0 {
                return Err(58);
            }
            match &mut descriptor {
                Descriptor::File {
                    rights: available,
                    read,
                    write,
                    ..
                } => {
                    *available &= rights;
                    *read &= rights & (1 << 1) != 0;
                    *write &= rights & (1 << 6) != 0;
                }
                Descriptor::Directory(directory) => {
                    directory.rights &= rights;
                    directory.inheriting &= inheriting;
                }
                _ => (),
            }
            caller.data_mut().descriptors.insert(available, descriptor);
            memory
                .write(&mut caller, opened as usize, &available.to_le_bytes())
                .map_err(|_| 21)?;
            return Ok(());
        }
        let (target_exists, target_kind) = match directory.filesystem.metadata(&path) {
            Ok(metadata) => (true, metadata.kind),
            Err(FileError::NotFound) if oflags & 1 != 0 && oflags & 2 == 0 => {
                (false, EntryType::File)
            }
            Err(failure) => return Err(failure.errno()),
        };
        if target_exists && oflags & 5 == 5 {
            return Err(20);
        }
        let descriptor = if oflags & 2 != 0 || target_kind == EntryType::Directory {
            if oflags & !2 != 0 {
                return Err(28);
            }
            // Directory enumeration completes synchronously. NONBLOCK does
            // not require a readiness wait, but remains visible in fdstat.
            if flags & !4 != 0 {
                return Err(58);
            }
            if target_kind != EntryType::Directory {
                return Err(54);
            }
            let view = directory
                .filesystem
                .directory_view(&path)
                .map_err(FileError::errno)?;
            let (filesystem, path) = match view {
                Some(filesystem) => (filesystem, String::new()),
                None => (directory.filesystem.clone(), path),
            };
            Descriptor::Directory(Directory {
                filesystem,
                path,
                preopen: false,
                rights: rights & io::DIRECTORY_RIGHTS,
                inheriting,
                entries: Rc::new(RefCell::new(None)),
                flags: flags as u16,
            })
        } else {
            let allowed_flags = if target_kind == EntryType::CharacterDevice {
                5
            } else {
                1
            };
            if flags & !allowed_flags != 0 {
                return Err(58);
            }
            let options = OpenOptions {
                read: rights & (1 << 1) != 0,
                write: rights & (1 << 6) != 0,
                create: oflags & 1 != 0,
                exclusive: oflags & 4 != 0,
                truncate: oflags & 8 != 0,
                append: flags & 1 != 0,
            };
            if (options.truncate || options.append) && !options.write {
                return Err(76);
            }
            if options.exclusive && !options.create {
                return Err(28);
            }
            let mut handle = directory
                .filesystem
                .open(&path, options)
                .map_err(FileError::errno)?;
            if !target_exists
                && (caller.data().creation_mode.is_some() || caller.data().umask != 0o022)
            {
                handle
                    .set_mode(caller.data().creation_mode.unwrap_or(0o666) & !caller.data().umask)
                    .map_err(FileError::errno)?;
            }
            Descriptor::File {
                handle: Rc::new(RefCell::new(handle)),
                read: options.read,
                write: options.write,
                rights: rights & io::FILE_RIGHTS,
                flags: Rc::new(Cell::new(flags as u16)),
            }
        };
        caller.data_mut().descriptors.insert(available, descriptor);
        memory
            .write(&mut caller, opened as usize, &available.to_le_bytes())
            .map_err(|_| 21)
    })();
    result.err().unwrap_or(0)
}

pub(crate) fn link(linker: &mut HostLinker) {
    linker
        .func_wrap("wasi_snapshot_preview1", "path_readlink", readlink)
        .expect("Unique import");
    linker
        .func_wrap(
            "wasi_snapshot_preview1",
            "path_create_directory",
            |caller: Caller<'_, Context>, fd: u32, ptr: u32, len: u32| -> i32 {
                let result = (|| {
                    let directory = directory(&caller, fd, 1 << 9)?;
                    let memory = memory(&caller)?;
                    let bytes =
                        &memory.data(&caller)[range(ptr, len as usize, memory.data_size(&caller))?];
                    let text = core::str::from_utf8(bytes).map_err(|_| 25)?;
                    // A trailing slash requires a directory, which mkdir creates.
                    let path = directory.resolve(text.trim_end_matches('/'))?;
                    directory
                        .filesystem
                        .create_directory(&path)
                        .map_err(FileError::errno)?;
                    if caller.data().creation_mode.is_some() || caller.data().umask != 0o022 {
                        directory
                            .filesystem
                            .set_mode(
                                &path,
                                caller.data().creation_mode.unwrap_or(0o777) & !caller.data().umask,
                            )
                            .map_err(FileError::errno)?;
                    }
                    Ok(())
                })();
                result.err().unwrap_or(0)
            },
        )
        .expect("Unique import");
    for (name, right) in [
        ("path_unlink_file", 1 << 26),
        ("path_remove_directory", 1 << 25),
    ] {
        linker
            .func_wrap(
                "wasi_snapshot_preview1",
                name,
                move |caller: Caller<'_, Context>, fd: u32, ptr: u32, len: u32| -> i32 {
                    let result = (|| {
                        let directory = directory(&caller, fd, right)?;
                        let path = path(&caller, &directory, ptr, len)?;
                        if right == 1 << 26 {
                            directory.filesystem.unlink(&path)
                        } else {
                            directory.filesystem.remove_directory(&path)
                        }
                        .map_err(FileError::errno)
                    })();
                    result.err().unwrap_or(0)
                },
            )
            .expect("Unique import");
    }
    linker
        .func_wrap(
            "wasi_snapshot_preview1",
            "path_rename",
            |caller: Caller<'_, Context>,
             old_fd: u32,
             old_ptr: u32,
             old_len: u32,
             new_fd: u32,
             new_ptr: u32,
             new_len: u32|
             -> i32 {
                let result = (|| {
                    let old = directory(&caller, old_fd, 1 << 16)?;
                    let new = directory(&caller, new_fd, 1 << 17)?;
                    let source = path(&caller, &old, old_ptr, old_len)?;
                    let destination = path(&caller, &new, new_ptr, new_len)?;
                    old.filesystem
                        .rename_to(&source, new.filesystem.as_ref(), &destination)
                        .map_err(FileError::errno)
                })();
                result.err().unwrap_or(0)
            },
        )
        .expect("Unique import");
    linker
        .func_wrap(
            "wasi_snapshot_preview1",
            "path_filestat_set_times",
            |caller: Caller<'_, Context>,
             fd: u32,
             lookup: u32,
             ptr: u32,
             len: u32,
             access: u64,
             modify: u64,
             flags: u32|
             -> i32 {
                let result = (|| {
                    let directory = directory(&caller, fd, 1 << 20)?;
                    if lookup & !1 != 0 || flags & !15 != 0 || flags & 3 == 3 || flags & 12 == 12 {
                        return Err(28);
                    }
                    let path = path(&caller, &directory, ptr, len)?;
                    directory
                        .filesystem
                        .set_times(
                            &path,
                            lookup & 1 != 0,
                            time_update(access, flags & 3),
                            time_update(modify, (flags >> 2) & 3),
                        )
                        .map_err(FileError::errno)
                })();
                result.err().unwrap_or(0)
            },
        )
        .expect("Unique import");
    let wasi = "wasi_snapshot_preview1";
    linker
        .func_wrap(
            wasi,
            "fd_advise",
            |caller: Caller<'_, Context>, fd: u32, _offset: u64, _length: u64, advice: u32| {
                let result: Result<(), i32> = (|| {
                    let descriptor = caller.data().descriptors.get(&fd).ok_or(io::BADF)?;
                    if advice > 5 {
                        return Err(28);
                    }
                    match descriptor {
                        Descriptor::File { rights, .. } if rights & (1 << 7) != 0 => Err(58),
                        Descriptor::File { .. } => Err(76),
                        _ => Err(28),
                    }
                })();
                result.err().unwrap_or(0)
            },
        )
        .expect("Unique import");
    linker
        .func_wrap(
            wasi,
            "fd_filestat_set_times",
            |caller: Caller<'_, Context>, fd: u32, access: u64, modify: u64, flags: u32| {
                let result = (|| {
                    let descriptor = caller.data().descriptors.get(&fd).ok_or(io::BADF)?;
                    if flags & !15 != 0 || flags & 3 == 3 || flags & 12 == 12 {
                        return Err(28);
                    }
                    let rights = match descriptor {
                        Descriptor::File { rights, .. } => *rights,
                        Descriptor::Directory(directory) => directory.rights,
                        _ => return Err(28),
                    };
                    if rights & (1 << 23) == 0 {
                        return Err(76);
                    }
                    let access = time_update(access, flags & 3);
                    let modify = time_update(modify, (flags >> 2) & 3);
                    match descriptor {
                        Descriptor::File { handle, .. } => {
                            handle.borrow_mut().set_times(access, modify)
                        }
                        Descriptor::Directory(directory) => {
                            directory
                                .filesystem
                                .set_times(&directory.path, true, access, modify)
                        }
                        _ => unreachable!(),
                    }
                    .map_err(FileError::errno)
                })();
                result.err().unwrap_or(0)
            },
        )
        .expect("Unique import");
    for (name, right) in [
        ("fd_pread", (1 << 1) | (1 << 2)),
        ("fd_pwrite", (1 << 6) | (1 << 2)),
    ] {
        linker
            .func_wrap(
                wasi,
                name,
                move |caller: Caller<'_, Context>,
                      fd: u32,
                      ptr: u32,
                      count: u32,
                      _offset: u64,
                      output: u32| {
                    let result = (|| {
                        let descriptor = caller.data().descriptors.get(&fd).ok_or(io::BADF)?;
                        match descriptor {
                            Descriptor::File { rights, .. } if rights & right == right => (),
                            Descriptor::File { .. } => return Err(76),
                            _ => return Err(70),
                        }
                        range(output, 4, memory(&caller)?.data_size(&caller))?;
                        vectors(&caller, ptr, count)?;
                        Err::<(), i32>(58)
                    })();
                    result.err().unwrap_or(0)
                },
            )
            .expect("Unique import");
    }
    linker
        .func_wrap(
            wasi,
            "path_symlink",
            |caller: Caller<'_, Context>,
             old_ptr: u32,
             old_len: u32,
             fd: u32,
             new_ptr: u32,
             new_len: u32| {
                let result = (|| {
                    let directory = directory(&caller, fd, 1 << 24)?;
                    let memory = memory(&caller)?;
                    let bytes = &memory.data(&caller)
                        [range(old_ptr, old_len as usize, memory.data_size(&caller))?];
                    let target = core::str::from_utf8(bytes).map_err(|_| 25)?;
                    let path = path(&caller, &directory, new_ptr, new_len)?;
                    directory
                        .filesystem
                        .symlink(target, &path)
                        .map_err(FileError::errno)
                })();
                result.err().unwrap_or(0)
            },
        )
        .expect("Unique import");
    linker
        .func_wrap(
            wasi,
            "path_link",
            |caller: Caller<'_, Context>,
             old_fd: u32,
             flags: u32,
             old_ptr: u32,
             old_len: u32,
             new_fd: u32,
             new_ptr: u32,
             new_len: u32| {
                let result = (|| {
                    let old = directory(&caller, old_fd, 1 << 11)?;
                    let new = directory(&caller, new_fd, 1 << 12)?;
                    if flags & !1 != 0 {
                        return Err(28);
                    }
                    let source = path(&caller, &old, old_ptr, old_len)?;
                    let destination = path(&caller, &new, new_ptr, new_len)?;
                    old.filesystem
                        .hard_link_to(&source, new.filesystem.as_ref(), &destination, flags == 1)
                        .map_err(FileError::errno)
                })();
                result.err().unwrap_or(0)
            },
        )
        .expect("Unique import");
    linker
        .func_wrap(
            wasi,
            "fd_filestat_set_size",
            |caller: Caller<'_, Context>, fd: u32, size: u64| match caller
                .data()
                .descriptors
                .get(&fd)
            {
                Some(Descriptor::File {
                    handle,
                    rights,
                    write,
                    ..
                }) if rights & (1 << 22) != 0 && *write => handle
                    .borrow_mut()
                    .resize(size)
                    .err()
                    .map(FileError::errno)
                    .unwrap_or(0),
                Some(Descriptor::File { .. }) => 76,
                Some(Descriptor::Directory(_)) => 31,
                Some(_) => 28,
                None => io::BADF,
            },
        )
        .expect("Unique import");
    linker
        .func_wrap(
            wasi,
            "fd_readdir",
            |mut caller: Caller<'_, Context>,
             fd: u32,
             ptr: u32,
             len: u32,
             cookie: u64,
             used: u32| {
                let result = (|| {
                    let directory = directory(&caller, fd, 1 << 14)?;
                    let memory = memory(&caller)?;
                    range(ptr, len as usize, memory.data_size(&caller))?;
                    range(used, 4, memory.data_size(&caller))?;
                    let bytes = directory.read(cookie, len as usize)?;
                    memory
                        .write(&mut caller, ptr as usize, &bytes)
                        .map_err(|_| 21)?;
                    memory
                        .write(
                            &mut caller,
                            used as usize,
                            &(bytes.len() as u32).to_le_bytes(),
                        )
                        .map_err(|_| 21)
                })();
                result.err().unwrap_or(0)
            },
        )
        .expect("Unique import");
    linker
        .func_wrap(wasi, "path_open", open)
        .expect("Unique import");
    linker
        .func_wrap(
            wasi,
            "path_filestat_get",
            |mut caller: Caller<'_, Context>,
             fd: u32,
             flags: u32,
             ptr: u32,
             len: u32,
             output: u32| {
                let result = (|| {
                    let directory = directory(&caller, fd, 1 << 18)?;
                    if flags & !1 != 0 {
                        return Err(28);
                    }
                    let memory = memory(&caller)?;
                    range(output, 64, memory.data_size(&caller))?;
                    let path = path(&caller, &directory, ptr, len)?;
                    let stat = if flags & 1 != 0 {
                        directory.filesystem.metadata(&path)
                    } else {
                        directory.filesystem.symlink_metadata(&path)
                    }
                    .map_err(FileError::errno)?
                    .wasi();
                    memory
                        .write(&mut caller, output as usize, &stat)
                        .map_err(|_| 21)
                })();
                result.err().unwrap_or(0)
            },
        )
        .expect("Unique import");
    linker
        .func_wrap(
            wasi,
            "fd_prestat_get",
            |mut caller: Caller<'_, Context>, fd: u32, ptr: u32| {
                let result = (|| {
                    let directory = directory(&caller, fd, 0).map_err(|_| io::BADF)?;
                    if !directory.preopen {
                        return Err(io::BADF);
                    }
                    memory(&caller)?
                        .write(&mut caller, ptr as usize, &[0, 0, 0, 0, 1, 0, 0, 0])
                        .map_err(|_| 21)
                })();
                result.err().unwrap_or(0)
            },
        )
        .expect("Unique import");
    linker
        .func_wrap(
            wasi,
            "fd_prestat_dir_name",
            |mut caller: Caller<'_, Context>, fd: u32, ptr: u32, len: u32| {
                let result = (|| {
                    let directory = directory(&caller, fd, 0).map_err(|_| io::BADF)?;
                    if !directory.preopen {
                        return Err(io::BADF);
                    }
                    if len < 1 {
                        return Err(37);
                    }
                    memory(&caller)?
                        .write(&mut caller, ptr as usize, b"/")
                        .map_err(|_| 21)
                })();
                result.err().unwrap_or(0)
            },
        )
        .expect("Unique import");
    linker
        .func_wrap(
            wasi,
            "fd_tell",
            |mut caller: Caller<'_, Context>, fd: u32, ptr: u32| {
                let result = (|| {
                    let descriptor = caller.data().descriptors.get(&fd).ok_or(io::BADF)?;
                    let memory = memory(&caller)?;
                    range(ptr, 8, memory.data_size(&caller))?;
                    let position = descriptor.seek(0, 1)?;
                    memory
                        .write(&mut caller, ptr as usize, &position.to_le_bytes())
                        .map_err(|_| 21)
                })();
                result.err().unwrap_or(0)
            },
        )
        .expect("Unique import");
    for (name, right) in [("fd_sync", 1 << 4), ("fd_datasync", 1 << 0)] {
        linker
            .func_wrap(
                wasi,
                name,
                move |caller: Caller<'_, Context>, fd: u32| match caller.data().descriptors.get(&fd)
                {
                    Some(Descriptor::File { handle, rights, .. }) if rights & right != 0 => handle
                        .borrow_mut()
                        .sync(name == "fd_datasync")
                        .err()
                        .map(FileError::errno)
                        .unwrap_or(0),
                    Some(Descriptor::File { .. }) => 76,
                    Some(_) => 28,
                    None => io::BADF,
                },
            )
            .expect("Unique import");
    }
}
