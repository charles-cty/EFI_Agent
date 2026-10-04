//! Bare-metal model relay and native workspace configuration.
use alloc::{format, string::String, vec::Vec};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeConfig {
    pub relay_address: [u8; 4],
    pub relay_port: u16,
    pub workspace: String,
}

impl NativeConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.relay_port == 0 || self.relay_address == [0; 4] || self.relay_address[0] >= 224 {
            return Err(String::from(
                "Model relay requires a unicast IPv4 address and a nonzero port",
            ));
        }
        let components = components(&self.workspace)?;
        if !self.workspace.starts_with('\\') || components.is_empty() {
            return Err(String::from(
                "Native workspace must be an absolute UEFI directory",
            ));
        }
        Ok(())
    }

    pub fn resolve(&self, path: &str) -> Result<String, String> {
        self.validate()?;
        if path.starts_with(['\\', '/']) {
            return Err(String::from(
                "Tool paths must be relative to the native workspace",
            ));
        }
        let relative = components(path)?;
        let root = components(&self.workspace)?.join("\\");
        if relative.is_empty() {
            Ok(format!("\\{root}"))
        } else {
            Ok(format!("\\{root}\\{}", relative.join("\\")))
        }
    }
}

fn components(path: &str) -> Result<Vec<&str>, String> {
    if path
        .chars()
        .any(|c| c.is_control() || matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|'))
    {
        return Err(String::from("Invalid native workspace path"));
    }
    let mut result = Vec::new();
    for part in path.split(['\\', '/']) {
        if part == ".." || part.ends_with([' ', '.']) && part != "." {
            return Err(String::from("Native path must stay inside the workspace"));
        }
        if !part.is_empty() && part != "." {
            result.push(part);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_paths_are_rooted_and_traversal_is_rejected() {
        let config = NativeConfig {
            relay_address: [192, 168, 1, 73],
            relay_port: 7420,
            workspace: "\\work\\project".into(),
        };
        assert_eq!(
            config.resolve("src/main.rs").unwrap(),
            "\\work\\project\\src\\main.rs"
        );
        assert_eq!(config.resolve(".").unwrap(), "\\work\\project");
        for path in [
            "../secret",
            "src\\..\\secret",
            "/outside",
            "\\outside",
            "C:secret",
            "foo. ",
            "foo..",
            "nul\0name",
        ] {
            assert!(config.resolve(path).is_err(), "{path:?}");
        }
        let bad = NativeConfig {
            workspace: "relative".into(),
            ..config
        };
        assert!(bad.validate().is_err());
    }
}
