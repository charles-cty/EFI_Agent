//! Disk-backed command output with bounded head and tail previews.
use crate::{File, FileError, FileSystem, OpenOptions};
use alloc::{boxed::Box, collections::VecDeque, format, string::String, vec::Vec};

const HALF_PREVIEW: usize = 32 * 1024;

pub struct Log {
    pub path: String,
    pub bytes: u64,
    file: Box<dyn File>,
    head: Vec<u8>,
    tail: VecDeque<u8>,
}

impl Log {
    fn new(filesystem: &dyn FileSystem, path: String) -> Result<Self, FileError> {
        let file = filesystem.open(
            path.trim_start_matches('/'),
            OpenOptions {
                read: false,
                write: true,
                create: true,
                exclusive: true,
                truncate: false,
                append: false,
            },
        )?;
        Ok(Self {
            path,
            bytes: 0,
            file,
            head: Vec::new(),
            tail: VecDeque::new(),
        })
    }

    pub fn append(&mut self, mut bytes: &[u8]) -> Result<(), FileError> {
        while !bytes.is_empty() {
            let count = self.file.write(bytes)?;
            if count == 0 {
                return Err(FileError::NoSpace);
            }
            let written = &bytes[..count];
            self.bytes += count as u64;
            let head = (HALF_PREVIEW - self.head.len()).min(written.len());
            self.head.extend_from_slice(&written[..head]);
            for &byte in &written[head..] {
                if self.tail.len() == HALF_PREVIEW {
                    self.tail.pop_front();
                }
                self.tail.push_back(byte);
            }
            bytes = &bytes[count..];
        }
        Ok(())
    }

    pub fn preview(&self) -> String {
        let tail: Vec<_> = self.tail.iter().copied().collect();
        let omitted = self
            .bytes
            .saturating_sub((self.head.len() + tail.len()) as u64);
        let mut output = String::from_utf8_lossy(&self.head).into_owned();
        if omitted != 0 {
            output.push_str(&format!(
                "\n[Output preview omits {omitted} bytes; full output: {}]\n",
                self.path
            ));
        }
        output.push_str(&String::from_utf8_lossy(&tail));
        output
    }

    pub fn flush(&mut self) -> Result<(), FileError> {
        self.file.flush()
    }
}

pub struct Capture {
    pub stdout: Log,
    pub stderr: Log,
}

impl Capture {
    pub fn new(filesystem: &dyn FileSystem) -> Result<Self, FileError> {
        match filesystem.create_directory(".agent-output") {
            Ok(()) | Err(FileError::Exists) => (),
            Err(error) => return Err(error),
        }
        for sequence in 0u64.. {
            let directory = format!(".agent-output/{sequence}");
            match filesystem.create_directory(&directory) {
                Ok(()) => {
                    return Ok(Self {
                        stdout: Log::new(filesystem, format!("/{directory}/stdout"))?,
                        stderr: Log::new(filesystem, format!("/{directory}/stderr"))?,
                    });
                }
                Err(FileError::Exists) => (),
                Err(error) => return Err(error),
            }
        }
        unreachable!("Capture directory sequence exhausted")
    }

    pub fn append(&mut self, stdout: &[u8], stderr: &[u8]) -> Result<(), FileError> {
        self.stdout.append(stdout)?;
        self.stderr.append(stderr)
    }

    pub fn result(&self, status: &str) -> String {
        format!(
            "{status}\nstdout_path: {} ({} bytes)\nstderr_path: {} ({} bytes)\n\nStandard output:\n{}\n\nStandard error:\n{}",
            self.stdout.path,
            self.stdout.bytes,
            self.stderr.path,
            self.stderr.bytes,
            self.stdout.preview(),
            self.stderr.preview()
        )
    }

    pub fn flush(&mut self) -> Result<(), FileError> {
        self.stdout.flush()?;
        self.stderr.flush()
    }
}
