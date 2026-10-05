//! Desktop clipboard ownership and Linux compositor integration.
#[cfg(target_os = "linux")]
use std::{
    io::Write,
    process::{Command, Stdio},
};

pub enum Clipboard {
    Desktop(arboard::Clipboard),
    #[cfg(target_os = "linux")]
    WaylandTools,
}

impl Clipboard {
    pub fn new() -> Result<Self, String> {
        match arboard::Clipboard::new() {
            Ok(clipboard) => Ok(Self::Desktop(clipboard)),
            Err(error) => {
                #[cfg(target_os = "linux")]
                if std::env::var_os("WAYLAND_DISPLAY").is_some() {
                    // wl-clipboard also supports compositors without data-control.
                    // Probe executables without changing clipboard contents.
                    for tool in ["wl-copy", "wl-paste"] {
                        let output = Command::new(tool)
                            .arg("--version")
                            .output()
                            .map_err(|e| format!("Wayland clipboard requires wl-clipboard: {e}"))?;
                        if !output.status.success() {
                            return Err(format!("Could not start {tool}"));
                        }
                    }
                    return Ok(Self::WaylandTools);
                }
                Err(error.to_string())
            }
        }
    }

    pub fn get_text(&mut self) -> Result<String, String> {
        match self {
            Self::Desktop(clipboard) => clipboard.get_text().map_err(|e| e.to_string()),
            #[cfg(target_os = "linux")]
            Self::WaylandTools => {
                let output = Command::new("wl-paste")
                    .args(["--no-newline", "--type", "text"])
                    .output()
                    .map_err(|e| e.to_string())?;
                if !output.status.success() {
                    return Err(String::from_utf8_lossy(&output.stderr).into_owned());
                }
                String::from_utf8(output.stdout).map_err(|e| e.to_string())
            }
        }
    }

    pub fn set_text(&mut self, text: impl Into<String>) -> Result<(), String> {
        let text = text.into();
        match self {
            Self::Desktop(clipboard) => clipboard.set_text(text).map_err(|e| e.to_string()),
            #[cfg(target_os = "linux")]
            Self::WaylandTools => {
                // The helper retains ownership in its daemon after initialization.
                // Keep helper diagnostics out of the active terminal.
                let mut child = Command::new("wl-copy")
                    .args(["--type", "text/plain;charset=utf-8"])
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .map_err(|e| e.to_string())?;
                let write = child
                    .stdin
                    .take()
                    .expect("Piped clipboard input")
                    .write_all(text.as_bytes());
                // The owner daemon can retain stderr; waiting for pipe EOF would
                // freeze editing until another copy replaced the selection.
                let status = child.wait().map_err(|e| e.to_string())?;
                write.map_err(|e| e.to_string())?;
                if !status.success() {
                    return Err(format!("wl-copy failed: {status}"));
                }
                Ok(())
            }
        }
    }
}
