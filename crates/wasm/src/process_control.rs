//! Internal process operations for unmodified WASI interpreters over an open descriptor.
use crate::{Execution, Pid, SpawnSettings, Status, World, filesystem::FileError, io::Descriptor};
use alloc::{
    collections::BTreeSet,
    format,
    rc::Rc,
    string::{String, ToString},
    vec::Vec,
};
use core::cell::RefCell;

fn word(bytes: &[u8], index: usize) -> Result<u32, i32> {
    let start = index.checked_mul(4).ok_or(-22)?;
    let value = bytes.get(start..start + 4).ok_or(-22)?;
    Ok(u32::from_le_bytes(value.try_into().expect("Four bytes")))
}

fn strings(bytes: &[u8]) -> Result<Vec<String>, i32> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    if bytes.last() != Some(&0) {
        return Err(-22);
    }
    bytes[..bytes.len() - 1]
        .split(|byte| *byte == 0)
        .map(|value| {
            core::str::from_utf8(value)
                .map(String::from)
                .map_err(|_| -22)
        })
        .collect()
}

pub(super) fn respond(
    execution: &mut Execution,
    fd: u32,
    written: u32,
    count: u32,
    result: i32,
    value: i32,
) -> Result<(), wasmi::errors::MemoryError> {
    let Descriptor::Control(reply) = &execution.store.data().descriptors[&fd] else {
        unreachable!("Control endpoint remains open during its syscall")
    };
    let mut reply = reply.borrow_mut();
    reply.extend(result.to_le_bytes());
    reply.extend(value.to_le_bytes());
    drop(reply);
    execution
        .memory
        .write(&mut execution.store, written as usize, &count.to_le_bytes())
}

