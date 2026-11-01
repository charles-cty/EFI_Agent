//! Direct provider and boot-volume workspace configuration.
use alloc::{collections::BTreeMap, format, string::String, vec::Vec};
use serde::Deserialize;

#[derive(Clone, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentConfig {
    pub api_base: String,
    pub api_key: String,
    pub model: String,
    #[serde(default = "default_format")]
    pub api_format: String,
    #[serde(default = "default_effort")]
    pub reasoning_effort: String,
    #[serde(default = "default_dns")]
    pub dns_address: [u8; 4],
    #[serde(default = "default_dns_port")]
    pub dns_port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ipv4: Option<StaticIpv4>,
    #[serde(default = "default_workspace")]
    pub workspace: String,
    /// Optional additional DER certificate on the boot volume for a private CA.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ca_certificate: Option<String>,
    /// Guest startup values override the built-in defaults.
    #[serde(default = "default_environment")]
    pub environment: BTreeMap<String, String>,
}

pub fn default_environment() -> BTreeMap<String, String> {
    BTreeMap::from([(String::from("PYTHON_BASIC_REPL"), String::from("1"))])
}

/// Resolve defaults once for new roots. Children inherit their parent's values.
pub fn guest_environment(overrides: Option<&BTreeMap<String, String>>) -> Vec<String> {
    let mut environment = default_environment();
    if let Some(overrides) = overrides {
        environment.extend(overrides.clone());
    }
    environment
        .into_iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect()
}

#[derive(Clone, Deserialize, serde::Serialize)]
pub struct StaticIpv4 {
    pub address: [u8; 4],
    pub subnet_mask: [u8; 4],
    pub gateway: [u8; 4],
}
fn default_format() -> String {
    "chat_completions".into()
}
fn default_effort() -> String {
    "medium".into()
}
fn default_dns_port() -> u16 {
    53
}
fn default_dns() -> [u8; 4] {
    [1, 1, 1, 1]
}
fn default_workspace() -> String {
    "\\work".into()
}

