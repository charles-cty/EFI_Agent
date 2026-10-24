//! Transactional discovery and validation of file-backed commands.
use crate::{
    Context, World,
    filesystem::{self, EntryType, FileError, FileSystem, OpenOptions},
};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    format,
    string::String,
    vec,
    vec::Vec,
};
pub use efi_agent_filesystem::installation::{InstallError, MANIFEST_PATH, Manifest};
use efi_agent_filesystem::installation::{
    MAX_COMMANDS, MAX_MANIFEST_BYTES, MAX_MODULES, valid_command,
};
use wasmi::{CompilationMode, Engine, ExternType, FuncType, IntoFunc, Linker, WasmTyList};

/// Record signatures at the same call site that installs each host function.
pub(crate) struct HostLinker {
    pub linker: Linker<Context>,
    pub signatures: BTreeMap<(String, String), FuncType>,
}

impl HostLinker {
    pub fn new(engine: &Engine) -> Self {
        Self {
            linker: Linker::new(engine),
            signatures: BTreeMap::new(),
        }
    }

    pub fn func_wrap<P, R, F>(
        &mut self,
        module: &str,
        name: &str,
        func: F,
    ) -> Result<&mut Self, wasmi::errors::LinkerError>
    where
        F: IntoFunc<Context, P, R>,
    {
        let signature = FuncType::new(F::Params::types(), F::Results::types());
        self.linker.func_wrap(module, name, func)?;
        self.signatures
            .insert((module.into(), name.into()), signature);
        Ok(self)
    }
}

pub(crate) type Revision = (u64, u64, u64, u64);

fn read_file(
    fs: &dyn FileSystem,
    path: &str,
    limit: usize,
) -> Result<(Vec<u8>, Option<Revision>), InstallError> {
    let error = |kind| InstallError::new(path, "Read installation file", kind);
    let mut file = fs.open(path, read_options()).map_err(error)?;
    let info = file.metadata().map_err(error)?;
    if info.kind != EntryType::File {
        return Err(error(FileError::Invalid));
    }
    if info.size > limit as u64 {
        return Err(error(FileError::FileTooLarge));
    }
    let revision = file
        .content_revision()
        .map_err(error)?
        .map(|rev| (info.device, info.inode, rev, info.size));
    let mut bytes = vec![0; info.size as usize];
    let mut used = 0;
    while used < bytes.len() {
        let end = (used + crate::PIPE_CAPACITY).min(bytes.len());
        let count = file.read(&mut bytes[used..end]).map_err(error)?;
        if count == 0 {
            return Err(error(FileError::Io));
        }
        used += count;
    }
    Ok((bytes, revision))
}

