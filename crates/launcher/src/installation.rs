//! Select prepared command assets and merge them without overwriting user data.
use crate::workspace::{self, Contents, Snapshot};
use efi_agent_filesystem::installation::{
    ASH_APPLETS, MANIFEST_PATH, MAX_MANIFEST_BYTES, Manifest, Module,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::Path,
};
type Error = Box<dyn std::error::Error>;

fn selected(root: &Path, manifest: &Manifest, include_manifest: bool) -> Result<Snapshot, Error> {
    manifest.validate()?;
    let mut paths = BTreeSet::new();
    if include_manifest {
        paths.insert(MANIFEST_PATH.into());
    }
    for module in &manifest.modules {
        for (path, directory_allowed) in std::iter::once((&module.path, false))
            .chain(module.libraries.iter().map(|path| (path, true)))
        {
            let info = fs::symlink_metadata(root.join(path))
                .map_err(|error| format!("Find installation asset {path}: {error}"))?;
            if !(info.is_file() || directory_allowed && info.is_dir()) {
                return Err(format!("Invalid installation asset type: {path}").into());
            }
            paths.insert(path.clone());
        }
        let mut magic = [0; 8];
        fs::File::open(root.join(&module.path))?.read_exact(&mut magic)?;
        if magic != *b"\0asm\x01\0\0\0" {
            return Err(format!("Invalid command binary: {}", module.path).into());
        }
    }
    let mut snapshot = workspace::host_selected(root, Some(&paths))?;
    for module in &manifest.modules {
        snapshot
            .get_mut(&module.path)
            .expect("Selected module exists")
            .attributes
            .mode = 0o755;
    }
    Ok(snapshot)
}

pub fn assets(root: &Path) -> Result<Snapshot, Error> {
    let mut bytes = Vec::new();
    fs::File::open(root.join(MANIFEST_PATH))?
        .take(MAX_MANIFEST_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    selected(root, &Manifest::parse(&bytes)?, true)
}

/// Distribution notices are packaged outside the persistent workspace.
pub fn licenses(root: &Path) -> Result<BTreeMap<String, Vec<u8>>, Error> {
    let mut notices = BTreeMap::new();
    for entry in fs::read_dir(root.join("licenses"))? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            return Err(format!(
                "License notice must be a regular file: {}",
                entry.path().display()
            )
            .into());
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "Invalid license filename")?;
        notices.insert(name, fs::read(entry.path())?);
    }
    if notices.is_empty() {
        return Err("Prepared distribution licenses are missing".into());
    }
    Ok(notices)
}

pub fn overlay(workspace: &Snapshot, assets: &Snapshot) -> Result<Snapshot, Error> {
    let mut merged = workspace.clone();
    for (path, asset) in assets {
        if let Some(existing) = merged.get(path) {
            if existing.contents != asset.contents {
                return Err(format!("Installation conflicts with workspace path: {path}").into());
            }
        } else {
            merged.insert(path.clone(), asset.clone());
        }
    }
    workspace::validate(&merged)?;
    Ok(merged)
}

pub fn packaged(tree: &Snapshot) -> Result<Snapshot, Error> {
    let Some(entry) = tree.get(MANIFEST_PATH) else {
        return Ok(Snapshot::new());
    };
    let Contents::File(bytes) = &entry.contents else {
        return Err("Packaged manifest must be a file".into());
    };
    let manifest = Manifest::parse(bytes)?;
    let mut paths = BTreeSet::from([MANIFEST_PATH.to_string()]);
    for module in &manifest.modules {
        for path in std::iter::once(&module.path).chain(&module.libraries) {
            if !tree.contains_key(path) {
                return Err(format!("Packaged installation asset is missing: {path}").into());
            }
            paths.insert(path.clone());
        }
    }
    let selected: Snapshot = tree
        .iter()
        .filter(|(path, _)| {
            paths.iter().any(|wanted| {
                *path == wanted
                    || path.starts_with(&format!("{wanted}/"))
                    || wanted.starts_with(&format!("{path}/"))
            })
        })
        .map(|(path, entry)| (path.clone(), entry.clone()))
        .collect();
    workspace::validate(&selected)?;
    Ok(selected)
}

