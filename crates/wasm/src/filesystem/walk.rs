//! Resolve links across runtime mounts before dispatching provider operations.
use super::{EntryType, FileError, Scope, VirtualFileSystem};
use alloc::{collections::VecDeque, format, rc::Rc, string::String, vec, vec::Vec};

pub(super) const MAX_PATH: usize = 4096;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Follow {
    Final,
    Inspect,
    Parent,
}

pub(super) struct Resolved {
    pub path: String,
    pub filesystem: Option<Rc<VirtualFileSystem>>,
}

impl Resolved {
    pub(super) fn owner<'a>(&'a self, fallback: &'a VirtualFileSystem) -> &'a VirtualFileSystem {
        self.filesystem.as_deref().unwrap_or(fallback)
    }
}

impl From<String> for Resolved {
    fn from(path: String) -> Self {
        Self {
            path,
            filesystem: None,
        }
    }
}

impl From<&str> for Resolved {
    fn from(path: &str) -> Self {
        String::from(path).into()
    }
}

impl core::ops::Deref for Resolved {
    type Target = str;
    fn deref(&self) -> &str {
        &self.path
    }
}

fn components(path: &str) -> Result<VecDeque<String>, FileError> {
    if path.len() > MAX_PATH {
        return Err(FileError::ResourceLimit);
    }
    if path.contains(['\\', '\0']) {
        return Err(FileError::Invalid);
    }
    path.split('/')
        .filter(|part| !part.is_empty())
        .map(|part| {
            if part.len() > 1024 {
                Err(FileError::ResourceLimit)
            } else {
                Ok(part.into())
            }
        })
        .collect()
}

fn child(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.into()
    } else {
        format!("{parent}/{name}")
    }
}

impl VirtualFileSystem {
    pub(super) fn walk(
        &self,
        path: &str,
        follow: Follow,
        missing: bool,
    ) -> Result<Resolved, FileError> {
        self.walk_budget(path, follow, missing, &mut 0)
    }

    fn walk_budget(
        &self,
        path: &str,
        follow: Follow,
        missing: bool,
        links: &mut usize,
    ) -> Result<Resolved, FileError> {
        if path.starts_with('/') {
            return Err(FileError::NotCapable);
        }
        let mut parts = components(path)?;
        let mut stack: Vec<String> = vec![String::new()];
        let mut directory_required = path.ends_with('/');
        while let Some(part) = parts.pop_front() {
            let current = stack.last().unwrap();
            let directory = self.metadata_raw(current)?;
            if directory.kind != EntryType::Directory {
                return Err(FileError::NotDirectory);
            }
            if directory.mode & 0o100 == 0 {
                return Err(FileError::Access);
            }
            if part == "." {
                continue;
            }
            if part == ".." {
                if stack.len() == 1 {
                    return Err(FileError::NotCapable);
                }
                stack.pop();
                continue;
            }
            let candidate = child(current, &part);
            if candidate.len() > MAX_PATH {
                return Err(FileError::ResourceLimit);
            }
            if parts.is_empty() && follow == Follow::Parent {
                return Ok(if directory_required {
                    format!("{candidate}/")
                } else {
                    candidate
                }
                .into());
            }
            let metadata = match self.symlink_metadata_raw(&candidate) {
                Ok(metadata) => metadata,
                Err(FileError::NotFound) if missing && parts.is_empty() && !directory_required => {
                    return Ok(candidate.into());
                }
                Err(error) => return Err(error),
            };
            let follow_link = follow == Follow::Final || !parts.is_empty() || directory_required;
            if metadata.kind == EntryType::Symlink && follow_link {
                *links += 1;
                if *links > 40 {
                    return Err(FileError::Loop);
                }
                if let Some(super::process::Node::Fd(pid, fd)) =
                    self.process_node(&self.mounted(&candidate))?
                {
                    if !parts.is_empty() || directory_required {
                        let crate::io::Descriptor::Directory(directory) =
                            self.process_files.descriptor(pid, fd)?
                        else {
                            return Err(FileError::NotDirectory);
                        };
                        let target = (directory.filesystem.as_ref() as &dyn core::any::Any)
                            .downcast_ref::<Self>()
                            .ok_or(FileError::Unsupported)?;
                        let suffix = if parts.is_empty() {
                            String::from(".")
                        } else {
                            parts.into_iter().collect::<Vec<_>>().join("/")
                        };
                        let suffix = if directory_required {
                            format!("{suffix}/")
                        } else {
                            suffix
                        };
                        let path = directory
                            .filesystem
                            .resolve_path(&directory.path, &suffix)?;
                        let mut resolved = target.walk_budget(&path, follow, missing, links)?;
                        if resolved.filesystem.is_none() {
                            resolved.filesystem = Some(Rc::new(target.clone()));
                        }
                        return Ok(resolved);
                    }
                    return Ok(candidate.into());
                }
                // Installed command aliases expose executable, read-only
                // metadata even when their shared source file is not executable.
                if self.program(&self.mounted(&candidate)).is_some() {
                    if !parts.is_empty() || directory_required {
                        return Err(FileError::NotDirectory);
                    }
                    return Ok(candidate.into());
                }
                let target = self.read_link_raw(&candidate)?;
                if target.is_empty() {
                    return Err(FileError::NotFound);
                }
                if parts.is_empty() && target.ends_with('/') {
                    directory_required = true;
                }
                if target.starts_with('/') {
                    if self.scope != Scope::Root {
                        return Err(FileError::NotCapable);
                    }
                    stack.truncate(1);
                }
                let mut replacement = components(&target)?;
                replacement.append(&mut parts);
                let prefix = stack.last().unwrap().len();
                let suffix = replacement.iter().map(String::len).sum::<usize>()
                    + replacement.len().saturating_sub(1);
                let separator = usize::from(prefix != 0 && !replacement.is_empty());
                if prefix + separator + suffix > MAX_PATH {
                    return Err(FileError::ResourceLimit);
                }
                parts = replacement;
                continue;
            }
            if parts.is_empty() && !directory_required {
                return Ok(candidate.into());
            }
            if metadata.kind != EntryType::Directory {
                return Err(FileError::NotDirectory);
            }
            stack.push(candidate);
        }
        Ok(stack.pop().unwrap().into())
    }
}
