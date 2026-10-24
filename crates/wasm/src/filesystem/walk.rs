//! Resolve links across runtime mounts before dispatching provider operations.
use super::{EntryType, FileError, Scope, VirtualFileSystem};
use alloc::{collections::VecDeque, format, string::String, vec, vec::Vec};

pub(super) const MAX_PATH: usize = 4096;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Follow {
    Final,
    Inspect,
    Parent,
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
    ) -> Result<String, FileError> {
        if path.starts_with('/') {
            return Err(FileError::NotCapable);
        }
        let mut parts = components(path)?;
        let mut stack: Vec<String> = vec![String::new()];
        let mut links = 0;
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
                });
            }
            let metadata = match self.symlink_metadata_raw(&candidate) {
                Ok(metadata) => metadata,
                Err(FileError::NotFound) if missing && parts.is_empty() && !directory_required => {
                    return Ok(candidate);
                }
                Err(error) => return Err(error),
            };
            let follow_link = follow == Follow::Final || !parts.is_empty() || directory_required;
            if metadata.kind == EntryType::Symlink && follow_link {
                links += 1;
                if links > 40 {
                    return Err(FileError::Loop);
                }
                // Installed command aliases expose executable, read-only
                // metadata even when their shared source file is not executable.
                if self.program(&self.mounted(&candidate)).is_some() {
                    if !parts.is_empty() || directory_required {
                        return Err(FileError::NotDirectory);
                    }
                    return Ok(candidate);
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
                return Ok(candidate);
            }
            if metadata.kind != EntryType::Directory {
                return Err(FileError::NotDirectory);
            }
            stack.push(candidate);
        }
        Ok(stack.pop().unwrap())
    }
}