impl World {
    /// Validate an installation before publishing aliases. Translation and
    /// linear-memory allocation occur only when a command is launched.
    pub fn discover_programs(&mut self) -> Result<(), InstallError> {
        let fs = self
            .filesystem
            .clone()
            .ok_or_else(|| InstallError::new("/", "Discover commands", FileError::Unsupported))?;
        let manifest = match fs.metadata(MANIFEST_PATH) {
            Ok(_) => Some(Manifest::parse(
                &read_file(fs.as_ref(), MANIFEST_PATH, MAX_MANIFEST_BYTES)?.0,
            )?),
            Err(FileError::NotFound) => None,
            Err(error) => {
                return Err(InstallError::new(
                    MANIFEST_PATH,
                    "Find installation manifest",
                    error,
                ));
            }
        };
        let mut pending = BTreeMap::new();
        let mut sources = BTreeSet::new();
        if let Some(manifest) = &manifest {
            for module in &manifest.modules {
                for path in &module.libraries {
                    let kind = fs
                        .metadata(path)
                        .map_err(|error| InstallError::new(path, "Find module library", error))?
                        .kind;
                    if !matches!(kind, EntryType::File | EntryType::Directory) {
                        return Err(InstallError::new(
                            path,
                            "Check module library type",
                            FileError::Invalid,
                        ));
                    }
                }
                for path in &module.licenses {
                    if fs
                        .metadata(path)
                        .map_err(|error| InstallError::new(path, "Find module license", error))?
                        .kind
                        != EntryType::File
                    {
                        return Err(InstallError::new(
                            path,
                            "Check module license type",
                            FileError::Invalid,
                        ));
                    }
                }
                sources.insert(module.path.clone());
                for name in &module.commands {
                    pending.insert(name.clone(), module.path.clone());
                }
            }
        }
        let entries = fs
            .read_directory("")
            .map_err(|error| InstallError::new("/", "List workspace modules", error))?;
        for (path, kind) in entries.iter() {
            if kind != EntryType::File || sources.contains(path) || path.len() < 5 {
                continue;
            }
            let Some(stem) = path.get(..path.len() - 5) else {
                continue;
            };
            let suffix = &path[stem.len()..];
            if !suffix.eq_ignore_ascii_case(".wasm") {
                continue;
            }
            if !valid_command(stem) {
                return Err(InstallError::new(
                    path,
                    "Check module command name",
                    FileError::Invalid,
                ));
            }
            if pending.contains_key(stem) {
                continue;
            }
            sources.insert(path.into());
            pending.insert(stem.into(), path.into());
        }
        if !pending.contains_key("qjs")
            && let Some(path) = pending.get("quickjs").cloned()
        {
            pending.insert("qjs".into(), path);
        }
        if sources.len() > MAX_MODULES || pending.len() > MAX_COMMANDS {
            return Err(InstallError::new(
                MANIFEST_PATH,
                "Check installation module/command count",
                FileError::ResourceLimit,
            ));
        }
        for path in &sources {
            self.validate_installation_module(fs.as_ref(), path)?;
        }

        // Dynamic explicit paths are separate from installation-owned names.
        let previous = self.program_files.clone();
        for name in &self.installed_names {
            self.program_files.remove(name);
        }
        for (name, path) in &pending {
            if !self.programs.contains_key(name) {
                self.program_files.insert(name.clone(), path.clone());
            }
        }
        if let Err(error) = self.refresh_bin() {
            self.program_files = previous;
            return Err(InstallError::new(
                "/bin",
                "Publish installed commands",
                error,
            ));
        }
        self.installed_names = pending.into_keys().collect();
        self.validated_files
            .retain(|path, _| sources.contains(path));
        Ok(())
    }

    fn validate_installation_module(
        &mut self,
        fs: &dyn FileSystem,
        path: &str,
    ) -> Result<(), InstallError> {
        let error = |kind| InstallError::new(path, "Validate command module", kind);
        let mut file = fs.open(path, read_options()).map_err(error)?;
        let info = file.metadata().map_err(error)?;
        if info.kind != EntryType::File {
            return Err(error(FileError::Invalid));
        }
        if info.size > filesystem::MAX_RESIZE_BYTES {
            return Err(error(FileError::FileTooLarge));
        }
        let revision = file
            .content_revision()
            .map_err(error)?
            .map(|rev| (info.device, info.inode, rev, info.size));
        if let Some(revision) = revision
            && self.validated_files.get(path) == Some(&revision)
        {
            return Ok(());
        }
        drop(file);
        let (bytes, revision) = read_file(fs, path, filesystem::MAX_RESIZE_BYTES as usize)?;
        let mut config = self.engine.config().clone();
        config.compilation_mode(CompilationMode::LazyTranslation);
        // A short-lived engine releases validation buffers instead of retaining
        // a second compiled image for the installed program's lifetime.
        let engine = Engine::new(&config);
        let module = wasmi::Module::new(&engine, bytes).map_err(|cause| {
            InstallError::new(
                path,
                format!("Validate command module ({cause})"),
                FileError::Invalid,
            )
        })?;
        for import in module.imports() {
            let signature = self
                .import_signatures
                .get(&(import.module().into(), import.name().into()));
            if !matches!(import.ty(), ExternType::Func(ty) if signature == Some(ty)) {
                return Err(InstallError::new(
                    path,
                    format!(
                        "Check supported import {}.{}",
                        import.module(),
                        import.name()
                    ),
                    FileError::Unsupported,
                ));
            }
        }
        if !matches!(module.get_export("_start"), Some(ExternType::Func(ty)) if ty.params().is_empty() && ty.results().is_empty())
            || !matches!(module.get_export("memory"), Some(ExternType::Memory(_)))
        {
            return Err(InstallError::new(
                path,
                "Check command memory and _start exports",
                FileError::Invalid,
            ));
        }
        if let Some(revision) = revision {
            self.validated_files.insert(path.into(), revision);
        }
        Ok(())
    }
}

fn read_options() -> OpenOptions {
    OpenOptions {
        read: true,
        write: false,
        create: false,
        exclusive: false,
        truncate: false,
        append: false,
    }
}
