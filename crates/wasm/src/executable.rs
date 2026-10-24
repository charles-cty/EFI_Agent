//! Command lookup and bounded interpreter dispatch over the guest namespace.
use crate::filesystem::FileSystem;
use crate::{ASH_APPLETS, Error, World, filesystem};
use alloc::{
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};

impl World {
    fn command_module<'a>(&self, name: &'a str) -> &'a str {
        if Self::ash_applet(name)
            && !self.programs.contains_key(name)
            && !self.program_files.contains_key(name)
        {
            "ash"
        } else {
            name
        }
    }

    pub(crate) fn refresh_bin(&mut self) -> Result<(), filesystem::FileError> {
        let mut entries = alloc::collections::BTreeMap::new();
        for name in self.programs.keys().chain(self.program_files.keys()) {
            if !name.is_empty() && !name.contains('/') {
                entries.insert(name.clone(), self.program_files.get(name).cloned());
            }
        }
        if self.has_program("ash") {
            let source = self.program_files.get("ash").cloned();
            for name in ASH_APPLETS {
                entries
                    .entry((*name).into())
                    .or_insert_with(|| source.clone());
            }
        }
        if let Some(filesystem) = &self.filesystem
            && let Some(filesystem) = (filesystem.as_ref() as &dyn core::any::Any)
                .downcast_ref::<filesystem::VirtualFileSystem>()
        {
            filesystem.validate_programs(&entries)?;
        }
        *self.bin.borrow_mut() = entries;
        Ok(())
    }

    pub(crate) fn resolve_command(
        &mut self,
        mut args: Vec<String>,
        environment: &[String],
        cwd: &str,
        caller: crate::Pid,
    ) -> Result<(String, Vec<String>), Error> {
        // Host launchers select installed modules directly. Guest launches use PATH.
        if caller == 0
            && (!args[0].contains('/') || args[0].starts_with("/bin/"))
            && self.has_program(&args[0])
        {
            let name = args[0].strip_prefix("/bin/").unwrap_or(&args[0]);
            return Ok((self.command_module(name).into(), args));
        }
        let filesystem = self.filesystem.as_ref().map(|filesystem| {
            (filesystem.as_ref() as &dyn core::any::Any)
                .downcast_ref::<filesystem::VirtualFileSystem>()
                .expect("World installs the virtual root filesystem")
                .for_process(caller)
        });
        for _ in 0..5 {
            let path = self.command_path(&args[0], environment, cwd, filesystem.as_deref())?;
            if let Some(name) = path.strip_prefix("bin/")
                && self.bin.borrow().contains_key(name)
            {
                // Preserve the installed WASI command's startup name. The
                // prebuilt interpreter uses it to find its packaged library.
                if !Self::ash_applet(name) {
                    args[0] = name.into();
                }
                return Ok((self.command_module(name).into(), args));
            }
            let filesystem = filesystem
                .as_ref()
                .ok_or_else(|| Error::UnknownProgram(path.clone()))?;
            let mut file = filesystem
                .open(
                    &path,
                    filesystem::OpenOptions {
                        read: true,
                        write: false,
                        create: false,
                        exclusive: false,
                        truncate: false,
                        append: false,
                    },
                )
                .map_err(Error::ProgramFile)?;
            let mut header = vec![0; 4096];
            let mut used = 0;
            while used < header.len() {
                let count = file.read(&mut header[used..]).map_err(Error::ProgramFile)?;
                if count == 0 {
                    break;
                }
                used += count;
            }
            header.truncate(used);
            if header.starts_with(b"\0asm\x01\0\0\0") {
                let key = format!("/{path}");
                self.program_files.entry(key.clone()).or_insert(path);
                return Ok((key, args));
            }
            if !header.starts_with(b"#!") {
                return Err(Error::InvalidExecutable);
            }
            let end = header
                .iter()
                .position(|byte| *byte == b'\n')
                .ok_or(Error::InvalidExecutable)?;
            let line =
                core::str::from_utf8(&header[2..end]).map_err(|_| Error::InvalidExecutable)?;
            let line = line.trim_matches([' ', '\t', '\r']);
            let split = line.find([' ', '\t']).unwrap_or(line.len());
            let interpreter = &line[..split];
            if !interpreter.starts_with('/') {
                return Err(Error::InvalidExecutable);
            }
            let option = line[split..].trim_matches([' ', '\t']);
            let mut next = vec![interpreter.to_string()];
            if !option.is_empty() {
                next.push(option.to_string());
            }
            // Relative scripts must use the child's inherited cwd. An address
            // through the parent's proc cwd could change after spawn returns.
            let prefix = format!("{}/", cwd.trim_matches('/'));
            next.push(match path.strip_prefix(&prefix) {
                Some(relative) => relative.to_string(),
                None => format!("/{path}"),
            });
            next.extend(args.into_iter().skip(1));
            args = next;
        }
        Err(Error::ProgramFile(filesystem::FileError::Loop))
    }

    fn command_path(
        &self,
        name: &str,
        environment: &[String],
        cwd: &str,
        filesystem: Option<&filesystem::VirtualFileSystem>,
    ) -> Result<String, Error> {
        let explicit = name.contains('/');
        let path = environment
            .iter()
            .find_map(|value| value.strip_prefix("PATH="))
            .unwrap_or("/bin");
        let candidates = if explicit {
            vec![name.to_string()]
        } else {
            path.split(':')
                .map(|directory| {
                    if directory.is_empty() {
                        name.to_string()
                    } else {
                        format!("{directory}/{name}")
                    }
                })
                .collect()
        };
        let mut denied = false;
        for candidate in candidates {
            let candidate = if candidate.starts_with('/') {
                candidate.trim_start_matches('/').to_string()
            } else {
                format!("{}/{}", cwd.trim_matches('/'), candidate)
                    .trim_start_matches('/')
                    .to_string()
            };
            if let Some(command) = candidate.strip_prefix("bin/")
                && self.bin.borrow().contains_key(command)
            {
                return Ok(candidate);
            }
            let Some(virtual_root) = filesystem else {
                continue;
            };
            let (resolved, info) = match virtual_root.executable_path(&candidate) {
                Ok(resolved) => resolved,
                Err(filesystem::FileError::NotFound | filesystem::FileError::NotDirectory) => {
                    continue;
                }
                Err(filesystem::FileError::Access) if !explicit => {
                    denied = true;
                    continue;
                }
                Err(error) => return Err(Error::ProgramFile(error)),
            };
            if info.kind != filesystem::EntryType::File || info.mode & 0o100 == 0 {
                denied = true;
                continue;
            }
            if let Some(command) = resolved.strip_prefix("bin/")
                && self.bin.borrow().contains_key(command)
            {
                return Ok(resolved);
            }
            return Ok(resolved);
        }
        if denied {
            Err(Error::ProgramFile(filesystem::FileError::Access))
        } else {
            Err(Error::UnknownProgram(name.into()))
        }
    }
}
