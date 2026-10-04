use crate::bridge;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    terminal,
};
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
            print!("\x1b[0m\x1b[?25h\x1b[?1049l");
            let _ = std::io::stdout().flush();
        }
    }
}

pub fn run(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 4 {
        return Err("Usage: efi-agent vm <qemu> <OVMF_CODE.fd> <ESP-directory> <workspace>".into());
    }
    let firmware = PathBuf::from(&args[1]).canonicalize()?;
    let esp = PathBuf::from(&args[2]).canonicalize()?;
    let root = PathBuf::from(&args[3]).canonicalize()?;
    // QEMU parses drive arguments itself. Extended Win32 prefixes and commas
    // must not pass through this comma-delimited option syntax unchanged.
    let firmware = qemu_path(&firmware)?;
    let esp = qemu_path(&esp)?;
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
        .args(["-drive", &format!("format=raw,file=fat:ro:{esp}")])
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
    print!("\x1b[?1049h\x1b[2J\x1b[H");
    std::io::stdout().flush()?;
    let (w, h) = terminal::size()?;
    socket.write_all(format!("\x1b[8;{h};{w}t").as_bytes())?;
    let mut buffer = [0; 8192];
    loop {
        if session.child.try_wait()?.is_some() {
            break;
        }
        match socket.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                std::io::stdout().write_all(&buffer[..count])?;
                std::io::stdout().flush()?;
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(e) => return Err(e.into()),
        }
        if event::poll(Duration::from_millis(5))? {
            match event::read()? {
                Event::Resize(w, h) => socket.write_all(format!("\x1b[8;{h};{w}t").as_bytes())?,
                Event::Key(key) if key.kind != KeyEventKind::Release => {
                    let bytes = match key.code {
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            vec![3]
                        }
                        KeyCode::Char(c) => c.to_string().into_bytes(),
                        KeyCode::Enter => vec![13],
                        KeyCode::Backspace => vec![127],
                        KeyCode::Esc => vec![27],
                        KeyCode::Up => b"\x1b[A".to_vec(),
                        KeyCode::Down => b"\x1b[B".to_vec(),
                        _ => Vec::new(),
                    };
                    socket.write_all(&bytes)?;
                    // Ctrl+C is also an immediate escape hatch if the guest is stuck.
                    if bytes == [3] {
                        break;
                    }
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
