use crate::bridge;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    terminal,
};
use efi_agent_core::serial;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Session {
    child: Child,
    raw: bool,
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if self.raw {
            let _ = terminal::disable_raw_mode();
        }
        if self.raw {
            print!("\x1b[?2004l\x1b[0m\x1b[?25h\x1b[?1049l");
            let _ = std::io::stdout().flush();
        }
    }
}

pub fn run(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 5 {
        return Err(
            "Usage: efi-agent vm <qemu> <OVMF_CODE.fd> <OVMF_VARS.fd> <ESP-directory> <workspace>"
                .into(),
        );
    }
    let firmware = PathBuf::from(&args[1]).canonicalize()?;
    let variables = PathBuf::from(&args[2]).canonicalize()?;
    let esp = PathBuf::from(&args[3]).canonicalize()?;
    let root = PathBuf::from(&args[4]).canonicalize()?;
    // QEMU parses drive arguments itself. Extended Win32 prefixes and commas
    // must not pass through this comma-delimited option syntax unchanged.
    let firmware = qemu_path(&firmware)?;
    let esp = qemu_path(&esp)?;
    let variables = qemu_path(&variables)?;
    let console = TcpListener::bind("127.0.0.1:0")?;
    let console_port = console.local_addr()?.port();
    console.set_nonblocking(true)?;
    let rpc = TcpListener::bind("127.0.0.1:0")?;
    let rpc_port = rpc.local_addr()?.port();
    let _service = bridge::serve(rpc, root);
    let accelerator = if cfg!(windows) { "whpx" } else { "kvm" };
    let mut child = Command::new(&args[0]);
    child
        .args([
            "-machine",
            &format!("q35,accel={accelerator}"),
            "-m",
            "256",
            "-display",
            "none",
            "-monitor",
            "none",
            "-serial",
            "none",
            "-no-reboot",
        ])
        .args([
            "-drive",
            &format!("if=pflash,format=raw,readonly=on,file={}", firmware),
        ])
        .args([
            "-drive",
            &format!("if=pflash,format=raw,snapshot=on,file={variables}"),
        ])
        .args([
            "-drive",
            &format!("if=none,id=esp,format=raw,readonly=on,file=fat:ro:{esp}"),
        ])
        .args(["-device", "virtio-blk-pci,drive=esp"])
        .args([
            "-device",
            "virtio-serial-pci",
            "-chardev",
            &format!("socket,id=terminal,host=127.0.0.1,port={console_port}"),
            "-device",
            "virtconsole,chardev=terminal",
        ])
        .args([
            "-netdev",
            &format!("user,id=network,guestfwd=tcp:10.0.2.100:7420-tcp:127.0.0.1:{rpc_port}"),
            "-device",
            "virtio-net-pci,netdev=network",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    let mut session = Session {
        child: child.spawn()?,
        raw: false,
    };
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut socket: TcpStream = loop {
        if let Some(status) = session.child.try_wait()? {
            return Err(format!("QEMU exited: {status}").into());
        }
        match console.accept() {
            Ok((socket, _)) => break socket,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e.into()),
        }
        if Instant::now() > deadline {
            return Err("QEMU terminal connection timed out".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    socket.set_read_timeout(Some(Duration::from_millis(5)))?;
    socket.set_write_timeout(Some(Duration::from_secs(2)))?;
    terminal::enable_raw_mode()?;
    session.raw = true;
    print!("\x1b[?1049h\x1b[?2004h\x1b[2J\x1b[H");
    std::io::stdout().flush()?;
    // Firmware consumes serial input before the application boots. Do not
    // send any terminal input until the guest identifies itself as ready.
    let mut ready = false;
    let mut dimensions = None;
    let mut size_check = Instant::now();
    let mut decoder = serial::Decoder::default();
    let boot_deadline = Instant::now() + Duration::from_secs(60);
    let mut buffer = [0; 8192];
    loop {
        if session.child.try_wait()?.is_some() {
            break;
        }
        match socket.read(&mut buffer) {
            Ok(0) => {
                std::io::stdout().write_all(&decoder.finish())?;
                break;
            }
            Ok(count) => {
                let (output, notifications) = decoder.push(&buffer[..count]);
                if notifications > 0 {
                    ready = true;
                    let (w, h) = terminal::size()?;
                    socket.write_all(format!("\x1b[8;{h};{w}t").as_bytes())?;
                    dimensions = Some((w, h));
                }
                std::io::stdout().write_all(&output)?;
                std::io::stdout().flush()?;
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::Interrupted
                ) => {}
            Err(e) => return Err(e.into()),
        }
        if !ready && Instant::now() > boot_deadline {
            return Err("UEFI application did not become ready within 60 seconds".into());
        }
        // A resize can happen before the platform event source is initialized,
        // and terminal hosts can coalesce notifications. Reconcile actual size.
        if ready && size_check.elapsed() >= Duration::from_millis(250) {
            let (w, h) = terminal::size()?;
            if dimensions != Some((w, h)) {
                socket.write_all(format!("\x1b[8;{h};{w}t").as_bytes())?;
                dimensions = Some((w, h));
            }
            size_check = Instant::now();
        }
        let available = match event::poll(Duration::from_millis(5)) {
            Ok(available) => available,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.into()),
        };
        if available {
            let input = match event::read() {
                Ok(input) => input,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            };
            match input {
                Event::Resize(w, h) if ready => {
                    socket.write_all(format!("\x1b[8;{h};{w}t").as_bytes())?;
                    dimensions = Some((w, h));
                }
                Event::Key(key) if key.kind != KeyEventKind::Release => {
                    let bytes = match key.code {
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            vec![3]
                        }
                        KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            vec![10]
                        }
                        KeyCode::Char(c) => c.to_string().into_bytes(),
                        KeyCode::Enter
                            if key
                                .modifiers
                                .intersects(KeyModifiers::ALT | KeyModifiers::SHIFT) =>
                        {
                            vec![10]
                        }
                        KeyCode::Enter => vec![13],
                        KeyCode::Backspace => vec![127],
                        KeyCode::Esc => vec![27],
                        KeyCode::Up => b"\x1b[A".to_vec(),
                        KeyCode::Down => b"\x1b[B".to_vec(),
                        KeyCode::Left => b"\x1b[D".to_vec(),
                        KeyCode::Right => b"\x1b[C".to_vec(),
                        KeyCode::Home => b"\x1b[H".to_vec(),
                        KeyCode::End => b"\x1b[F".to_vec(),
                        KeyCode::Delete => b"\x1b[3~".to_vec(),
                        _ => Vec::new(),
                    };
                    if ready {
                        socket.write_all(&bytes)?;
                    }
                    // Ctrl+C is also an immediate escape hatch if the guest is stuck.
                    if bytes == [3] {
                        break;
                    }
                }
                Event::Paste(text) if ready => {
                    // Filter terminal controls. Preserve only printable text,
                    // line breaks and tabs; pasted escapes must never become
                    // resize, quit, or editor control sequences in the guest.
                    let text = text.replace("\r\n", "\n").replace('\r', "\n");
                    let text: String = text
                        .chars()
                        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
                        .collect();
                    socket.write_all(b"\x1b[200~")?;
                    socket.write_all(text.as_bytes())?;
                    socket.write_all(b"\x1b[201~")?;
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn qemu_path(path: &std::path::Path) -> Result<String, Box<dyn std::error::Error>> {
    let text = path.to_str().ok_or("QEMU path must be valid UTF-8")?;
    let text = text.strip_prefix("\\\\?\\").unwrap_or(text);
    if text.contains(',') {
        return Err("QEMU firmware and ESP paths must not contain commas".into());
    }
    Ok(text.to_owned())
}
