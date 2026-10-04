use alloc::string::String;
use core::fmt;
use efi_agent_core::{ansi::Sink, input::Key};
use ratatui::{
    backend::{Backend, ClearType, WindowSize},
    buffer::Cell,
    layout::{Position, Size},
};
use uefi::{
    boot::ScopedProtocol,
    proto::console::{
        serial::Serial,
        text::{Color, Key as FirmwareKey, ScanCode},
    },
    system,
};

#[derive(Debug)]
pub struct Error(pub uefi::Status);
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "UEFI terminal: {:?}", self.0)
    }
}
impl core::error::Error for Error {}

pub struct SerialSink(pub ScopedProtocol<Serial>);
impl SerialSink {
    fn raw(&mut self) -> *mut uefi_raw::protocol::console::serial::SerialIoProtocol {
        // Serial is repr(transparent) over SerialIoProtocol in uefi-rs.
        (&mut *self.0 as *mut Serial).cast()
    }

    pub fn read(&mut self, bytes: &mut [u8]) -> Result<usize, Error> {
        let protocol = self.raw();
        let mut count = bytes.len();
        // SAFETY: protocol is exclusively open and bytes is a writable buffer.
        let status = unsafe { ((*protocol).read)(protocol, &mut count, bytes.as_mut_ptr()) };
        if count > bytes.len() {
            return Err(Error(uefi::Status::BAD_BUFFER_SIZE));
        }
        match status {
            uefi::Status::SUCCESS | uefi::Status::TIMEOUT => Ok(count),
            other => Err(Error(other)),
        }
    }
}
impl Sink for SerialSink {
    type Error = Error;
    fn write(&mut self, bytes: &[u8]) -> Result<(), Error> {
        // OVMF VirtioSerialDxe returns successful short writes (128-byte TX
        // queue), unlike the all-or-timeout assumption in uefi-rs write_exact.
        // Honor the firmware's returned count and keep every byte in order.
        let mut remaining = bytes;
        let mut no_progress = 0;
        while !remaining.is_empty() {
            let protocol = self.raw();
            let mut count = remaining.len();
            // SAFETY: the firmware only reads this live slice for this call.
            let status = unsafe { ((*protocol).write)(protocol, &mut count, remaining.as_ptr()) };
            if count > remaining.len() {
                return Err(Error(uefi::Status::BAD_BUFFER_SIZE));
            }
            if status != uefi::Status::SUCCESS && status != uefi::Status::TIMEOUT {
                return Err(Error(status));
            }
            remaining = &remaining[count..];
            if count == 0 {
                no_progress += 1;
                if no_progress >= 1000 {
                    return Err(Error(uefi::Status::TIMEOUT));
                }
                uefi::boot::stall(core::time::Duration::from_millis(1));
            } else {
                no_progress = 0;
            }
        }
        Ok(())
    }
}

pub struct SimpleText;
impl Backend for SimpleText {
    type Error = Error;
    fn draw<'a, I>(&mut self, mut content: I) -> Result<(), Error>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        system::with_stdout(|out| {
            let mode = out
                .current_mode()
                .map_err(|e| Error(e.status()))?
                .ok_or(Error(uefi::Status::UNSUPPORTED))?;
            for (x, y, cell) in content.by_ref() {
                // Writing the bottom-right cell scrolls some firmware consoles.
                if usize::from(x) + 1 == mode.columns() && usize::from(y) + 1 == mode.rows() {
                    continue;
                }
                out.set_cursor_position(x.into(), y.into())
                    .map_err(|e| Error(e.status()))?;
                let fg = match cell.fg {
                    ratatui::style::Color::Cyan => Color::LightCyan,
                    ratatui::style::Color::DarkGray => Color::LightGray,
                    _ => Color::White,
                };
                out.set_color(fg, Color::Black)
                    .map_err(|e| Error(e.status()))?;
                // SimpleText is UCS-2. Substitute characters outside its repertoire.
                let text: String = cell
                    .symbol()
                    .chars()
                    .map(|c| if c as u32 > 0xffff { '?' } else { c })
                    .collect();
                fmt::Write::write_str(out, &text).map_err(|_| Error(uefi::Status::DEVICE_ERROR))?;
            }
            Ok(())
        })
    }
    fn hide_cursor(&mut self) -> Result<(), Error> {
        system::with_stdout(|s| s.enable_cursor(false)).map_err(|e| Error(e.status()))
    }
    fn show_cursor(&mut self) -> Result<(), Error> {
        system::with_stdout(|s| s.enable_cursor(true)).map_err(|e| Error(e.status()))
    }
    fn get_cursor_position(&mut self) -> Result<Position, Error> {
        Ok(Position::ORIGIN)
    }
    fn set_cursor_position<P: Into<Position>>(&mut self, p: P) -> Result<(), Error> {
        let p = p.into();
        system::with_stdout(|s| s.set_cursor_position(p.x.into(), p.y.into()))
            .map_err(|e| Error(e.status()))
    }
    fn clear(&mut self) -> Result<(), Error> {
        system::with_stdout(|s| s.clear()).map_err(|e| Error(e.status()))
    }
    fn clear_region(&mut self, _: ClearType) -> Result<(), Error> {
        self.clear()
    }
    fn size(&self) -> Result<Size, Error> {
        system::with_stdout(|s| {
            let mode = s
                .current_mode()
                .map_err(|e| Error(e.status()))?
                .ok_or(Error(uefi::Status::UNSUPPORTED))?;
            Ok(Size::new(mode.columns() as u16, mode.rows() as u16))
        })
    }
    fn window_size(&mut self) -> Result<WindowSize, Error> {
        Ok(WindowSize {
            columns_rows: self.size()?,
            pixels: Size::ZERO,
        })
    }
    fn flush(&mut self) -> Result<(), Error> {
        Ok(())
    }
}

pub fn console_key() -> Option<Key> {
    match system::with_stdin(|s| s.read_key()).ok().flatten()? {
        FirmwareKey::Printable(c) => match u16::from(c) {
            3 => Some(Key::Quit),
            13 => Some(Key::Enter),
            8 => Some(Key::Backspace),
            _ => Some(Key::Character(char::from(c))),
        },
        FirmwareKey::Special(code) => match code {
            ScanCode::UP => Some(Key::Up),
            ScanCode::DOWN => Some(Key::Down),
            ScanCode::ESCAPE => Some(Key::Quit),
            _ => None,
        },
    }
}