pub fn stage(tree: &Snapshot, destination: &Path) -> Result<(), Error> {
    let temporary = destination.with_extension(format!("install-{}", std::process::id()));
    let backup = destination.with_extension(format!("previous-{}", std::process::id()));
    if temporary.exists() || backup.exists() {
        return Err(format!(
            "Installation recovery directory exists beside {}",
            destination.display()
        )
        .into());
    }
    if let Err(error) = workspace::stage(tree, &temporary) {
        let _ = fs::remove_dir_all(&temporary);
        return Err(error);
    }
    let existing = destination.exists();
    if existing {
        fs::rename(destination, &backup)?;
    }
    if let Err(error) = fs::rename(&temporary, destination) {
        if existing {
            fs::rename(&backup, destination)?;
        }
        let _ = fs::remove_dir_all(&temporary);
        return Err(error.into());
    }
    if existing {
        fs::remove_dir_all(backup)?;
    }
    Ok(())
}

pub fn install(source: &Path, destination: &Path) -> Result<(), Error> {
    let assets = assets(source)?;
    let baseline = workspace::host(destination)?;
    let merged = overlay(&baseline, &assets)?;
    workspace::save(destination, &baseline, &merged)
}

pub fn run(args: Vec<String>) -> Result<(), Error> {
    if args.len() == 3 && args[2] == "--guest" {
        let source = assets(Path::new(&args[0]))?;
        let destination = Path::new(&args[1]);
        let baseline = workspace::guest(destination, &Snapshot::new())?;
        let merged = overlay(&baseline, &source)?;
        return stage(&merged, destination);
    }
    if args.len() != 2 {
        return Err("Usage: efi-agent install <prepared-directory> <workspace> [--guest]".into());
    }
    install(Path::new(&args[0]), Path::new(&args[1]))
}

