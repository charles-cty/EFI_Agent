use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind},
    terminal,
};
use efi_agent_core::serial;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum InterruptSource {
    Signal,
    Keyboard,
}

enum InterruptAction {
    Ignore,
    Copy,
    Hint,
    Exit,
}

#[derive(Default)]
struct InterruptGuard {
    last: Option<(Instant, InterruptSource)>,
    armed: Option<Instant>,
}

impl InterruptGuard {
    fn press(&mut self, source: InterruptSource, selected: bool, now: Instant) -> InterruptAction {
        // Some Windows hosts deliver one press as both a console signal and
        // an input record. Count that pair once; same-source presses stay distinct.
        if self.last.is_some_and(|(time, previous)| {
            previous != source && now.duration_since(time) <= Duration::from_millis(100)
        }) {
            self.last = None;
            return InterruptAction::Ignore;
        }
        self.last = Some((now, source));
        if selected {
            self.armed = None;
            InterruptAction::Copy
        } else if self
            .armed
            .take()
            .is_some_and(|time| now.duration_since(time) <= Duration::from_secs(1))
        {
            InterruptAction::Exit
        } else {
            self.armed = Some(now);
            InterruptAction::Hint
        }
    }
}

struct Session {
    child: Child,
    raw: bool,
    disk: Option<crate::boot::Disk>,
    // X11 serves clipboard contents from their owner while this handle lives.
    clipboard: Option<crate::clipboard::Clipboard>,
}
impl Session {
    fn finish(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let _ = self.child.kill();
        let _ = self.child.wait();
        #[cfg(windows)]
        if self.raw {
            use crossterm::Command;
            let _ = event::DisableMouseCapture.execute_winapi();
        }
        if self.raw {
            let _ = terminal::disable_raw_mode();
        }
        if self.raw {
            print!("\x1b[?1000l\x1b[?1002l\x1b[?1006l\x1b[?2004l\x1b[0m\x1b[?25h\x1b[?1049l");
            let _ = std::io::stdout().flush();
        }
        self.raw = false;
        if let Some(mut disk) = self.disk.take() {
            if let Err(error) = disk.save() {
                disk.preserve();
                return Err(format!(
                    "Workspace save failed: {error}; recover files from {}",
                    disk.image.display()
                )
                .into());
            }
            disk.cleanup()?;
        }
        Ok(())
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        if let Err(error) = self.finish() {
            eprintln!("{error}");
        }
    }
}

