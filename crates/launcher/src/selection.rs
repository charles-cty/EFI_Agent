use efi_agent_core::serial::{SelectionView, VIEW_PREFIX};
use std::{
    collections::BTreeMap,
    fmt::Write as _,
    io::{self, Write},
};

struct TextRow {
    first: u16,
    end: u16,
    cells: Vec<vt100::Cell>,
}

// Input and transcript rows occupy separate coordinate domains.
const INPUT_BASE: usize = usize::MAX / 2;

/// Selection uses guest transcript coordinates, not terminal chrome coordinates.
pub struct Selection {
    parser: vt100::Parser,
    presented: vt100::Screen,
    pending: Vec<u8>,
    view: SelectionView,
    rows: BTreeMap<usize, TextRow>,
    anchor: Option<(usize, u16)>,
    end: Option<(usize, u16)>,
    pointer: Option<(u16, u16)>,
    dragging: bool,
}

impl Selection {
    pub fn new(width: u16, height: u16) -> Self {
        let parser = vt100::Parser::new(height.max(1), width.max(1), 0);
        let presented = parser.screen().clone();
        Self {
            parser,
            presented,
            pending: Vec::new(),
            view: SelectionView::default(),
            rows: BTreeMap::new(),
            anchor: None,
            end: None,
            pointer: None,
            dragging: false,
        }
    }