impl World {
    pub(super) fn control(&mut self, pid: Pid, fd: u32, bytes: &[u8], written: u32) {
        // One request per write; the caller must consume the previous response.
        let empty = match &self.processes[&pid].execution().store.data().descriptors[&fd] {
            Descriptor::Control(reply) => reply.borrow().is_empty(),
            _ => unreachable!("Control action has a control descriptor"),
        };
        if !empty {
            let execution = self
                .processes
                .get_mut(&pid)
                .unwrap()
                .execution
                .as_mut()
                .unwrap();
            execution.reply = 6; // AGAIN: drain the pending response before another request.
            self.ready.push_back(pid);
            return;
        }
        let mut waiting = None;
        let result = (|| {
            if bytes.len() > 70_672 {
                return Err(-22);
            }
            let operation = word(bytes, 0)?;
            let expected = match operation {
                1 | 6 | 7 | 16 => 4,
                2 | 3 | 5 | 10 => 12,
                4 | 13 | 15 | 18 | 19 => 8,
                14 => 12,
                20 | 21 => bytes.len(),
                8 | 9 | 11 | 12 | 17 | 22 | 23 => bytes.len(),
                _ => return Err(-22),
            };
            if bytes.len() != expected {
                return Err(-22);
            }
            match operation {
                1 => {
                    let context = self
                        .processes
                        .get_mut(&pid)
                        .unwrap()
                        .execution
                        .as_mut()
                        .unwrap()
                        .store
                        .data_mut();
                    let fds = crate::io::pipe(&mut context.descriptors).map_err(|_| -24)?;
                    context.close_on_exec.extend(fds);
                    Ok((fds[0] as i32, fds[1] as i32))
                }
                2 | 3 => {
                    let source = word(bytes, 1)?;
                    let target = word(bytes, 2)?;
                    if target >= crate::MAX_DESCRIPTORS || target == 3 {
                        return Err(-9);
                    }
                    let context = self
                        .processes
                        .get_mut(&pid)
                        .unwrap()
                        .execution
                        .as_mut()
                        .unwrap()
                        .store
                        .data_mut();
                    let descriptor = context.descriptors.get(&source).ok_or(-9)?.clone();
                    if matches!(descriptor, Descriptor::Control(_)) {
                        return Err(-22);
                    }
                    if operation == 3
                        && matches!(
                            context.descriptors.get(&target),
                            Some(Descriptor::Control(_))
                        )
                    {
                        return Err(-22);
                    }
                    let target = if operation == 2 {
                        (target..crate::MAX_DESCRIPTORS)
                            .find(|fd| *fd != 3 && !context.descriptors.contains_key(fd))
                            .ok_or(-24)?
                    } else {
                        target
                    };
                    if source != target || operation == 2 {
                        context.descriptors.insert(target, descriptor);
                        context.close_on_exec.remove(&target);
                    }
                    Ok((target as i32, 0))
                }
                4 | 5 => {
                    let source = word(bytes, 1)?;
                    let flags = if operation == 5 { word(bytes, 2)? } else { 0 };
                    if flags > 1 {
                        return Err(-22);
                    }
                    let context = self
                        .processes
                        .get_mut(&pid)
                        .unwrap()
                        .execution
                        .as_mut()
                        .unwrap()
                        .store
                        .data_mut();
                    let descriptor = context.descriptors.get(&source).ok_or(-9)?;
                    if operation == 4 {
                        return Ok((i32::from(context.close_on_exec.contains(&source)), 0));
                    }
                    if matches!(descriptor, Descriptor::Control(_)) && flags == 0 {
                        return Err(-22);
                    }
                    if flags == 1 {
                        context.close_on_exec.insert(source);
                    } else {
                        context.close_on_exec.remove(&source);
                    }
                    Ok((0, 0))
                }
                6 => Ok((pid as i32, 0)),
                7 => Ok((self.processes[&pid].parent as i32, 0)),
                8 => {
                    let name = core::str::from_utf8(&bytes[4..]).map_err(|_| -22)?;
                    if name.is_empty() || name.contains('\0') {
                        return Err(-22);
                    }
                    Ok((i32::from(self.has_program(name)), 0))
                }
                9 => {
                    let args_size = word(bytes, 1)? as usize;
                    let env_size = word(bytes, 2)? as usize;
                    let maps = word(bytes, 3)? as usize;
                    if args_size > 65536 || env_size > 65536 - args_size {
                        return Err(-7);
                    }
                    if maps > 127 || 16 + maps * 8 + args_size + env_size != bytes.len() {
                        return Err(-22);
                    }
                    let mut mapping = Vec::new();
                    let mut targets = BTreeSet::new();
                    for index in 0..maps {
                        let target = word(bytes, 4 + index * 2)?;
                        let source = word(bytes, 5 + index * 2)?;
                        if target == 3
                            || target >= crate::MAX_DESCRIPTORS
                            || !targets.insert(target)
                        {
                            return Err(-9);
                        }
                        if matches!(
                            self.processes[&pid]
                                .execution()
                                .store
                                .data()
                                .descriptors
                                .get(&source),
                            Some(Descriptor::Control(_))
                        ) {
                            return Err(-9);
                        }
                        mapping.push((target, source));
                    }
                    let start = 16 + maps * 8;
                    let args = strings(&bytes[start..start + args_size])?;
                    let env = strings(&bytes[start + args_size..])?;
                    let child = self
                        .spawn_child(
                            pid,
                            args,
                            Some(env),
                            Vec::new(),
                            Some(mapping),
                            SpawnSettings::default(),
                        )
                        .map_err(|error| error.spawn_errno())?;
                    Ok((child as i32, 0))
                }
                22 | 23 => {
                    let has_umask = operation == 23;
                    let args_size = word(bytes, 1)? as usize;
                    let env_size = word(bytes, 2)? as usize;
                    let maps = word(bytes, 3)? as usize;
                    let cwd_size = word(bytes, 4)? as usize;
                    let umask = if has_umask {
                        let value = word(bytes, 5)?;
                        if value == u32::MAX {
                            None
                        } else if value & !0o777 == 0 {
                            Some(value as u16)
                        } else {
                            return Err(-22);
                        }
                    } else {
                        None
                    };
                    if args_size > 65536 || env_size > 65536 - args_size || cwd_size == 0 {
                        return Err(-7);
                    }
                    if cwd_size > 4096 {
                        return Err(-36);
                    }
                    let header_size = if has_umask { 24 } else { 20 };
                    if maps > 127
                        || header_size + maps * 8 + args_size + env_size + cwd_size != bytes.len()
                    {
                        return Err(-22);
                    }
                    let mut mapping = Vec::new();
                    let mut targets = BTreeSet::new();
                    for index in 0..maps {
                        let map_start = if has_umask { 6 } else { 5 };
                        let target = word(bytes, map_start + index * 2)?;
                        let source = word(bytes, map_start + index * 2 + 1)?;
                        if target == 3
                            || target >= crate::MAX_DESCRIPTORS
                            || !targets.insert(target)
                        {
                            return Err(-9);
                        }
                        if matches!(
                            self.processes[&pid]
                                .execution()
                                .store
                                .data()
                                .descriptors
                                .get(&source),
                            Some(Descriptor::Control(_))
                        ) {
                            return Err(-9);
                        }
                        mapping.push((target, source));
                    }
                    let start = header_size + maps * 8;
                    let args = strings(&bytes[start..start + args_size])?;
                    let env = strings(&bytes[start + args_size..start + args_size + env_size])?;
                    let cwd_bytes = &bytes[start + args_size + env_size..];
                    let cwd = core::str::from_utf8(cwd_bytes)
                        .map_err(|_| -22)?
                        .to_string();
                    if !cwd.starts_with('/') || cwd.contains('\0') {
                        return Err(-22);
                    }
                    let filesystem = self.filesystem.as_ref().ok_or(-2)?;
                    let resolved = filesystem
                        .resolve_path(
                            "",
                            if cwd == "/" {
                                "."
                            } else {
                                cwd.trim_start_matches('/')
                            },
                        )
                        .map_err(|error| -FileError::errno(error))?;
                    let metadata = filesystem
                        .metadata(&resolved)
                        .map_err(|error| -FileError::errno(error))?;
                    if !metadata.is_directory() {
                        return Err(-54);
                    }
                    if metadata.mode & 0o100 == 0 {
                        return Err(-2);
                    }
                    let child = self
                        .spawn_child(
                            pid,
                            args,
                            Some(env),
                            Vec::new(),
                            Some(mapping),
                            SpawnSettings {
                                cwd: Some(cwd),
                                umask,
                            },
                        )
                        .map_err(|error| error.spawn_errno())?;
                    Ok((child as i32, 0))
                }
                10 => {
                    let child = word(bytes, 1)?;
                    let nohang = word(bytes, 2)?;
                    if nohang > 1 {
                        return Err(-22);
                    }
                    let process = self
                        .processes
                        .get(&child)
                        .filter(|process| process.parent == pid)
                        .ok_or(-10)?;
                    if process.status.is_live() {
                        if nohang == 0 {
                            waiting = Some(child);
                        }
                        Ok((0, 0))
                    } else {
                        let status = match process.status {
                            Status::Exited(code) => code,
                            _ => 128,
                        };
                        self.processes.remove(&child);
                        Ok((child as i32, status))
                    }
                }
                11..=14 => {
                    let fd = word(bytes, 1)?;
                    let flags = if operation <= 12 { word(bytes, 2)? } else { 1 };
                    let mode = match operation {
                        12 => word(bytes, 3)?,
                        14 => word(bytes, 2)?,
                        _ => 0,
                    };
                    let start = if operation == 12 { 16 } else { 12 };
                    let path = if operation <= 12 {
                        let path = core::str::from_utf8(bytes.get(start..).ok_or(-22)?)
                            .map_err(|_| -22)?;
                        if path.is_empty() || path.contains('\0') {
                            return Err(-22);
                        }
                        path
                    } else {
                        ""
                    };
                    let context = self.processes[&pid].execution().store.data();
                    // File operations retain WASI errors in the first response
                    // word. Process operations above retain negative POSIX errors.
                    Ok(
                        match crate::filesystem_abi::control_mode(
                            context, operation, fd, flags, mode, path,
                        ) {
                            Ok(mode) => (0, i32::from(mode)),
                            Err(error) => (error, 0),
                        },
                    )
                }
                15 => {
                    let mask = word(bytes, 1)? as u16 & 0o777;
                    let context = self
                        .processes
                        .get_mut(&pid)
                        .unwrap()
                        .execution
                        .as_mut()
                        .unwrap()
                        .store
                        .data_mut();
                    let previous = core::mem::replace(&mut context.umask, mask);
                    Ok((i32::from(previous), 0))
                }
                19 => {
                    let mode = word(bytes, 1)?;
                    if mode != u32::MAX && mode & !0o7777 != 0 {
                        return Err(-22);
                    }
                    let context = self
                        .processes
                        .get_mut(&pid)
                        .unwrap()
                        .execution
                        .as_mut()
                        .unwrap()
                        .store
                        .data_mut();
                    let previous = core::mem::replace(
                        &mut context.creation_mode,
                        (mode != u32::MAX).then_some(mode as u16),
                    );
                    Ok((0, previous.map_or(-1, i32::from)))
                }
                20 | 21 => {
                    let fd = word(bytes, 1)?;
                    let path = match core::str::from_utf8(&bytes[8..]) {
                        Ok(path) => path,
                        Err(_) => return Ok((22, 0)),
                    };
                    if path.is_empty() || path.contains('\0') {
                        return Ok((22, 0));
                    }
                    let directory = match self.processes[&pid]
                        .execution()
                        .store
                        .data()
                        .descriptors
                        .get(&fd)
                    {
                        Some(crate::io::Descriptor::Directory(directory)) => directory.clone(),
                        Some(_) => return Ok((54, 0)),
                        None => return Ok((9, 0)),
                    };
                    let right = if operation == 20 { 1 << 26 } else { 1 << 25 };
                    if directory.rights & right != right {
                        return Ok((76, 0));
                    }
                    let path = match directory.resolve(path) {
                        Ok(path) => path,
                        Err(error) => return Ok((error, 0)),
                    };
                    let result = if operation == 20 {
                        directory.filesystem.unlink(&path)
                    } else {
                        directory.filesystem.remove_directory(&path)
                    };
                    Ok((result.map_or_else(FileError::errno, |_| 0), 0))
                }
                16 => {
                    let context = self
                        .processes
                        .get_mut(&pid)
                        .unwrap()
                        .execution
                        .as_mut()
                        .unwrap()
                        .store
                        .data_mut();
                    let fd = (4..crate::MAX_DESCRIPTORS)
                        .find(|fd| !context.descriptors.contains_key(fd))
                        .ok_or(-24)?;
                    context.descriptors.insert(
                        fd,
                        Descriptor::Input(Rc::new(RefCell::new(
                            context.cwd.as_bytes().iter().copied().collect(),
                        ))),
                    );
                    context.close_on_exec.insert(fd);
                    Ok((fd as i32, 0))
                }
                17 => {
                    let path = core::str::from_utf8(&bytes[4..]).map_err(|_| -22)?;
                    if !path.starts_with('/') || path.contains('\0') {
                        return Err(-22);
                    }
                    let filesystem = self.filesystem.as_ref().ok_or(-2)?;
                    let result = (|| {
                        let path = if path.trim_matches('/').is_empty() {
                            String::new()
                        } else {
                            filesystem
                                .resolve_path("", path.trim_start_matches('/'))
                                .map_err(crate::filesystem::FileError::errno)?
                        };
                        let info = filesystem
                            .metadata(&path)
                            .map_err(crate::filesystem::FileError::errno)?;
                        if !info.is_directory() {
                            return Err(54);
                        }
                        if info.mode & 0o100 == 0 {
                            return Err(2);
                        }
                        Ok(format!("/{path}"))
                    })();
                    match result {
                        Ok(path) => {
                            self.processes
                                .get_mut(&pid)
                                .unwrap()
                                .execution
                                .as_mut()
                                .unwrap()
                                .store
                                .data_mut()
                                .cwd = path;
                            Ok((0, 0))
                        }
                        Err(error) => Ok((error, 0)),
                    }
                }
                18 => {
                    let source = word(bytes, 1)?;
                    let context = self
                        .processes
                        .get_mut(&pid)
                        .unwrap()
                        .execution
                        .as_mut()
                        .unwrap()
                        .store
                        .data_mut();
                    let result = (|| {
                        let directory = match context.descriptors.get(&source) {
                            Some(Descriptor::Directory(directory)) => directory,
                            Some(_) => return Err(54),
                            None => return Err(8),
                        };
                        if directory.rights & (1 << 14) == 0 {
                            return Err(76);
                        }
                        let entries = directory
                            .filesystem
                            .read_directory(&directory.path)
                            .map_err(crate::filesystem::FileError::errno)?;
                        let mut bytes = Vec::new();
                        // DirectoryEntries bounds the count and encoded name bytes.
                        for (name, _) in entries.iter() {
                            bytes.extend((name.len() as u32).to_le_bytes());
                            bytes.extend(name.as_bytes());
                        }
                        let fd = (4..crate::MAX_DESCRIPTORS)
                            .find(|fd| !context.descriptors.contains_key(fd))
                            .ok_or(33)?;
                        context
                            .descriptors
                            .insert(fd, Descriptor::Input(Rc::new(RefCell::new(bytes.into()))));
                        context.close_on_exec.insert(fd);
                        Ok(fd)
                    })();
                    Ok(match result {
                        Ok(fd) => (0, fd as i32),
                        Err(error) => (error, 0),
                    })
                }
                _ => unreachable!("Operation checked"),
            }
        })();
        let process = self.processes.get_mut(&pid).expect("Control caller exists");
        let execution = process.execution.as_mut().expect("Control caller is live");
        if let Some(child) = waiting {
            execution.control_wait = Some((fd, written, bytes.len() as u32));
            execution.wait_status = u32::MAX;
            process.status = Status::Waiting(child);
            return;
        }
        let (result, value) = result.unwrap_or_else(|error| (error, 0));
        execution.reply = match respond(execution, fd, written, bytes.len() as u32, result, value) {
            Ok(()) => 0,
            Err(_) => 21,
        };
        self.ready.push_back(pid);
    }
}
