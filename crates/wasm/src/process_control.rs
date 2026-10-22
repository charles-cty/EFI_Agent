//! Internal process operations for unmodified WASI interpreters over an open descriptor.
use crate::{Execution, Pid, Status, World, io::Descriptor};
use alloc::{collections::BTreeSet, string::String, vec::Vec};

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
            if bytes.len() > 66_580 {
                return Err(-22);
            }
            let operation = word(bytes, 0)?;
            let expected = match operation {
                1 | 6 | 7 => 4,
                2 | 3 | 5 | 10 => 12,
                4 => 8,
                8 | 9 => bytes.len(),
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
                        .spawn_child(pid, args, Some(env), Vec::new(), Some(mapping))
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