pub fn run(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 5 && args.len() != 7 {
        return Err(
            "Usage: efi-agent vm <qemu> <OVMF_CODE.fd> <OVMF_VARS.fd> <EFI-file-or-ESP-directory-or-image> <workspace> [--memory-mib <MiB>]"
                .into(),
        );
    }
    let memory_mib = if args.len() == 7 {
        if args[5] != "--memory-mib" {
            return Err("Unknown VM option; use --memory-mib <MiB>".into());
        }
        let value = args[6]
            .parse::<u32>()
            .map_err(|_| "VM memory must be a positive integer in MiB")?;
        if value == 0 {
            return Err("VM memory must be greater than zero MiB".into());
        }
        value
    } else {
        128
    };
    let memory = memory_mib.to_string();
    crate::boot::configuration()?;
    let interrupted = Arc::new(AtomicBool::new(false));
    let selection_active = Arc::new(AtomicBool::new(false));
    let copy_requested = Arc::new(AtomicBool::new(false));
    let hint_requested = Arc::new(AtomicBool::new(false));
    let interrupt_guard = Arc::new(Mutex::new(InterruptGuard::default()));
    let signal = Arc::clone(&interrupted);
    let selected = Arc::clone(&selection_active);
    let copy_signal = Arc::clone(&copy_requested);
    let hint_signal = Arc::clone(&hint_requested);
    let guard_signal = Arc::clone(&interrupt_guard);
    ctrlc::set_handler(move || {
        if let Ok(mut guard) = guard_signal.lock() {
            match guard.press(
                InterruptSource::Signal,
                selected.load(Ordering::Relaxed),
                Instant::now(),
            ) {
                InterruptAction::Copy => copy_signal.store(true, Ordering::Relaxed),
                InterruptAction::Hint => hint_signal.store(true, Ordering::Relaxed),
                InterruptAction::Exit => signal.store(true, Ordering::Relaxed),
                InterruptAction::Ignore => {}
            }
        }
    })?;
    let firmware = PathBuf::from(&args[1]).canonicalize()?;
    let variables = PathBuf::from(&args[2]).canonicalize()?;
    let esp = PathBuf::from(&args[3]).canonicalize()?;
    let root = PathBuf::from(&args[4]).canonicalize()?;
    let disk = crate::boot::Disk::prepare(&esp, root)?;
    let boot_drive = qemu_path(&disk.image)?;
    // QEMU parses drive arguments itself. Extended Win32 prefixes and commas
    // must not pass through this comma-delimited option syntax unchanged.
    let firmware = qemu_path(&firmware)?;
    let variables = qemu_path(&variables)?;
    let console = TcpListener::bind("127.0.0.1:0")?;
    let console_port = console.local_addr()?.port();
    console.set_nonblocking(true)?;
    let accelerator = if cfg!(windows) { "whpx" } else { "kvm" };
    let mut child = Command::new(&args[0]);
    child
        .args([
            "-machine",
            &format!("q35,accel={accelerator}"),
            "-m",
            &memory,
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
            &format!("if=none,id=esp,format=raw,file={boot_drive}"),
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
            "user,id=network",
            "-device",
            "virtio-net-pci,netdev=network",
            "-device",
            "virtio-rng-pci",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    let mut session = Session {
        child: child.spawn()?,
        raw: false,
        disk: Some(disk),
        clipboard: None,
    };
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut socket: TcpStream = loop {
        if hint_requested.swap(false, Ordering::Relaxed) {
            eprintln!("Press Ctrl+C again within 1 second to exit");
        }
        if interrupted.load(Ordering::Relaxed) {
            return session.finish();
        }
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
    #[cfg(windows)]
    {
        use crossterm::Command;
        event::EnableMouseCapture.execute_winapi()?;
    }
    print!("\x1b[?1049h\x1b[?2004h\x1b[?1000h\x1b[?1002h\x1b[?1006h\x1b[2J\x1b[H");
    std::io::stdout().flush()?;
    let (width, height) = terminal::size()?;
    let mut selection = crate::selection::Selection::new(width, height);
    let mut mouse_down = None;
    let mut mouse_dragged = false;
    let mut scroll_tick = Instant::now();
    let mut clipboard_input = crate::paste::ClipboardInput::default();
    // Firmware consumes serial input before the application boots. Do not
    // send any terminal input until the guest identifies itself as ready.
    let mut ready = false;
    let mut dimensions = None;
    let mut size_check = Instant::now();
    let mut decoder = serial::Decoder::default();
    let boot_deadline = Instant::now() + Duration::from_secs(60);
    let mut buffer = [0; 8192];
    loop {
        if let Some(input) = clipboard_input.expired(Instant::now()) {
            match input {
                crate::paste::Input::Keys(bytes) => socket.write_all(&bytes)?,
                crate::paste::Input::Paste(text) => crate::selection::paste(&mut socket, &text)?,
                crate::paste::Input::Pending => {}
            }
        }
        if hint_requested.swap(false, Ordering::Relaxed) {
            clipboard_notice("Press Ctrl+C again within 1 second to exit")?;
        }
        if copy_requested.swap(false, Ordering::Relaxed) {
            copy_selection(&selection, &mut session.clipboard)?;
        }
        if interrupted.load(Ordering::Relaxed) {
            break;
        }
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
                    selection.resize(w, h)?;
                }
                let (changed, rendered) = selection.output(&output)?;
                selection_active.store(selection.active(), Ordering::Relaxed);
                if changed {
                    mouse_down = None;
                }
                std::io::stdout().write_all(&rendered)?;
                std::io::stdout().flush()?;
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::Interrupted
                ) => {}
            Err(e) if e.kind() == std::io::ErrorKind::ConnectionReset => {
                // Windows can reset the virtconsole socket while QEMU exits
                // after a firmware shutdown. Confirm a successful process exit
                // before treating that reset as normal terminal completion.
                let deadline = Instant::now() + Duration::from_secs(1);
                loop {
                    if let Some(status) = session.child.try_wait()? {
                        if status.success() {
                            return session.finish();
                        }
                        return Err(format!("QEMU exited: {status}").into());
                    }
                    if Instant::now() >= deadline {
                        return Err(e.into());
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            Err(e) => return Err(e.into()),
        }
        if ready && scroll_tick.elapsed() >= Duration::from_millis(90) {
            if let Some(button) = selection.scroll_direction() {
                socket.write_all(format!("\x1b[<{button};1;1M").as_bytes())?;
            }
            scroll_tick = Instant::now();
        }
        if !ready && Instant::now() > boot_deadline {
            return Err("UEFI application did not become ready within 60 seconds".into());
        }
        // A resize can happen before the platform event source is initialized,
        // and terminal hosts can coalesce notifications. Reconcile actual size.
        if ready && size_check.elapsed() >= Duration::from_millis(250) {
            let (w, h) = terminal::size()?;
            if dimensions != Some((w, h)) {
                selection.resize(w, h)?;
                selection_active.store(false, Ordering::Relaxed);
                mouse_down = None;
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
                Event::Mouse(mouse) if ready => {
                    match mouse.kind {
                        MouseEventKind::Down(MouseButton::Left) => {
                            selection.begin(mouse.column, mouse.row)?;
                            selection_active.store(false, Ordering::Relaxed);
                            mouse_down = Some((mouse.column, mouse.row));
                            mouse_dragged = false;
                        }
                        MouseEventKind::Drag(MouseButton::Left) if mouse_down.is_some() => {
                            mouse_dragged = true;
                            selection.drag(mouse.column, mouse.row)?;
                            selection_active.store(selection.active(), Ordering::Relaxed);
                        }
                        MouseEventKind::Up(MouseButton::Left) => {
                            if selection.active() {
                                selection.drag(mouse.column, mouse.row)?;
                            } else if !mouse_dragged && let Some((x, y)) = mouse_down {
                                socket.write_all(
                                    format!("\x1b[<0;{};{}M", u32::from(x) + 1, u32::from(y) + 1)
                                        .as_bytes(),
                                )?;
                            }
                            selection.release();
                            mouse_down = None;
                        }
                        MouseEventKind::Down(MouseButton::Right) => {
                            if selection.active() {
                                copy_selection(&selection, &mut session.clipboard)?;
                            } else {
                                paste_clipboard(&mut socket, &mut session.clipboard)?;
                            }
                        }
                        _ => {}
                    }
                    let button = match mouse.kind {
                        MouseEventKind::ScrollUp => Some(64),
                        MouseEventKind::ScrollDown => Some(65),
                        _ => None,
                    };
                    if let Some(button) = button {
                        socket.write_all(
                            format!(
                                "\x1b[<{button};{};{}M",
                                u32::from(mouse.column) + 1,
                                u32::from(mouse.row) + 1
                            )
                            .as_bytes(),
                        )?;
                    }
                }
                Event::Resize(w, h) if ready && dimensions != Some((w, h)) => {
                    selection.resize(w, h)?;
                    selection_active.store(false, Ordering::Relaxed);
                    mouse_down = None;
                    socket.write_all(format!("\x1b[8;{h};{w}t").as_bytes())?;
                    dimensions = Some((w, h));
                }
                Event::Key(key) if key.kind != KeyEventKind::Release => {
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && matches!(key.code, KeyCode::Char('c' | 'C'))
                        && !key.modifiers.contains(KeyModifiers::SHIFT)
                    {
                        if key.kind == KeyEventKind::Repeat {
                            continue;
                        }
                        let action = interrupt_guard
                            .lock()
                            .map_err(|_| "Interrupt state lock failed")?
                            .press(
                                InterruptSource::Keyboard,
                                selection.active(),
                                Instant::now(),
                            );
                        match action {
                            InterruptAction::Copy => {
                                copy_selection(&selection, &mut session.clipboard)?
                            }
                            InterruptAction::Hint => {
                                clipboard_notice("Press Ctrl+C again within 1 second to exit")?
                            }
                            InterruptAction::Exit => break,
                            InterruptAction::Ignore => {}
                        }
                        continue;
                    }
                    interrupt_guard
                        .lock()
                        .map_err(|_| "Interrupt state lock failed")?
                        .armed = None;
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        match key.code {
                            KeyCode::Char('c' | 'C') if selection.active() => {
                                copy_selection(&selection, &mut session.clipboard)?;
                                continue;
                            }
                            KeyCode::Char('c' | 'C')
                                if key.modifiers.contains(KeyModifiers::SHIFT) =>
                            {
                                continue;
                            }
                            KeyCode::Char('v' | 'V') if ready => {
                                selection.clear()?;
                                selection_active.store(false, Ordering::Relaxed);
                                paste_clipboard(&mut socket, &mut session.clipboard)?;
                                continue;
                            }
                            _ => {}
                        }
                    }
                    if selection.active() && key.code == KeyCode::Esc {
                        selection.clear()?;
                        selection_active.store(false, Ordering::Relaxed);
                        continue;
                    }
                    let bytes = match key.code {
                        KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            vec![10]
                        }
                        KeyCode::Char(c) => c.to_string().into_bytes(),
                        KeyCode::Enter
                            if key.modifiers.intersects(
                                KeyModifiers::ALT | KeyModifiers::SHIFT | KeyModifiers::CONTROL,
                            ) =>
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
                        KeyCode::Tab => vec![9],
                        _ => Vec::new(),
                    };
                    if ready {
                        let clipboard = if cfg!(windows)
                            && !clipboard_input.pending()
                            && !bytes.is_empty()
                            && !key.modifiers.contains(KeyModifiers::CONTROL)
                        {
                            session_clipboard(&mut session.clipboard)
                                .and_then(|clipboard| clipboard.get_text())
                                .ok()
                        } else {
                            None
                        };
                        match clipboard_input.push(&bytes, clipboard, Instant::now()) {
                            crate::paste::Input::Pending => {}
                            crate::paste::Input::Keys(bytes) => socket.write_all(&bytes)?,
                            crate::paste::Input::Paste(text) => {
                                selection.clear()?;
                                selection_active.store(false, Ordering::Relaxed);
                                crate::selection::paste(&mut socket, &text)?;
                            }
                        }
                    }
                }
                Event::Paste(text) if ready => {
                    selection.clear()?;
                    selection_active.store(false, Ordering::Relaxed);
                    crate::selection::paste(&mut socket, &text)?;
                }
                _ => {}
            }
        }
    }
    session.finish()
}

fn clipboard_notice(message: &str) -> std::io::Result<()> {
    let (_, height) = terminal::size()?;
    let mut out = std::io::stdout().lock();
    write!(out, "\x1b7\x1b[{};1H\x1b[0m\x1b[2K{message}\x1b8", height)?;
    out.flush()
}

fn session_clipboard(
    clipboard: &mut Option<crate::clipboard::Clipboard>,
) -> Result<&mut crate::clipboard::Clipboard, String> {
    if clipboard.is_none() {
        *clipboard = Some(crate::clipboard::Clipboard::new()?);
    }
    Ok(clipboard.as_mut().expect("Clipboard initialized"))
}

fn copy_selection(
    selection: &crate::selection::Selection,
    clipboard: &mut Option<crate::clipboard::Clipboard>,
) -> std::io::Result<()> {
    if !selection.active() {
        return Ok(());
    }
    let result =
        session_clipboard(clipboard).and_then(|clipboard| clipboard.set_text(selection.text()));
    clipboard_notice(if result.is_ok() {
        "Copied selection"
    } else {
        "Clipboard unavailable; selection retained"
    })
}

fn paste_clipboard(
    socket: &mut TcpStream,
    clipboard: &mut Option<crate::clipboard::Clipboard>,
) -> std::io::Result<()> {
    match session_clipboard(clipboard).and_then(|clipboard| clipboard.get_text()) {
        Ok(text) => crate::selection::paste(socket, &text),
        Err(_) => clipboard_notice("Clipboard has no readable text"),
    }
}

fn qemu_path(path: &std::path::Path) -> Result<String, Box<dyn std::error::Error>> {
    let text = path.to_str().ok_or("QEMU path must be valid UTF-8")?;
    let text = text.strip_prefix("\\\\?\\").unwrap_or(text);
    if text.contains(',') {
        return Err("QEMU firmware and ESP paths must not contain commas".into());
    }
    Ok(text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "Requires a desktop clipboard and temporarily changes its contents"]
    fn clipboard_contents_survive_copy_until_another_process_reads() {
        const CONTENTS: &str = "clipboard owner 中\nsecond line";
        let mut clipboard = None;
        if std::env::var_os("EFI_AGENT_CLIPBOARD_TEST_READER").is_some() {
            assert_eq!(
                session_clipboard(&mut clipboard)
                    .unwrap()
                    .get_text()
                    .unwrap(),
                CONTENTS
            );
            return;
        }
        let original = session_clipboard(&mut clipboard).unwrap().get_text().ok();
        session_clipboard(&mut clipboard)
            .unwrap()
            .set_text(CONTENTS)
            .unwrap();
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "vm::tests::clipboard_contents_survive_copy_until_another_process_reads",
                "--ignored",
            ])
            .env("EFI_AGENT_CLIPBOARD_TEST_READER", "1")
            .output()
            .unwrap();
        // Read through a different Wayland implementation as well as arboard.
        let wayland_output =
            std::env::var_os("EFI_AGENT_CLIPBOARD_TEST_WAYLAND_READER").map(|_| {
                Command::new("wl-paste")
                    .args(["--no-newline", "--type", "text/plain;charset=utf-8"])
                    .output()
                    .unwrap()
            });
        if let Some(original) = original {
            session_clipboard(&mut clipboard)
                .unwrap()
                .set_text(original)
                .unwrap();
        }
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if let Some(output) = wayland_output {
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(String::from_utf8(output.stdout).unwrap(), CONTENTS);
        }
    }

    #[test]
    fn exit_requires_two_presses_and_duplicate_delivery_counts_once() {
        let now = Instant::now();
        for first in [InterruptSource::Signal, InterruptSource::Keyboard] {
            let second = if first == InterruptSource::Signal {
                InterruptSource::Keyboard
            } else {
                InterruptSource::Signal
            };
            let mut guard = InterruptGuard::default();
            assert!(matches!(
                guard.press(first, false, now),
                InterruptAction::Hint
            ));
            assert!(matches!(
                guard.press(second, false, now + Duration::from_millis(20)),
                InterruptAction::Ignore
            ));
            assert!(matches!(
                guard.press(first, false, now + Duration::from_millis(300)),
                InterruptAction::Exit
            ));
        }
        for (delay, exit) in [(999, true), (1000, true), (1001, false)] {
            let mut guard = InterruptGuard::default();
            guard.press(InterruptSource::Keyboard, false, now);
            assert_eq!(
                matches!(
                    guard.press(
                        InterruptSource::Keyboard,
                        false,
                        now + Duration::from_millis(delay)
                    ),
                    InterruptAction::Exit
                ),
                exit
            );
        }
        let mut guard = InterruptGuard::default();
        guard.press(InterruptSource::Keyboard, false, now);
        assert!(matches!(
            guard.press(
                InterruptSource::Keyboard,
                true,
                now + Duration::from_millis(300)
            ),
            InterruptAction::Copy
        ));
        assert!(matches!(
            guard.press(
                InterruptSource::Keyboard,
                true,
                now + Duration::from_millis(500)
            ),
            InterruptAction::Copy
        ));
        assert!(matches!(
            guard.press(
                InterruptSource::Keyboard,
                false,
                now + Duration::from_millis(700)
            ),
            InterruptAction::Hint
        ));
    }
}
