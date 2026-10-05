use alloc::{format, string::String};
use core::fmt::Write;
use ratatui::{
    backend::{Backend, ClearType, WindowSize},
    buffer::Cell,
    layout::{Position, Size},
    style::{Color, Modifier},
};

pub trait Sink {
    type Error: core::error::Error;
    fn write(&mut self, bytes: &[u8]) -> Result<(), Self::Error>;
}

pub struct Ansi<S> {
    pub sink: S,
    pub dimensions: Size,
    cursor: Position,
}

impl<S: Sink> Ansi<S> {
    pub fn new(sink: S, dimensions: Size) -> Self {
        Self {
            sink,
            dimensions,
            cursor: Position::ORIGIN,
        }
    }
}

fn color(output: &mut String, color: Color, foreground: bool) {
    let prefix = if foreground { 38 } else { 48 };
    match color {
        Color::Reset => {
            let _ = write!(output, "\x1b[{}m", if foreground { 39 } else { 49 });
        }
        Color::Rgb(r, g, b) => {
            let _ = write!(output, "\x1b[{prefix};2;{r};{g};{b}m");
        }
        value => {
            let index = match value {
                Color::Black => 0,
                Color::Red => 1,
                Color::Green => 2,
                Color::Yellow => 3,
                Color::Blue => 4,
                Color::Magenta => 5,
                Color::Cyan => 6,
                Color::Gray => 7,
                Color::DarkGray => 8,
                Color::LightRed => 9,
                Color::LightGreen => 10,
                Color::LightYellow => 11,
                Color::LightBlue => 12,
                Color::LightMagenta => 13,
                Color::LightCyan => 14,
                Color::White => 15,
                Color::Indexed(i) => i,
                _ => 7,
            };
            let _ = write!(output, "\x1b[{prefix};5;{index}m");
        }
    }
}

impl<S: Sink> Backend for Ansi<S> {
    type Error = S::Error;
    fn draw<'a, I>(&mut self, content: I) -> Result<(), Self::Error>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        let mut output = String::new();
        for (x, y, cell) in content {
            let _ = write!(output, "\x1b[{};{}H\x1b[0m", y + 1, x + 1);
            color(&mut output, cell.fg, true);
            color(&mut output, cell.bg, false);
            if cell.modifier.contains(Modifier::BOLD) {
                output.push_str("\x1b[1m");
            }
            if cell.modifier.contains(Modifier::UNDERLINED) {
                output.push_str("\x1b[4m");
            }
            if cell.modifier.contains(Modifier::REVERSED) {
                output.push_str("\x1b[7m");
            }
            output.push_str(cell.symbol());
            if output.len() >= 4096 {
                self.sink.write(output.as_bytes())?;
                output.clear();
            }
        }
        self.sink.write(output.as_bytes())
    }
    fn hide_cursor(&mut self) -> Result<(), Self::Error> {
        self.sink.write(b"\x1b[?25l")
    }
    fn show_cursor(&mut self) -> Result<(), Self::Error> {
        self.sink.write(b"\x1b[?25h")
    }
    fn get_cursor_position(&mut self) -> Result<Position, Self::Error> {
        Ok(self.cursor)
    }
    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> Result<(), Self::Error> {
        self.cursor = position.into();
        self.sink
            .write(format!("\x1b[{};{}H", self.cursor.y + 1, self.cursor.x + 1).as_bytes())
    }
    fn clear(&mut self) -> Result<(), Self::Error> {
        self.sink.write(b"\x1b[2J\x1b[H")
    }
    fn clear_region(&mut self, kind: ClearType) -> Result<(), Self::Error> {
        self.sink.write(match kind {
            ClearType::All => b"\x1b[2J",
            ClearType::AfterCursor => b"\x1b[0J",
            ClearType::BeforeCursor => b"\x1b[1J",
            ClearType::CurrentLine => b"\x1b[2K",
            ClearType::UntilNewLine => b"\x1b[0K",
        })
    }
    fn size(&self) -> Result<Size, Self::Error> {
        Ok(self.dimensions)
    }
    fn window_size(&mut self) -> Result<WindowSize, Self::Error> {
        Ok(WindowSize {
            columns_rows: self.dimensions,
            pixels: Size::ZERO,
        })
    }
    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}