/// Generate the catalog for the five prepared application ports. User-supplied
/// modules use the same schema through an explicit installation manifest.
pub fn catalog(args: Vec<String>) -> Result<(), Error> {
    if args.len() != 1 {
        return Err("Usage: efi-agent catalog <prepared-directory>".into());
    }
    let root = Path::new(&args[0]);
    let module = |path: &str, commands: &[&str], libraries: &[&str]| Module {
        path: path.into(),
        commands: commands.iter().map(|name| (*name).into()).collect(),
        libraries: libraries.iter().map(|name| (*name).into()).collect(),
    };
    let manifest = Manifest {
        version: 1,
        modules: vec![
            module("ash.wasm", ASH_APPLETS, &["share/zoneinfo"]),
            module("python.wasm", &["python"], &["lib/python3.14"]),
            module("curl.wasm", &["curl"], &[]),
            module("quickjs.wasm", &["quickjs", "qjs"], &[]),
            module("wolfssh.wasm", &["wolfssh"], &[]),
        ],
    };
    selected(root, &manifest, false)?;
    let notices = licenses(root)?;
    for name in [
        "ash.LICENSE",
        "musl.COPYRIGHT",
        "tzdata.LICENSE",
        "python.LICENSE",
        "curl.LICENSE",
        "wolfssl.COPYING",
        "quickjs.LICENSE",
        "zx.LICENSE",
        "zx-third-party.LICENSE",
        "wolfssh.LICENSING",
    ] {
        if !notices.contains_key(name) {
            return Err(format!("Prepared distribution license is missing: {name}").into());
        }
    }
    let bytes = serde_json::to_vec_pretty(&manifest)?;
    // A failed catalog validation leaves the previous catalog intact.
    fs::write(root.join(MANIFEST_PATH), bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn selected_assets_preserve_user_files_and_reject_conflicts_before_effects() {
        let root = std::env::temp_dir().join(format!(
            "efi-install-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = root.join("prepared");
        let destination = root.join("workspace");
        fs::create_dir_all(source.join("lib")).unwrap();
        fs::create_dir(source.join("licenses")).unwrap();
        fs::create_dir(&destination).unwrap();
        fs::write(source.join("tool.wasm"), b"\0asm\x01\0\0\0").unwrap();
        fs::write(source.join("licenses/COPYING"), b"license").unwrap();
        fs::write(source.join("lib/data"), b"library").unwrap();
        fs::create_dir(source.join("unselected-build")).unwrap();
        let sparse = fs::File::create(source.join("unselected-build/large")).unwrap();
        sparse.set_len(49 * 1024 * 1024).unwrap();
        drop(sparse);
        fs::write(source.join(MANIFEST_PATH), br#"{"version":1,"modules":[{"path":"tool.wasm","commands":["one","two"],"libraries":["lib"]}]}"#).unwrap();
        fs::write(destination.join("user.py"), b"keep").unwrap();
        fs::write(destination.join("COPYING"), b"user notice").unwrap();
        install(&source, &destination).unwrap();
        let installed = workspace::host(&destination).unwrap();
        assert_eq!(installed.len(), 6);
        assert!(!installed.contains_key("licenses"));
        assert_eq!(
            fs::read(destination.join("COPYING")).unwrap(),
            b"user notice"
        );
        assert_eq!(licenses(&source).unwrap()["COPYING"], b"license");
        assert!(!installed.contains_key("unselected-build"));
        assert!(!installed.contains_key("one"));
        assert_eq!(installed["tool.wasm"].attributes.mode, 0o755);
        assert_eq!(fs::read(destination.join("user.py")).unwrap(), b"keep");
        install(&source, &destination).unwrap();
        let baseline = workspace::host(&destination).unwrap();
        let esp = root.join("esp");
        fs::create_dir_all(esp.join("EFI/BOOT")).unwrap();
        fs::write(esp.join("EFI/BOOT/BOOTX64.EFI"), b"test EFI").unwrap();
        stage(&baseline, &esp.join("work")).unwrap();
        // Repeated packaging uses the logical tree, including its metadata.
        stage(&baseline, &esp.join("work")).unwrap();
        let image = root.join("package.img");
        crate::pack::run(vec![
            esp.to_string_lossy().into(),
            image.to_string_lossy().into(),
        ])
        .unwrap();
        let extracted = root.join("extracted");
        crate::boot::extract(&image, &extracted).unwrap();
        let recovered = workspace::guest(&extracted.join("work"), &Snapshot::new()).unwrap();
        assert_eq!(recovered, baseline);
        let defaults = packaged(&recovered).unwrap();
        assert_eq!(defaults.len(), 4);
        assert!(!defaults.contains_key("COPYING"));
        assert!(!defaults.contains_key("user.py"));
        assert_eq!(overlay(&baseline, &defaults).unwrap(), baseline);
        let guest = root.join("guest");
        fs::create_dir(&guest).unwrap();
        fs::write(guest.join("user.py"), b"keep").unwrap();
        run(vec![
            source.to_string_lossy().into(),
            guest.to_string_lossy().into(),
            "--guest".into(),
        ])
        .unwrap();
        let installed_guest = workspace::guest(&guest, &Snapshot::new()).unwrap();
        assert_eq!(installed_guest["tool.wasm"].attributes.mode, 0o755);
        assert_eq!(
            installed_guest["user.py"].contents,
            Contents::File(std::rc::Rc::from(b"keep".as_slice()))
        );
        assert!(!installed_guest.contains_key(".efi-agent-workspace.json"));
        assert!(!installed_guest.contains_key("COPYING"));
        assert!(!installed_guest.contains_key("licenses"));
        fs::write(source.join("tool.wasm"), b"\0asm\x01\0\0\0changed").unwrap();
        assert!(
            install(&source, &destination)
                .unwrap_err()
                .to_string()
                .contains("tool.wasm")
        );
        assert_eq!(workspace::host(&destination).unwrap(), baseline);
        fs::remove_file(source.join("licenses/COPYING")).unwrap();
        assert!(licenses(&source).is_err());
        assert!(assets(&source).is_ok());
        assert_eq!(workspace::host(&destination).unwrap(), baseline);
        fs::remove_dir_all(root).unwrap();
    }
}