    /// A metadata marker commits a complete guest frame. Partial TCP reads
    /// never expose an unhighlighted intermediate frame to the host terminal.
    pub fn output(&mut self, bytes: &[u8]) -> io::Result<(bool, Vec<u8>)> {
        self.pending.extend_from_slice(bytes);
        if self.pending.len() > 1024 * 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Guest frame exceeds limit",
            ));
        }
        let mut changed = false;
        let mut committed = false;
        while let Some(start) = self
            .pending
            .windows(VIEW_PREFIX.len())
            .position(|w| w == VIEW_PREFIX)
        {
            let payload = start + VIEW_PREFIX.len();
            let Some(length) = self.pending[payload..].iter().position(|b| *b == 7) else {
                break;
            };
            let view: SelectionView =
                serde_json::from_slice(&self.pending[payload..payload + length])
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            // Size belongs to the committed guest frame. A host resize can
            // precede older frames already in flight; do not reject those.
            let (width, height) = view.dimensions;
            if width == 0 || height == 0 || width > 300 || height > 120 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid guest frame size",
                ));
            }
            if self.parser.screen().size() != (height, width) {
                changed |= self.anchor.is_some();
                self.reset();
                self.rows.clear();
                self.parser.screen_mut().set_size(height, width);
                self.presented.set_size(height, width);
            }
            self.validate(&view)?;
            self.parser.process(&self.pending[..start]);
            self.pending.drain(..payload + length + 1);
            let input_selection = self.anchor.is_some_and(|(row, _)| row >= INPUT_BASE);
            if (input_selection
                && (view.input_revision != self.view.input_revision
                    || view.input_rows != self.view.input_rows))
                || (!input_selection && view.revision != self.view.revision)
            {
                changed |= self.anchor.is_some();
                self.reset();
                self.rows.clear();
            }
            self.view = view;
            self.capture_rows();
            if self.dragging
                && let Some((x, y)) = self.pointer
            {
                self.end = self.position(x, y, true);
            }
            committed = true;
        }
        Ok((
            changed,
            if committed {
                self.rendered_diff()
            } else {
                Vec::new()
            },
        ))
    }

    fn validate(&self, view: &SelectionView) -> io::Result<()> {
        let (height, width) = self.parser.screen().size();
        if view.top.saturating_add(view.height) > height
            || view.offset > view.max_offset
            || view.rows.len() > usize::from(height)
            || view.rows.iter().any(|&(y, first, end)| {
                y < view.top || y >= view.top + view.height || first > end || end > width
            })
            || view.input_rows.len() > usize::from(height)
            || view.input_rows.iter().any(|&(y, first, end)| {
                y >= height || first > end || end > width || y < view.top + view.height
            })
            || view
                .input_cursor
                .is_some_and(|(x, y)| x >= width || y >= height)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid guest selection view",
            ));
        }
        Ok(())
    }

    fn capture_rows(&mut self) {
        if self.anchor.is_none() {
            self.rows.clear();
        }
        let screen = self.parser.screen();
        let (_, width) = screen.size();
        for (y, first, end, logical) in self.visible_rows() {
            self.rows.insert(
                logical,
                TextRow {
                    first,
                    end,
                    cells: (0..width)
                        .map(|x| screen.cell(y, x).unwrap().clone())
                        .collect(),
                },
            );
        }
    }

    fn position(&self, x: u16, y: u16, clamp: bool) -> Option<(usize, u16)> {
        let input_selection = self.anchor.is_some_and(|(row, _)| row >= INPUT_BASE);
        if (!clamp || input_selection) && !self.view.input_rows.is_empty() {
            let first = self.view.input_rows.first()?.0;
            let last = self.view.input_rows.last()?.0;
            if input_selection || (y >= first && y <= last) {
                return Some((
                    INPUT_BASE + usize::from(y.clamp(first, last)),
                    x.min(self.view.dimensions.0.saturating_sub(1)),
                ));
            }
        }
        if self.view.height == 0 {
            return None;
        }
        let row = if clamp {
            y.clamp(self.view.top, self.view.top + self.view.height - 1)
        } else if y >= self.view.top && y < self.view.top + self.view.height {
            y
        } else {
            return None;
        };
        // Body rows accept the whole row, including indentation and padding.
        // Empty transcript rows also accept an anchor. Non-body text is chrome.
        if !clamp && !self.view.rows.iter().any(|&(body, _, _)| body == row) {
            let screen = self.parser.screen();
            let (_, width) = screen.size();
            if (0..width).any(|col| {
                screen
                    .cell(row, col)
                    .is_some_and(|cell| !cell.contents().trim().is_empty())
            }) {
                return None;
            }
        }
        Some((
            self.view.offset + usize::from(row - self.view.top),
            x.min(self.view.dimensions.0.saturating_sub(1)),
        ))
    }

    pub fn begin(&mut self, x: u16, y: u16) -> io::Result<()> {
        self.reset();
        self.capture_rows();
        self.anchor = self.position(x, y, false);
        self.write_diff()
    }

    pub fn drag(&mut self, x: u16, y: u16) -> io::Result<()> {
        if self.anchor.is_some() {
            self.pointer = Some((x, y));
            self.dragging = true;
            self.end = self.position(x, y, true);
            self.write_diff()?;
        }
        Ok(())
    }

    pub fn release(&mut self) {
        self.dragging = false;
        self.pointer = None;
    }

    /// Repeat scrolling even if the mouse stops moving at the viewport edge.
    pub fn scroll_direction(&self) -> Option<u8> {
        if !self.dragging || self.anchor.is_some_and(|(row, _)| row >= INPUT_BASE) {
            return None;
        }
        let (_, y) = self.pointer?;
        if y <= self.view.top && self.view.offset > 0 {
            Some(64)
        } else if y >= self.view.top + self.view.height.saturating_sub(1)
            && self.view.offset < self.view.max_offset
        {
            Some(65)
        } else {
            None
        }
    }

    pub fn active(&self) -> bool {
        self.anchor.is_some() && self.end.is_some()
    }

    fn bounds(&self) -> Option<((usize, u16), (usize, u16))> {
        let anchor = self.anchor?;
        let end = self.end?;
        Some((anchor.min(end), anchor.max(end)))
    }

    fn range(&self, logical: usize, row: &TextRow) -> Option<(u16, u16)> {
        let ((top, left), (bottom, right)) = self.bounds()?;
        if logical < top || logical > bottom {
            return None;
        }
        let first = if logical == top {
            left.max(row.first)
        } else {
            row.first
        };
        let end = if logical == bottom {
            (right + 1).min(row.end)
        } else {
            row.end
        };
        (first <= end).then_some((first, end))
    }

    pub fn text(&self) -> String {
        let mut lines = Vec::new();
        for (&logical, row) in &self.rows {
            let Some((first, end)) = self.range(logical, row) else {
                continue;
            };
            let mut line = String::new();
            for col in first..end {
                if logical >= INPUT_BASE
                    && self.view.input_cursor == Some((col, (logical - INPUT_BASE) as u16))
                {
                    continue;
                }
                let cell = &row.cells[usize::from(col)];
                if cell.is_wide_continuation() {
                    if col == first && col > 0 {
                        line.push_str(row.cells[usize::from(col - 1)].contents());
                    }
                } else {
                    line.push_str(if cell.contents().is_empty() {
                        " "
                    } else {
                        cell.contents()
                    });
                }
            }
            lines.push(line);
        }
        lines.join("\n")
    }

    fn reset(&mut self) {
        self.anchor = None;
        self.end = None;
        self.pointer = None;
        self.dragging = false;
    }

    pub fn clear(&mut self) -> io::Result<()> {
        self.reset();
        self.write_diff()
    }

    pub fn resize(&mut self, width: u16, height: u16) -> io::Result<()> {
        self.clear()?;
        self.rows.clear();
        self.view = SelectionView::default();
        self.parser
            .screen_mut()
            .set_size(height.max(1), width.max(1));
        self.presented.set_size(height.max(1), width.max(1));
        Ok(())
    }

    fn rendered_diff(&mut self) -> Vec<u8> {
        let screen = self.parser.screen();
        let (height, width) = screen.size();
        let mut display = vt100::Parser::new(height, width, 0);
        display.process(&screen.contents_formatted());
        let mut overlay = String::new();
        for (y, _, _, logical) in self.visible_rows() {
            let Some(row) = self.rows.get(&logical) else {
                continue;
            };
            let Some((first, end)) = self.range(logical, row) else {
                continue;
            };
            for col in first..end {
                if logical >= INPUT_BASE && self.view.input_cursor == Some((col, y)) {
                    continue;
                }
                let col = if row.cells[usize::from(col)].is_wide_continuation() {
                    col.saturating_sub(1)
                } else {
                    col
                };
                let cell = screen.cell(y, col).unwrap();
                let _ = write!(
                    overlay,
                    "\x1b[{};{}H\x1b[0;7m{}",
                    y + 1,
                    col + 1,
                    if cell.contents().is_empty() {
                        " "
                    } else {
                        cell.contents()
                    }
                );
            }
        }
        if !overlay.is_empty() {
            let (row, col) = screen.cursor_position();
            let _ = write!(overlay, "\x1b[0m\x1b[{};{}H", row + 1, col + 1);
            display.process(overlay.as_bytes());
        }
        let diff = display.screen().contents_diff(&self.presented);
        self.presented = display.screen().clone();
        diff
    }

    fn write_diff(&mut self) -> io::Result<()> {
        let diff = self.rendered_diff();
        let mut out = io::stdout().lock();
        out.write_all(&diff)?;
        out.flush()
    }

    fn visible_rows(&self) -> Vec<(u16, u16, u16, usize)> {
        self.view
            .rows
            .iter()
            .map(|&(y, first, end)| {
                (
                    y,
                    first,
                    end,
                    self.view.offset + usize::from(y - self.view.top),
                )
            })
            .chain(
                self.view
                    .input_rows
                    .iter()
                    .map(|&(y, first, end)| (y, first, end, INPUT_BASE + usize::from(y))),
            )
            .collect()
    }
}

