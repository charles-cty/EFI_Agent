//! A bounded VT screen for guest editors and pagers.
use alloc::{format, string::String, vec::Vec};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Line,
};
use vte::{Params, Parser, Perform};

pub struct TerminalScreen {
    parser: Parser,
    surface: Surface,
}

struct Surface {
    buffer: Buffer,
    x: u16,
    y: u16,
    saved: (u16, u16),
    top: u16,
    bottom: u16,
    style: Style,
    visible: bool,
    wrap: bool,
    pending_wrap: bool,
    replies: String,
}

impl TerminalScreen {
    pub fn new(columns: u16, rows: u16) -> Self {
        let columns = columns.clamp(1, 300);
        let rows = rows.clamp(1, 120);
        Self {
            parser: Parser::new(),
            surface: Surface {
                buffer: Buffer::empty(Rect::new(0, 0, columns, rows)),
                x: 0,
                y: 0,
                saved: (0, 0),
                top: 0,
                bottom: rows - 1,
                style: Style::default(),
                visible: true,
                wrap: true,
                pending_wrap: false,
                replies: String::new(),
            },
        }
    }

    pub fn resize(&mut self, columns: u16, rows: u16) {
        let columns = columns.clamp(1, 300);
        let rows = rows.clamp(1, 120);
        if self.surface.buffer.area == Rect::new(0, 0, columns, rows) {
            return;
        }
        let old = &self.surface.buffer;
        let mut buffer = Buffer::empty(Rect::new(0, 0, columns, rows));
        for y in 0..rows.min(old.area.height) {
            for x in 0..columns.min(old.area.width) {
                buffer[(x, y)] = old[(x, y)].clone();
            }
        }
        self.surface.buffer = buffer;
        self.surface.x = self.surface.x.min(columns - 1);
        self.surface.y = self.surface.y.min(rows - 1);
        self.surface.top = 0;
        self.surface.bottom = rows - 1;
        self.surface.pending_wrap = false;
    }

    pub fn output(&mut self, bytes: &[u8]) -> String {
        self.parser.advance(&mut self.surface, bytes);
        core::mem::take(&mut self.surface.replies)
    }

    pub fn render(&self, area: Rect, buffer: &mut Buffer) -> Option<(u16, u16)> {
        for y in 0..area.height.min(self.surface.buffer.area.height) {
            for x in 0..area.width.min(self.surface.buffer.area.width) {
                buffer[(area.x + x, area.y + y)] = self.surface.buffer[(x, y)].clone();
            }
        }
        if self.surface.visible && self.surface.x < area.width && self.surface.y < area.height {
            let position = (area.x + self.surface.x, area.y + self.surface.y);
            buffer[position].set_style(Style::default().add_modifier(Modifier::REVERSED));
            Some(position)
        } else {
            None
        }
    }
}

impl Surface {
    fn scroll(&mut self, count: u16, down: bool) {
        let width = usize::from(self.buffer.area.width);
        let count = count.min(self.bottom - self.top + 1);
        let start = usize::from(self.top) * width;
        let end = usize::from(self.bottom + 1) * width;
        let cells = &mut self.buffer.content[start..end];
        let amount = usize::from(count) * width;
        if down {
            cells.rotate_right(amount);
        } else {
            cells.rotate_left(amount);
        }
        let length = cells.len();
        let blank = if down {
            &mut cells[..amount]
        } else {
            &mut cells[length - amount..]
        };
        for cell in blank {
            cell.reset();
        }
    }

    fn newline(&mut self) {
        if self.y == self.bottom {
            self.scroll(1, false);
        } else {
            self.y = (self.y + 1).min(self.buffer.area.height - 1);
        }
        self.pending_wrap = false;
    }

    fn erase(&mut self, first: usize, end: usize) {
        for cell in &mut self.buffer.content[first..end] {
            cell.reset();
        }
    }

    fn color(index: u16) -> Color {
        [
            Color::Black,
            Color::Red,
            Color::Green,
            Color::Yellow,
            Color::Blue,
            Color::Magenta,
            Color::Cyan,
            Color::Gray,
        ][usize::from(index.min(7))]
    }
}

