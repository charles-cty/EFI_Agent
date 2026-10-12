use alloc::{rc::Rc, string::String};
use core::cell::RefCell;
use wasmi::{Caller, Linker};

use crate::{
    Context,
    abi::{memory, range},
    filesystem::{Directory, FileError, OpenOptions},
    io::{self, Descriptor},
};

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
    caller: Caller<'_, Context>,
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
        // Providers expose regular files and directories, never followed links.
        // Preserve missing/access errors; an existing non-link is EINVAL.
        directory
            .filesystem
            .metadata(&path)
            .map_err(FileError::errno)?;
        Err(28)
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
        let available = (0..io::MAX_DESCRIPTORS)
            .find(|fd| !caller.data().descriptors.contains_key(fd))
            .ok_or(io::MFILE)?;
        let target_is_directory = match directory.filesystem.metadata(&path) {
            Ok(metadata) => metadata.directory,
            Err(FileError::NotFound) if oflags & 1 != 0 && oflags & 2 == 0 => false,
            Err(failure) => return Err(failure.errno()),
        };
        let descriptor = if oflags & 2 != 0 || target_is_directory {
            if oflags & !2 != 0 {
                return Err(28);
            }
            // Directory enumeration completes synchronously. NONBLOCK does
            // not require a readiness wait, but remains visible in fdstat.
            if flags & !4 != 0 {
                return Err(58);
            }
            if !target_is_directory {
                return Err(54);
            }
            Descriptor::Directory(Directory {
                path,
                preopen: false,
                rights: rights & io::DIRECTORY_RIGHTS,
                inheriting,
                entries: Rc::new(RefCell::new(None)),
                flags: flags as u16,
                ..directory
            })
        } else {
            if flags & !1 != 0 {
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
            let handle = directory
                .filesystem
                .open(&path, options)
                .map_err(FileError::errno)?;
            Descriptor::File {
                handle: Rc::new(RefCell::new(handle)),
                read: options.read,
                write: options.write,
                rights: rights & io::FILE_RIGHTS,
                append: options.append,
            }
        };
        caller.data_mut().descriptors.insert(available, descriptor);
        memory
            .write(&mut caller, opened as usize, &available.to_le_bytes())
            .map_err(|_| 21)
    })();
    result.err().unwrap_or(0)
}

pub(crate) fn link(linker: &mut Linker<Context>) {
    linker
        .func_wrap("wasi_snapshot_preview1", "path_readlink", readlink)
        .expect("Unique import");
    // These operations have no provider implementation or granted rights yet.
    // Link callers so unsupported optional CLI features fail as domain errors.
    for (name, right) in [
        ("path_create_directory", 1 << 9),
        ("path_unlink_file", 1 << 26),
    ] {
        linker
            .func_wrap(
                "wasi_snapshot_preview1",
                name,
                move |caller: Caller<'_, Context>, fd: u32, ptr: u32, len: u32| -> i32 {
                    let result = (|| {
                        let directory = directory(&caller, fd, right)?;
                        path(&caller, &directory, ptr, len)?;
                        Err::<(), i32>(58)
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
                    path(&caller, &old, old_ptr, old_len)?;
                    path(&caller, &new, new_ptr, new_len)?;
                    Err::<(), i32>(58)
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
             _access: u64,
             _modify: u64,
             flags: u32|
             -> i32 {
                let result = (|| {
                    let directory = directory(&caller, fd, 1 << 20)?;
                    if lookup & !1 != 0 || flags & !15 != 0 || flags & 3 == 3 || flags & 12 == 12 {
                        return Err(28);
                    }
                    path(&caller, &directory, ptr, len)?;
                    Err::<(), i32>(58)
                })();
                result.err().unwrap_or(0)
            },
        )
        .expect("Unique import");
    let wasi = "wasi_snapshot_preview1";
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
                    let stat = directory
                        .filesystem
                        .metadata(&path)
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
                        .write(&mut caller, ptr as usize, b".")
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
                        .flush()
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
