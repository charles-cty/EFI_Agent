use efi_agent_core::protocol::{self, Operation, Request, Response};
use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Component, Path, PathBuf},
    time::Duration,
};

pub struct Bridge {
    root: PathBuf,
    client: reqwest::blocking::Client,
}

impl Bridge {
    pub fn new(root: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            root: root.canonicalize()?,
            client: reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()?,
        })
    }

    /// Resolve relative paths and check the final target (or parent for new files).
    fn resolve(&self, path: &str, create: bool) -> Result<PathBuf, String> {
        let relative = Path::new(path);
        if relative.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        }) {
            return Err("Path must stay inside the workspace".into());
        }
        let joined = self.root.join(relative);
        let resolved = if create && !joined.exists() {
            let parent = joined
                .parent()
                .ok_or("Missing parent directory")?
                .canonicalize()
                .map_err(|e| e.to_string())?;
            parent.join(joined.file_name().ok_or("Missing file name")?)
        } else {
            joined.canonicalize().map_err(|e| e.to_string())?
        };
        if !resolved.starts_with(&self.root) {
            return Err("Path leaves the workspace".into());
        }
        Ok(resolved)
    }

    pub fn execute(&self, operation: Operation) -> Result<String, String> {
        match operation {
            Operation::List { path } => {
                let mut entries = fs::read_dir(self.resolve(&path, false)?)
                    .map_err(|e| e.to_string())?
                    .map(|entry| entry.map(|e| e.file_name().to_string_lossy().into_owned()))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| e.to_string())?;
                entries.sort();
                Ok(entries.join("\n"))
            }
            Operation::Read { path } => {
                let path = self.resolve(&path, false)?;
                if fs::metadata(&path).map_err(|e| e.to_string())?.len() > 512 * 1024 {
                    return Err("File exceeds 512 KiB".into());
                }
                fs::read_to_string(path).map_err(|e| e.to_string())
            }
            Operation::Write { path, content } => {
                fs::write(self.resolve(&path, true)?, content).map_err(|e| e.to_string())?;
                Ok("File saved".into())
            }
            Operation::Complete { messages } => {
                let base = std::env::var("EFI_AGENT_API_BASE")
                    .map_err(|_| "Set EFI_AGENT_API_BASE to the provider base URL")?;
                let key =
                    std::env::var("EFI_AGENT_API_KEY").map_err(|_| "Set EFI_AGENT_API_KEY")?;
                let model = std::env::var("EFI_AGENT_MODEL").map_err(|_| "Set EFI_AGENT_MODEL")?;
                let result: serde_json::Value = self
                    .client
                    .post(format!("{}/chat/completions", base.trim_end_matches('/')))
                    .bearer_auth(key)
                    .json(&serde_json::json!({"model":model,"messages":messages,"stream":false}))
                    .send()
                    .map_err(|e| e.to_string())?
                    .error_for_status()
                    .map_err(|e| e.to_string())?
                    .json()
                    .map_err(|e| e.to_string())?;
                result["choices"][0]["message"]["content"]
                    .as_str()
                    .map(str::to_owned)
                    .ok_or("Provider returned no text".into())
            }
        }
    }

    pub fn connection(&self, mut stream: TcpStream) -> Result<(), Box<dyn std::error::Error>> {
        stream.set_read_timeout(Some(Duration::from_secs(180)))?;
        stream.set_write_timeout(Some(Duration::from_secs(30)))?;
        loop {
            let mut header = [0; 4];
            match stream.read_exact(&mut header) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
                Err(e) => return Err(e.into()),
            }
            let mut body = vec![0; protocol::frame_length(header)?];
            stream.read_exact(&mut body)?;
            let request: Request = serde_json::from_slice(&body)?;
            let response = Response {
                id: request.id,
                result: self.execute(request.operation),
            };
            let frame = protocol::encode(&response).unwrap_or_else(|_| {
                protocol::encode(&Response {
                    id: response.id,
                    result: Err("Response exceeds frame limit".into()),
                })
                .expect("Small error frame")
            });
            stream.write_all(&frame)?;
        }
    }
}

pub fn serve(listener: TcpListener, root: PathBuf) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let bridge = match Bridge::new(&root) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("HostBridge: {e}");
                return;
            }
        };
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    if let Err(e) = bridge.connection(stream) {
                        eprintln!("HostBridge: {e}");
                    }
                }
                Err(e) => {
                    eprintln!("HostBridge accept: {e}");
                    break;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_socket_fragmentation_and_files() {
        let root = std::env::temp_dir().join(format!("efi-bridge-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("asymmetric.txt"), "three 中").unwrap();
        let bridge = Bridge::new(&root).unwrap();
        assert!(bridge.resolve("../outside", false).is_err());
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            bridge
                .connection(listener.accept().unwrap().0)
                .map_err(|e| e.to_string())
        });
        let mut stream = TcpStream::connect(address).unwrap();
        let request = Request {
            id: 731,
            operation: Operation::Read {
                path: "asymmetric.txt".into(),
            },
        };
        for byte in protocol::encode(&request).unwrap() {
            stream.write_all(&[byte]).unwrap();
        }
        let mut header = [0; 4];
        stream.read_exact(&mut header).unwrap();
        let mut bytes = vec![0; protocol::frame_length(header).unwrap()];
        stream.read_exact(&mut bytes).unwrap();
        let response: Response = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(response.id, 731);
        assert_eq!(response.result.unwrap(), "three 中");
        drop(stream);
        worker.join().unwrap().unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}