impl Perform for Surface {
    fn print(&mut self, character: char) {
        let mut bytes = [0; 4];
        let text = character.encode_utf8(&mut bytes);
        let width = Line::from(&*text).width() as u16;
        if width == 0 {
            if self.x > 0 {
                let cell = &mut self.buffer[(self.x - 1, self.y)];
                let mut symbol = String::from(cell.symbol());
                symbol.push(character);
                cell.set_symbol(&symbol);
            }
            return;
        }
        if self.wrap && (self.pending_wrap || self.x + width > self.buffer.area.width) {
            self.x = 0;
            self.newline();
        }
        self.buffer
            .set_stringn(self.x, self.y, text, width as usize, self.style);
        let next = self.x + width;
        self.pending_wrap = next >= self.buffer.area.width;
        self.x = next.min(self.buffer.area.width - 1);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' | 11 | 12 => self.newline(),
            b'\r' => {
                self.x = 0;
                self.pending_wrap = false;
            }
            8 => {
                self.x = self.x.saturating_sub(1);
                self.pending_wrap = false;
            }
            b'\t' => {
                self.x = ((self.x / 8 + 1) * 8).min(self.buffer.area.width - 1);
                self.pending_wrap = false;
            }
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
        if ignore {
            return;
        }
        let values: Vec<u16> = params.iter().map(|value| value[0]).collect();
        let value = values.first().copied().unwrap_or(0);
        let count = value.max(1);
        let width = self.buffer.area.width;
        let height = self.buffer.area.height;
        let offset = usize::from(self.y) * usize::from(width) + usize::from(self.x);
        let length = self.buffer.content.len();
        if matches!(
            action,
            'A' | 'B' | 'C' | 'D' | 'E' | 'F' | 'G' | '`' | 'd' | 'H' | 'f' | 'r' | 'u'
        ) {
            self.pending_wrap = false;
        }
        match action {
            'A' => self.y = self.y.saturating_sub(count),
            'B' | 'e' => self.y = self.y.saturating_add(count).min(height - 1),
            'C' | 'a' => self.x = self.x.saturating_add(count).min(width - 1),
            'D' => self.x = self.x.saturating_sub(count),
            'E' => {
                self.y = self.y.saturating_add(count).min(height - 1);
                self.x = 0;
            }
            'F' => {
                self.y = self.y.saturating_sub(count);
                self.x = 0;
            }
            'G' | '`' => self.x = count.saturating_sub(1).min(width - 1),
            'd' => self.y = count.saturating_sub(1).min(height - 1),
            'H' | 'f' => {
                self.y = count.saturating_sub(1).min(height - 1);
                self.x = values
                    .get(1)
                    .copied()
                    .unwrap_or(1)
                    .max(1)
                    .saturating_sub(1)
                    .min(width - 1);
            }
            'J' => match value {
                0 => self.erase(offset, length),
                1 => self.erase(0, offset + 1),
                2 | 3 => self.erase(0, length),
                _ => {}
            },
            'K' => {
                let start = usize::from(self.y) * usize::from(width);
                match value {
                    0 => self.erase(offset, start + usize::from(width)),
                    1 => self.erase(start, offset + 1),
                    2 => self.erase(start, start + usize::from(width)),
                    _ => {}
                }
            }
            'X' => self.erase(
                offset,
                (offset + usize::from(count)).min(usize::from(self.y + 1) * usize::from(width)),
            ),
            'S' => self.scroll(count, false),
            'T' => self.scroll(count, true),
            'L' | 'M' if self.y >= self.top && self.y <= self.bottom => {
                let old = self.top;
                self.top = self.y;
                self.scroll(count, action == 'L');
                self.top = old;
            }
            'P' | '@' => {
                let end = usize::from(self.y + 1) * usize::from(width);
                let cells = &mut self.buffer.content[offset..end];
                let amount = usize::from(count).min(cells.len());
                let length = cells.len();
                if action == 'P' {
                    cells.rotate_left(amount);
                    for cell in &mut cells[length - amount..] {
                        cell.reset();
                    }
                } else {
                    cells.rotate_right(amount);
                    for cell in &mut cells[..amount] {
                        cell.reset();
                    }
                }
            }
            'r' if intermediates.is_empty() => {
                let top = count.saturating_sub(1);
                let bottom = values
                    .get(1)
                    .copied()
                    .unwrap_or(height)
                    .max(1)
                    .saturating_sub(1);
                if top < bottom && bottom < height {
                    self.top = top;
                    self.bottom = bottom;
                    self.x = 0;
                    self.y = 0;
                }
            }
            's' => self.saved = (self.x, self.y),
            'u' => {
                self.x = self.saved.0.min(width - 1);
                self.y = self.saved.1.min(height - 1);
            }
            'n' if value == 6 => {
                self.replies
                    .push_str(&format!("\x1b[{};{}R", self.y + 1, self.x + 1))
            }
            'h' | 'l' if intermediates == b"?" => {
                for value in values {
                    match value {
                        7 => self.wrap = action == 'h',
                        25 => self.visible = action == 'h',
                        _ => {}
                    }
                }
            }
            'm' => {
                for value in values {
                    self.style = match value {
                        0 => Style::default(),
                        1 => self.style.add_modifier(Modifier::BOLD),
                        7 => self.style.add_modifier(Modifier::REVERSED),
                        22 => self.style.remove_modifier(Modifier::BOLD),
                        27 => self.style.remove_modifier(Modifier::REVERSED),
                        30..=37 => self.style.fg(Self::color(value - 30)),
                        40..=47 => self.style.bg(Self::color(value - 40)),
                        39 => self.style.fg(Color::Reset),
                        49 => self.style.bg(Color::Reset),
                        _ => self.style,
                    };
                }
            }
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], ignore: bool, byte: u8) {
        if ignore {
            return;
        }
        self.pending_wrap = false;
        match byte {
            b'7' => self.saved = (self.x, self.y),
            b'8' => {
                self.x = self.saved.0.min(self.buffer.area.width - 1);
                self.y = self.saved.1.min(self.buffer.area.height - 1);
            }
            b'D' => self.newline(),
            b'E' => {
                self.x = 0;
                self.newline();
            }
            b'M' => {
                if self.y == self.top {
                    self.scroll(1, true);
                } else {
                    self.y = self.y.saturating_sub(1);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_terminal_operations_match_an_independent_emulator() {
        let mut actual = TerminalScreen::new(13, 7);
        let mut expected = vt100::Parser::new(7, 13, 0);
        let mut state = 431_u32;
        for _ in 0..600 {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let sequence = match state % 12 {
                0 => format!("\x1b[{};{}H", (state >> 8) % 7 + 1, (state >> 16) % 13 + 1),
                1 => String::from("abcdef"),
                2 => String::from("\r\n"),
                3 => format!("\x1b[{}K", (state >> 8) % 3),
                4 => format!("\x1b[{}J", (state >> 8) % 3),
                5 => String::from("\x1b[2P"),
                6 => String::from("\x1b[3@"),
                7 => String::from("\x1b[1S"),
                8 => String::from("\x1b[1T"),
                9 => String::from("\x1b[1A"),
                10 => String::from("\x1b[2D"),
                _ => String::from("\x1b[2X"),
            };
            // Deliberately split every escape sequence across writes.
            for byte in sequence.bytes() {
                actual.output(&[byte]);
                expected.process(&[byte]);
            }
            for y in 0..7 {
                for x in 0..13 {
                    let reference = expected.screen().cell(y, x).unwrap().contents();
                    assert_eq!(
                        actual.surface.buffer[(x, y)].symbol().trim_end(),
                        reference.trim_end(),
                        "after {sequence:?}, cell ({x},{y})"
                    );
                }
            }
        }
    }

    #[test]
    fn scroll_region_preserves_status_rows_and_resize_keeps_coordinates() {
        let mut screen = TerminalScreen::new(9, 5);
        screen.output(b"TOP\x1b[5;1HSTATUS\x1b[2;4r\x1b[2;1HA\r\nB\r\nC\r\nD");
        assert_eq!(screen.surface.buffer[(0, 0)].symbol(), "T");
        assert_eq!(screen.surface.buffer[(0, 1)].symbol(), "B");
        assert_eq!(screen.surface.buffer[(0, 3)].symbol(), "D");
        assert_eq!(screen.surface.buffer[(0, 4)].symbol(), "S");
        screen.resize(13, 7);
        assert_eq!(screen.surface.buffer[(0, 4)].symbol(), "S");
        assert_eq!(screen.output(b"\x1b[7;13H\x1b[6n"), "\x1b[7;13R");
    }
}