pub fn paste(writer: &mut impl Write, text: &str) -> io::Result<()> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let text: String = text
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect();
    writer.write_all(b"\x1b[200~")?;
    writer.write_all(text.as_bytes())?;
    writer.write_all(b"\x1b[201~")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(content: &[u8], view: &SelectionView) -> Vec<u8> {
        let mut wire = content.to_vec();
        wire.extend_from_slice(VIEW_PREFIX);
        wire.extend(serde_json::to_vec(view).unwrap());
        wire.push(7);
        wire
    }

    fn view(offset: usize) -> SelectionView {
        SelectionView {
            dimensions: (14, 4),
            revision: 731,
            top: 1,
            height: 2,
            offset,
            max_offset: 6,
            rows: vec![(1, 2, 6), (2, 2, 8)],
            ..SelectionView::default()
        }
    }

    #[test]
    fn split_frames_are_atomic_and_chrome_is_not_selectable() {
        let wire = frame(
            "\x1b[1;1HTITLE\x1b[2;3H中e\u{301}Z\x1b[3;3Hsecond\x1b[4;1HPROMPT".as_bytes(),
            &view(0),
        );
        for split in 1..wire.len() {
            let mut selection = Selection::new(14, 4);
            assert!(selection.output(&wire[..split]).unwrap().1.is_empty());
            selection.output(&wire[split..]).unwrap();
            assert_eq!(selection.position(0, 0, false), None);
            assert_eq!(selection.position(0, 3, false), None);
            assert_eq!(selection.position(0, 1, false), Some((0, 0)));
            selection.anchor = Some((1, 4));
            selection.end = Some((0, 3));
            assert_eq!(selection.text(), "中e\u{301}Z\nsec");
        }
    }

    #[test]
    fn input_selection_excludes_cursor_and_is_independent_of_transcript_scroll() {
        let mut selection = Selection::new(20, 7);
        let view = SelectionView {
            dimensions: (20, 7),
            revision: 1,
            top: 1,
            height: 2,
            max_offset: 9,
            input_revision: 2,
            input_rows: vec![(4, 2, 8), (5, 0, 4)],
            input_cursor: Some((4, 4)),
            ..SelectionView::default()
        };
        selection
            .output(&frame("\x1b[5;1H› ab▏c中\x1b[6;1Hnext".as_bytes(), &view))
            .unwrap();
        selection.anchor = selection.position(0, 4, false);
        selection.end = selection.position(19, 5, true);
        assert_eq!(selection.text(), "abc中\nnext");
        selection.dragging = true;
        selection.pointer = Some((19, 6));
        assert_eq!(selection.scroll_direction(), None);
        let mut scrolled = view.clone();
        scrolled.revision += 1;
        scrolled.offset = 3;
        assert!(!selection.output(&frame(b"", &scrolled)).unwrap().0);
        assert!(selection.active());
        scrolled.input_revision += 1;
        assert!(selection.output(&frame(b"", &scrolled)).unwrap().0);
        assert!(!selection.active());
    }

    #[test]
    fn blank_anchors_keep_coordinates_without_copying_padding_or_controls() {
        let mut selection = Selection::new(14, 4);
        selection
            .output(&frame(
                b"\x1b[1;1HTITLE\x1b[2;3Habcd\x1b[3;3Hsecond",
                &view(0),
            ))
            .unwrap();
        selection.anchor = selection.position(13, 1, false);
        selection.end = selection.position(3, 1, true);
        assert_eq!(selection.text(), "bcd");
        selection.anchor = selection.position(0, 1, false);
        selection.end = selection.position(3, 1, true);
        assert_eq!(selection.text(), "ab");
        selection.anchor = selection.position(13, 1, false);
        selection.end = selection.position(4, 2, true);
        assert_eq!(selection.text(), "sec");
        assert_eq!(selection.position(2, 0, false), None);
        selection
            .output(&frame(
                b"\x1b[2;1H\x1b[2K",
                &SelectionView {
                    rows: vec![(2, 2, 8)],
                    ..view(0)
                },
            ))
            .unwrap();
        assert_eq!(selection.position(8, 1, false), Some((0, 8)));
        selection.anchor = Some((0, 8));
        selection.end = Some((1, 4));
        assert_eq!(selection.text(), "sec");
    }

    #[test]
    fn scrolling_keeps_logical_selection_and_cached_offscreen_text() {
        let mut selection = Selection::new(14, 4);
        selection
            .output(&frame(b"\x1b[2;3Haaaa\x1b[3;3Hbbbbbb", &view(3)))
            .unwrap();
        selection.anchor = Some((4, 3));
        selection.end = Some((3, 2));
        selection.dragging = true;
        selection.pointer = Some((2, 1));
        assert_eq!(selection.scroll_direction(), Some(64));
        selection
            .output(&frame(b"\x1b[2;3Hcccc\x1b[3;3Hdddddd", &view(1)))
            .unwrap();
        assert_eq!(selection.text(), "cccc\ndddddd\naaaa\nbb");
        assert!(selection.active());
        selection.output(&frame(b"", &view(0))).unwrap();
        assert_eq!(selection.scroll_direction(), None);
        selection.pointer = Some((7, 3));
        assert_eq!(selection.scroll_direction(), Some(65));
        selection.output(&frame(b"", &view(6))).unwrap();
        assert_eq!(selection.scroll_direction(), None);
        selection.release();
        assert_eq!(selection.scroll_direction(), None);
    }

    #[test]
    fn resize_accepts_an_old_inflight_frame_before_the_new_guest_size() {
        let mut selection = Selection::new(14, 4);
        let old = frame(b"\x1b[2;3Habcd\x1b[3;3Hsecond", &view(0));
        let split = old.len() - 1;
        selection.output(&old[..split]).unwrap();
        selection.resize(8, 3).unwrap();
        selection.output(&old[split..]).unwrap();
        assert_eq!(selection.parser.screen().size(), (4, 14));
        let smaller = SelectionView {
            dimensions: (8, 3),
            revision: 2,
            top: 1,
            height: 1,
            offset: 0,
            max_offset: 0,
            rows: vec![(1, 2, 6)],
            ..SelectionView::default()
        };
        selection
            .output(&frame(b"\x1b[2J\x1b[2;3Htext", &smaller))
            .unwrap();
        assert_eq!(selection.parser.screen().size(), (3, 8));
        assert!(!selection.active());
    }

    #[test]
    fn copy_preserves_empty_body_rows_and_code_indentation() {
        let mut selection = Selection::new(20, 5);
        let view = SelectionView {
            dimensions: (20, 5),
            revision: 1,
            top: 1,
            height: 3,
            offset: 0,
            max_offset: 0,
            rows: vec![(1, 2, 10), (2, 2, 2), (3, 2, 6)],
            ..SelectionView::default()
        };
        selection
            .output(&frame(b"\x1b[2;3H    code\x1b[4;3Hnext", &view))
            .unwrap();
        selection.anchor = Some((0, 2));
        selection.end = Some((2, 5));
        assert_eq!(selection.text(), "    code\n\nnext");
        assert_eq!(selection.position(2, 2, true), Some((1, 2)));
    }

    #[test]
    fn overlay_diff_preserves_existing_highlight_and_never_erases_screen() {
        let mut selection = Selection::new(14, 4);
        let (_, diff) = selection
            .output(&frame(b"\x1b[2;3Habcd\x1b[3;3Hsecond", &view(0)))
            .unwrap();
        let mut host = vt100::Parser::new(4, 14, 0);
        host.process(&diff);
        selection.anchor = Some((0, 2));
        selection.end = Some((0, 3));
        host.process(&selection.rendered_diff());
        selection.end = Some((0, 5));
        let diff = selection.rendered_diff();
        assert!(!diff.windows(4).any(|w| w == b"\x1b[2J"));
        let mut trace = vt100::Parser::new(4, 14, 0);
        trace.process(&host.screen().contents_formatted());
        for byte in diff {
            trace.process(&[byte]);
            assert!(trace.screen().cell(1, 2).unwrap().inverse());
        }
        let idle = frame(b"\x1b[?25l", &view(0));
        assert!(!selection.output(&idle).unwrap().0);
        let mut changed = view(0);
        changed.revision += 1;
        assert!(selection.output(&frame(b"", &changed)).unwrap().0);
        assert!(!selection.active());
    }

    #[test]
    fn clipboard_paste_is_a_single_sanitized_prompt() {
        let mut bytes = Vec::new();
        paste(&mut bytes, "中\r\nnext\rline\t\x03\x1b").unwrap();
        assert_eq!(bytes, "\x1b[200~中\nnext\nline\t\x1b[201~".as_bytes());
    }
}