impl AgentConfig {
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in &self.environment {
            if name.is_empty() || name.contains(['=', '\0']) || value.contains('\0') {
                return Err("Guest environment requires non-empty names without '=' or NUL, and values without NUL".into());
            }
        }
        if guest_environment(Some(&self.environment))
            .iter()
            .map(|entry| entry.len() + 1)
            .sum::<usize>()
            > 64 * 1024
        {
            return Err("Guest environment exceeds 64 KiB".into());
        }
        if self.api_key.trim().is_empty() || self.model.trim().is_empty() {
            return Err("API key and model must be non-empty".into());
        }
        if self.api_key.chars().any(|c| c.is_control()) {
            return Err("API key contains a control character".into());
        }
        crate::http::Url::parse(&self.api_base)?;
        crate::model::api_format(Some(&self.api_format))?;
        crate::model::reasoning_effort(Some(&self.reasoning_effort))?;
        if self.dns_port == 0 || self.dns_address == [0; 4] || self.dns_address[0] >= 224 {
            return Err("DNS requires a nonzero unicast IPv4 address".into());
        }
        if let Some(ipv4) = &self.ipv4 {
            let address = u32::from_be_bytes(ipv4.address);
            let mask = u32::from_be_bytes(ipv4.subnet_mask);
            let gateway = u32::from_be_bytes(ipv4.gateway);
            let host_mask = !mask;
            if ipv4.address == [0; 4]
                || ipv4.address[0] >= 224
                || ipv4.gateway == [0; 4]
                || ipv4.gateway[0] >= 224
                || mask == 0
                || host_mask & host_mask.wrapping_add(1) != 0
                || (address & mask) != (gateway & mask)
                || (address & host_mask == 0)
                || (address & host_mask == host_mask)
            {
                return Err("Static IPv4 address, subnet mask, or gateway is invalid".into());
            }
        }
        let workspace_components = components(&self.workspace)?;
        if !self.workspace.starts_with('\\') || workspace_components.is_empty() {
            return Err("Workspace must be an absolute UEFI directory".into());
        }
        if let Some(path) = &self.ca_certificate {
            components(path)?;
            if !path.starts_with('\\') {
                return Err("CA certificate path must be absolute".into());
            }
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

/// Resolve a module path on the boot volume, relative to the configured workspace.
pub fn resolve_module_path(workspace: &str, path: &str) -> Result<String, String> {
    let root = components(workspace)?;
    if !workspace.starts_with('\\') || root.is_empty() {
        return Err("Workspace must be an absolute UEFI directory".into());
    }
    let module = components(path)?;
    if module.is_empty() {
        return Err("Module path must name a file".into());
    }
    if path.starts_with(['\\', '/']) {
        Ok(format!("\\{}", module.join("\\")))
    } else {
        Ok(format!("\\{}\\{}", root.join("\\"), module.join("\\")))
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

    fn configuration() -> AgentConfig {
        serde_json::from_str(
            r#"{"api_base":"https://provider.example/v1","api_key":"test","model":"test"}"#,
        )
        .unwrap()
    }

    #[test]
    fn guest_environment_defaults_merge_with_explicit_overrides() {
        let mut config = configuration();
        config.validate().unwrap();
        assert_eq!(guest_environment(None), ["PYTHON_BASIC_REPL=1"]);
        assert_eq!(
            guest_environment(Some(&config.environment)),
            ["PYTHON_BASIC_REPL=1"]
        );
        config.environment = BTreeMap::from([
            ("CUSTOM".into(), "a=b λ".into()),
            ("PYTHON_BASIC_REPL".into(), String::new()),
        ]);
        config.validate().unwrap();
        assert_eq!(
            guest_environment(Some(&config.environment)),
            ["CUSTOM=a=b λ", "PYTHON_BASIC_REPL="]
        );
        let restored: AgentConfig =
            serde_json::from_slice(&serde_json::to_vec(&config).unwrap()).unwrap();
        assert_eq!(restored.environment, config.environment);
        config.environment.remove("PYTHON_BASIC_REPL");
        assert_eq!(
            guest_environment(Some(&config.environment)),
            ["CUSTOM=a=b λ", "PYTHON_BASIC_REPL=1"]
        );
    }

    #[test]
    fn invalid_guest_environment_and_oversized_values_are_rejected() {
        for (name, value) in [("", "x"), ("A=B", "x"), ("A\0B", "x"), ("A", "x\0y")] {
            let mut config = configuration();
            config.environment.insert(name.into(), value.into());
            assert!(config.validate().is_err());
        }
        // Default entry takes 20 bytes; X=value plus NUL takes value.len() + 3.
        for (size, valid) in [(65_512, true), (65_513, true), (65_514, false)] {
            let mut config = configuration();
            config.environment.insert("X".into(), "x".repeat(size));
            assert_eq!(config.validate().is_ok(), valid, "{size}");
        }
    }

    #[test]
    fn module_paths_use_the_workspace_or_explicit_boot_volume_root() {
        let workspace = "\\projects\\demo";
        for path in ["python.wasm", "./python.wasm", ".\\python.wasm"] {
            assert_eq!(
                resolve_module_path(workspace, path).unwrap(),
                "\\projects\\demo\\python.wasm"
            );
        }
        for path in [
            "modules/python.wasm",
            "modules\\python.wasm",
            "modules/./python.wasm",
        ] {
            assert_eq!(
                resolve_module_path(workspace, path).unwrap(),
                "\\projects\\demo\\modules\\python.wasm"
            );
        }
        for path in [
            "/work/python.wasm",
            "\\work\\python.wasm",
            "/work\\python.wasm",
        ] {
            assert_eq!(
                resolve_module_path(workspace, path).unwrap(),
                "\\work\\python.wasm"
            );
        }
        assert_eq!(
            resolve_module_path("\\work", "work/python.wasm").unwrap(),
            "\\work\\work\\python.wasm"
        );
        for path in [
            "",
            ".",
            "/",
            "../python.wasm",
            "modules/../python.wasm",
            "C:\\python.wasm",
            "fs0:\\python.wasm",
            "python.wasm\0",
            "python.wasm.",
        ] {
            assert!(resolve_module_path(workspace, path).is_err(), "{path:?}");
        }
        assert!(resolve_module_path("relative", "python.wasm").is_err());
    }

    #[test]
    fn native_paths_are_rooted_and_traversal_is_rejected() {
        let config = AgentConfig {
            api_base: "https://provider.example/v1".into(),
            api_key: "test".into(),
            model: "test".into(),
            api_format: default_format(),
            reasoning_effort: default_effort(),
            dns_address: default_dns(),
            dns_port: default_dns_port(),
            ipv4: None,
            ca_certificate: None,
            workspace: "\\work\\project".into(),
            environment: default_environment(),
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
        let bad = AgentConfig {
            workspace: "relative".into(),
            ..config
        };
        assert!(bad.validate().is_err());
    }
}
