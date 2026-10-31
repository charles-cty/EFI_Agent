//! Persistent command installation data shared by runtime and packaging.
use alloc::{collections::BTreeSet, format, string::String, vec::Vec};
use core::fmt;
use serde::{Deserialize, Serialize};

use crate::FileError;

pub const MANIFEST_PATH: &str = "programs.json";
pub const MAX_MANIFEST_BYTES: usize = 64 * 1024;
pub const MAX_MODULES: usize = 128;
pub const MAX_COMMANDS: usize = 512;
pub const ASH_APPLETS: &[&str] = &[
    "dd",
    "mktemp",
    "readlink",
    "truncate",
    "split",
    "paste",
    "comm",
    "tac",
    "nl",
    "fold",
    "strings",
    "cmp",
    "diff",
    "patch",
    "expr",
    "bc",
    "dc",
    "cksum",
    "crc32",
    "md5sum",
    "sha1sum",
    "sha512sum",
    "bzip2",
    "bunzip2",
    "xz",
    "unxz",
    "lzma",
    "ash",
    "sh",
    "echo",
    "xargs",
    "gzip",
    "gunzip",
    "zcat",
    "unzip",
    "tar",
    "du",
    "df",
    "ln",
    "cat",
    "cp",
    "date",
    "env",
    "grep",
    "head",
    "ls",
    "mkdir",
    "mv",
    "rm",
    "rmdir",
    "touch",
    "sort",
    "uniq",
    "cut",
    "tr",
    "sed",
    "sleep",
    "yes",
    "awk",
    "chmod",
    "tail",
    "wc",
    "tee",
    "basename",
    "dirname",
    "pwd",
    "printenv",
    "seq",
    "true",
    "false",
    "find",
    "od",
    "hexdump",
    "base64",
    "sha256sum",
    "realpath",
    "stat",
    "which",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallError {
    pub path: String,
    pub operation: String,
    pub kind: FileError,
}

impl InstallError {
    pub fn new(path: &str, operation: impl Into<String>, kind: FileError) -> Self {
        Self {
            path: path.into(),
            operation: operation.into(),
            kind,
        }
    }
}

impl fmt::Display for InstallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}: {:?}", self.operation, self.path, self.kind)
    }
}

impl core::error::Error for InstallError {}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub version: u32,
    pub modules: Vec<Module>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Module {
    pub path: String,
    pub commands: Vec<String>,
    pub libraries: Vec<String>,
}

pub fn valid_command(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 127
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._+-".contains(&byte))
}

pub fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 4096
        && !path
            .bytes()
            .any(|byte| byte.is_ascii_control() || b"\\:".contains(&byte))
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
        && !matches!(
            path.split('/').next().unwrap(),
            "bin" | "dev" | "proc" | "tmp" | ".efi-agent-fs" | ".efi-process" | ".efi-session"
        )
        && path != MANIFEST_PATH
}

impl Manifest {
    pub fn parse(bytes: &[u8]) -> Result<Self, InstallError> {
        if bytes.len() > MAX_MANIFEST_BYTES {
            return Err(InstallError::new(
                MANIFEST_PATH,
                "Read installation manifest",
                FileError::FileTooLarge,
            ));
        }
        let manifest: Self = serde_json::from_slice(bytes).map_err(|error| {
            InstallError::new(
                MANIFEST_PATH,
                format!("Parse installation manifest ({error})"),
                FileError::Invalid,
            )
        })?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), InstallError> {
        let invalid =
            |path: &str, operation: &str| InstallError::new(path, operation, FileError::Invalid);
        if self.version != 1 || self.modules.len() > MAX_MODULES {
            return Err(invalid(
                MANIFEST_PATH,
                "Check manifest version/module count",
            ));
        }
        let mut sources = BTreeSet::new();
        let mut commands = BTreeSet::new();
        for module in &self.modules {
            if !valid_path(&module.path) || !sources.insert(&module.path) {
                return Err(invalid(&module.path, "Check unique module path"));
            }
            if module.commands.is_empty() || module.libraries.len() > 128 {
                return Err(invalid(&module.path, "Check module commands and assets"));
            }
            for name in &module.commands {
                if !valid_command(name) || !commands.insert(name) || commands.len() > MAX_COMMANDS {
                    return Err(invalid(name, "Check unique command name"));
                }
            }
            let mut assets = BTreeSet::new();
            for path in &module.libraries {
                if !valid_path(path) || path == &module.path || !assets.insert(path) {
                    return Err(invalid(path, "Check module asset path"));
                }
            }
        }
        Ok(())
    }
}
