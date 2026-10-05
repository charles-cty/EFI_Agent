use crate::{editor::Editor, input::Key, protocol::Message};
use alloc::{format, string::String, vec, vec::Vec};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};

pub struct App {
    pub editor: Editor,
    pub messages: Vec<Message>,
    pub status: String,
    pub workspace: String,
    pub capabilities: String,
    pub scroll: u16,
    pub quit: bool,
    pub selection_view: crate::serial::SelectionView,
    disclosure_hits: Vec<(Rect, usize)>,
    focused: Option<usize>,
    disclosure_anchor: Option<(usize, u16)>,
    pointer_position: Option<(u16, u16)>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            editor: Editor::default(),
            messages: Vec::new(),
            status: String::from("Ready"),
            workspace: String::from("UEFI workspace"),
            capabilities: String::new(),
            scroll: 0,
            quit: false,
            selection_view: crate::serial::SelectionView::default(),
            disclosure_hits: Vec::new(),
            focused: None,
            disclosure_anchor: None,
            pointer_position: None,
        }
    }
}

impl App {
    pub fn key(&mut self, key: Key) -> Option<String> {
        match key {
            Key::PointerMove(x, y) => self.pointer_position = Some((x, y)),
            Key::Click(x, y) => {
                if let Some((_, index)) = self
                    .disclosure_hits
                    .iter()
                    .find(|(rect, _)| rect.contains((x, y).into()))
                {
                    let index = *index;
                    self.toggle(index);
                }
            }
            Key::Tab => {
                let next = self
                    .disclosure_hits
                    .iter()
                    .find(|(_, index)| self.focused.is_none_or(|focused| *index > focused))
                    .or_else(|| self.disclosure_hits.first());
                self.focused = next.map(|(_, index)| *index);
            }
            Key::Enter if self.editor.text.is_empty() && self.focused.is_some() => {
                self.toggle(self.focused.unwrap());
            }
            Key::Character(c) => self.editor.insert(c),
            Key::Newline => self.editor.insert('\n'),
            Key::Backspace => self.editor.backspace(),
            Key::Delete => self.editor.delete(),
            Key::Left => self.editor.left(),
            Key::Right => self.editor.right(),
            Key::Home => self.editor.home(),
            Key::End => self.editor.end(),
            Key::Up if !self.editor.text.is_empty() => self.editor.vertical(false),
            Key::Down if !self.editor.text.is_empty() => self.editor.vertical(true),
            Key::Up | Key::ScrollUp => self.scroll = self.scroll.saturating_add(3),
            Key::Down | Key::ScrollDown => self.scroll = self.scroll.saturating_sub(3),
            Key::Quit => self.quit = true,
            Key::Enter if !self.editor.text.trim().is_empty() => {
                self.scroll = 0;
                return Some(self.editor.take());
            }
            _ => {}
        }
        None
    }

    pub fn message(&mut self, role: &str, content: String) {
        self.scroll = 0;
        self.messages.push(Message {
            role: role.into(),
            content,
            title: role.into(),
            collapsible: false,
            expanded: true,
        });
    }

    pub fn detail(&mut self, role: &str, title: String, content: String) {
        self.message(role, content);
        let message = self.messages.last_mut().expect("New detail message");
        message.title = title;
        message.collapsible = true;
        message.expanded = false;
    }

    pub fn clear_messages(&mut self) {
        self.messages.clear();
        self.disclosure_hits.clear();
        self.focused = None;
        self.disclosure_anchor = None;
        self.scroll = 0;
    }

    pub fn start_tool(&mut self, name: &str, arguments: &str) -> usize {
        self.detail(
            "tool_call",
            format!("Tool: {name} (running)"),
            format!("Arguments:\n{arguments}"),
        );
        self.messages.len() - 1
    }

    pub fn finish_tool(&mut self, index: usize, name: &str, result: &str, failed: bool) {
        let message = &mut self.messages[index];
        message.role = String::from(if failed { "tool_error" } else { "tool_result" });
        message.title = format!("Tool: {name} ({})", if failed { "failed" } else { "done" });
        message.content.push_str("\n\nResult:\n");
        message.content.push_str(result);
    }

    fn toggle(&mut self, index: usize) {
        self.disclosure_anchor = self
            .disclosure_hits
            .iter()
            .find(|(_, item)| *item == index)
            .map(|(rect, _)| (index, rect.y));
        if let Some(message) = self
            .messages
            .get_mut(index)
            .filter(|message| message.collapsible)
        {
            message.expanded = !message.expanded;
            self.focused = Some(index);
        }
    }

    pub fn render(&mut self, area: Rect, buffer: &mut Buffer) {
        self.disclosure_hits.clear();
        buffer.set_style(area, Style::default().fg(Color::White).bg(Color::Black));
        let before = &self.editor.text[..self.editor.cursor()];
        let after = &self.editor.text[self.editor.cursor()..];
        let mut prompt = Vec::new();
        for (index, line) in before.split('\n').enumerate() {
            prompt.push(Line::from(if index == 0 {
                format!("› {line}")
            } else {
                String::from(line)
            }));
        }
        if let Some(line) = prompt.last_mut() {
            line.spans
                .push(Span::styled("▏", Style::default().fg(Color::Cyan)));
            let mut remaining = after.split('\n');
            line.spans
                .push(Span::raw(remaining.next().unwrap_or_default()));
            prompt.extend(remaining.map(|line| Line::from(String::from(line))));
        }
        let prompt = Paragraph::new(prompt).wrap(Wrap { trim: false });
        let prompt_height = prompt.line_count(area.width).clamp(1, 6) as u16;
        let rows = Layout::vertical([
            Constraint::Length(2),
            Constraint::Min(3),
            Constraint::Length(prompt_height + 2),
            Constraint::Length(1),
        ])
        .split(area);
        Paragraph::new(Line::from(vec![
            Span::styled(
                " ✳ EFI AGENT ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {}", self.workspace),
                Style::default().fg(Color::DarkGray),
            ),
        ]))
        .render(rows[0], buffer);
        let mut lines = Vec::new();
        if self.messages.is_empty() {
            lines.push(Line::styled(
                "  What would you like to build?",
                Style::default().add_modifier(Modifier::BOLD),
            ));
            lines.push(Line::from(""));
            lines.push(Line::styled(
                "  Read, inspect, and edit your workspace with an AI assistant.",
                Style::default().fg(Color::DarkGray),
            ));
            lines.push(Line::from("  /help for commands · Ctrl+C to exit"));
        }
        let mut headers = Vec::new();
        let mut bodies = Vec::new();
        let mut line_offset = Paragraph::new(lines.clone())
            .wrap(Wrap { trim: false })
            .line_count(rows[1].width);
        for (index, message) in self.messages.iter().enumerate() {
            let color = match message.role.as_str() {
                "user" => Color::Cyan,
                "tool_call" => Color::Yellow,
                "tool_result" => Color::Green,
                "tool_error" => Color::LightRed,
                "reasoning" | "reasoning_summary" => Color::Magenta,
                "error" => Color::LightRed,
                _ => Color::White,
            };
            let title = if message.collapsible {
                format!(
                    "  [{}] {}",
                    if message.expanded { "-" } else { "+" },
                    message.title
                )
            } else {
                format!("  {}", message.title)
            };
            let mut style = Style::default().fg(color).add_modifier(Modifier::BOLD);
            if self.focused == Some(index) {
                style = style.add_modifier(Modifier::UNDERLINED);
            }
            let header = Line::styled(title, style);
            let header_height = Paragraph::new(header.clone())
                .wrap(Wrap { trim: false })
                .line_count(rows[1].width);
            if message.collapsible {
                headers.push((line_offset, header_height, index));
            }
            let mut block = vec![header];
            if !message.collapsible || message.expanded {
                let mut body_start = line_offset + header_height;
                for line in message.content.lines() {
                    let line = Line::from(format!("  {line}"));
                    let height = Paragraph::new(line.clone())
                        .wrap(Wrap { trim: false })
                        .line_count(rows[1].width);
                    bodies.push((body_start, height));
                    body_start += height;
                    block.push(line);
                }
            }
            block.push(Line::from(""));
            line_offset += Paragraph::new(block.clone())
                .wrap(Wrap { trim: false })
                .line_count(rows[1].width);
            lines.extend(block);
        }
        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        let bottom = paragraph
            .line_count(rows[1].width)
            .saturating_sub(rows[1].height.into())
            .min(u16::MAX as usize) as u16;
        if let Some((index, y)) = self.disclosure_anchor.take()
            && let Some((start, _, _)) = headers.iter().find(|(_, _, item)| *item == index)
        {
            let target = start
                .saturating_sub(usize::from(y.saturating_sub(rows[1].y)))
                .min(usize::from(bottom)) as u16;
            self.scroll = bottom.saturating_sub(target);
        }
        let offset = bottom.saturating_sub(self.scroll);
        for (start, height, index) in headers {
            let end = (start + height).min(usize::from(offset) + usize::from(rows[1].height));
            let start = start.max(usize::from(offset));
            if end > start {
                self.disclosure_hits.push((
                    Rect::new(
                        rows[1].x,
                        rows[1].y + (start - usize::from(offset)) as u16,
                        rows[1].width,
                        (end - start) as u16,
                    ),
                    index,
                ));
            }
        }
        paragraph.scroll((offset, 0)).render(rows[1], buffer);
        // Only message bodies are selectable. Headers, disclosures, the
        // welcome screen, prompt, footer and padding are interface controls.
        let mut revision = 0xcbf29ce484222325_u64;
        for message in &self.messages {
            for byte in message
                .content
                .as_bytes()
                .iter()
                .copied()
                .chain([u8::from(message.expanded), 0xff])
            {
                revision = (revision ^ u64::from(byte)).wrapping_mul(0x100000001b3);
            }
            for byte in message.title.as_bytes() {
                revision = (revision ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
            }
        }
        revision ^= u64::from(rows[1].width);
        let mut selectable = Vec::new();
        for (start, height) in bodies {
            for logical in start.max(usize::from(offset))
                ..(start + height).min(usize::from(offset) + usize::from(rows[1].height))
            {
                let y = rows[1].y + (logical - usize::from(offset)) as u16;
                let first = (rows[1].x + if logical == start { 2 } else { 0 }).min(rows[1].right());
                let mut end = first;
                for x in first..rows[1].right() {
                    if !buffer[(x, y)].symbol().trim().is_empty() {
                        end = x + 1;
                    }
                }
                if end > first {
                    // Include the continuation cell of a final wide glyph.
                    let symbol = buffer[(end - 1, y)].symbol();
                    let width = Line::from(symbol).width().min(2) as u16;
                    selectable.push((
                        y,
                        first,
                        (end + width.saturating_sub(1)).min(rows[1].right()),
                    ));
                } else {
                    selectable.push((y, first, first));
                }
            }
        }
        self.selection_view = crate::serial::SelectionView {
            dimensions: (area.right(), area.bottom()),
            revision,
            top: rows[1].y,
            height: rows[1].height,
            offset: usize::from(offset),
            max_offset: usize::from(bottom),
            rows: selectable,
            ..crate::serial::SelectionView::default()
        };
        let cursor_prefix = format!("› {before}");
        let cursor_lines = Paragraph::new(format!("{cursor_prefix}▏"))
            .wrap(Wrap { trim: false })
            .line_count(rows[2].width);
        prompt
            .scroll((
                cursor_lines
                    .saturating_sub(prompt_height as usize)
                    .min(u16::MAX as usize) as u16,
                0,
            ))
            .block(
                Block::default()
                    .borders(Borders::TOP | Borders::BOTTOM)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .render(rows[2], buffer);
        let mut input_revision = 0xcbf29ce484222325_u64;
        for byte in self.editor.text.as_bytes() {
            input_revision = (input_revision ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
        }
        input_revision ^= self.editor.cursor() as u64;
        self.selection_view.input_revision = input_revision;
        for y in rows[2].y + 1..rows[2].bottom().saturating_sub(1) {
            let first = rows[2].x
                + if y == rows[2].y + 1 && cursor_lines <= usize::from(prompt_height) {
                    2
                } else {
                    0
                };
            let mut end = first;
            for x in first..rows[2].right() {
                let cell = &buffer[(x, y)];
                if cell.symbol() == "▏" && cell.fg == Color::Cyan {
                    self.selection_view.input_cursor = Some((x, y));
                } else if !cell.symbol().trim().is_empty() {
                    let width = Line::from(cell.symbol()).width().min(2) as u16;
                    end = (x + width).min(rows[2].right());
                }
            }
            self.selection_view.input_rows.push((y, first, end));
        }
        Paragraph::new(format!(
            " {}  •  Enter send  •  Esc cancel  •  Ctrl+J newline  •  ↑↓ edit/empty: scroll  •  Click/Tab+Enter details",
            self.status
        ))
        .style(Style::default().fg(Color::DarkGray))
        .render(rows[3], buffer);
        if let Some((x, y)) = self
            .pointer_position
            .filter(|position| area.contains((*position).into()))
        {
            buffer[(x, y)].set_style(Style::default().add_modifier(Modifier::REVERSED));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draw(app: &mut App, area: Rect) -> String {
        let mut buffer = Buffer::empty(area);
        app.render(area, &mut buffer);
        buffer.content().iter().map(|cell| cell.symbol()).collect()
    }

    #[test]
    fn selectable_body_preserves_code_indentation_and_empty_lines() {
        let mut app = App::default();
        app.message("assistant", "    code 中\n\nnext".into());
        let area = Rect::new(0, 0, 30, 15);
        let mut buffer = Buffer::empty(area);
        app.render(area, &mut buffer);
        let texts: Vec<String> = app
            .selection_view
            .rows
            .iter()
            .map(|&(y, first, end)| {
                (first..end)
                    .filter(|&x| x == first || buffer[(x, y)].symbol() != " ")
                    .map(|x| buffer[(x, y)].symbol())
                    .collect()
            })
            .collect();
        // Check ranges directly; buffer wide continuations are padding cells.
        let (y, first, end) = app.selection_view.rows[0];
        assert_eq!(first, 2);
        assert_eq!(buffer[(first + 4, y)].symbol(), "c");
        assert_eq!(end, 13);
        let (_, first, end) = app.selection_view.rows[1];
        assert_eq!(first, end);
        assert_eq!(texts[2], "next");
    }

    #[test]
    fn selection_coordinates_exclude_controls_and_follow_wrapped_body_rows() {
        let mut app = App::default();
        let area = Rect::new(4, 3, 26, 15);
        app.message(
            "assistant",
            "FIRST 中\nSECOND\nTHIRD\nFOURTH\nFIFTH\nSIXTH\nSEVENTH\nEIGHTH".into(),
        );
        app.detail("reasoning", "Control header".into(), "HIDDEN".into());
        app.editor.insert('x');
        let mut buffer = Buffer::empty(area);
        app.render(area, &mut buffer);
        let revision = app.selection_view.revision;
        let before = app.selection_view.offset;
        for &(y, first, end) in &app.selection_view.rows {
            let text: String = (first..end).map(|x| buffer[(x, y)].symbol()).collect();
            assert!(
                !text.contains("Control") && !text.contains("assistant") && !text.contains('›')
            );
            assert!(
                y >= app.selection_view.top
                    && y < app.selection_view.top + app.selection_view.height
            );
        }
        app.key(Key::ScrollUp);
        app.render(area, &mut buffer);
        assert_eq!(app.selection_view.revision, revision);
        assert!(app.selection_view.offset < before);
        app.key(Key::Tab);
        app.render(area, &mut buffer);
        assert_eq!(app.selection_view.revision, revision);
        app.messages[0].content.push_str("\nCHANGED");
        app.render(area, &mut buffer);
        assert_ne!(app.selection_view.revision, revision);
        let mut empty = App::default();
        empty.render(area, &mut buffer);
        assert!(empty.selection_view.rows.is_empty());
    }

    #[test]
    fn tool_result_updates_its_call_and_preserves_disclosure_state() {
        let mut app = App::default();
        let first = app.start_tool("read", "FIRST_ARGUMENTS");
        let area = Rect::new(0, 0, 83, 30);
        assert!(!draw(&mut app, area).contains("FIRST_ARGUMENTS"));
        app.toggle(first);
        assert!(draw(&mut app, area).contains("FIRST_ARGUMENTS"));
        app.finish_tool(first, "read", "FIRST_RESULT", false);
        assert_eq!(app.messages.len(), 1);
        let visible = draw(&mut app, area);
        assert!(visible.contains("FIRST_ARGUMENTS") && visible.contains("FIRST_RESULT"));
        assert!(visible.contains("Tool: read (done)"));
        app.toggle(first);
        let second = app.start_tool("read", "SECOND_ARGUMENTS");
        app.finish_tool(second, "read", "SECOND_ERROR", true);
        let visible = draw(&mut app, area);
        assert!(!visible.contains("FIRST_ARGUMENTS") && !visible.contains("FIRST_RESULT"));
        assert!(!visible.contains("SECOND_ARGUMENTS") && !visible.contains("SECOND_ERROR"));
        assert_eq!(app.messages.len(), 2);
        assert!(app.messages[first].content.contains("FIRST_RESULT"));
        app.toggle(second);
        let visible = draw(&mut app, area);
        assert!(visible.contains("SECOND_ARGUMENTS") && visible.contains("SECOND_ERROR"));
        assert!(visible.contains("Tool: read (failed)"));
    }

    #[test]
    fn disclosures_follow_wrapping_resize_and_toggle_without_changing_content() {
        let mut app = App::default();
        app.message(
            "assistant",
            "Above 中 message that wraps across rows".into(),
        );
        app.detail(
            "reasoning",
            "Reasoning (provider text)".into(),
            "HIDDEN_REASON_731\n中 detail".into(),
        );
        let tool = app.start_tool("read", "{}");
        app.finish_tool(tool, "read", "HIDDEN_RESULT_419", false);
        let area = Rect::new(3, 5, 26, 24);
        assert!(!draw(&mut app, area).contains("HIDDEN_REASON"));
        let (header, _) = app
            .disclosure_hits
            .iter()
            .find(|(_, index)| *index == 1)
            .copied()
            .unwrap();
        app.key(Key::Click(header.x + 2, header.y + header.height - 1));
        assert!(draw(&mut app, area).contains("HIDDEN_REASON_731"));
        let (header, _) = app
            .disclosure_hits
            .iter()
            .find(|(_, index)| *index == 1)
            .copied()
            .unwrap();
        app.key(Key::Click(header.x + 2, header.y));
        assert!(!draw(&mut app, area).contains("HIDDEN_REASON_731"));
        app.scroll = 3;
        draw(&mut app, Rect::new(0, 0, 83, 19));
        app.key(Key::Tab);
        app.key(Key::Enter);
        assert!(
            app.messages
                .iter()
                .any(|message| message.collapsible && message.expanded)
        );
        app.clear_messages();
        app.key(Key::Click(5, 7));
        assert!(app.messages.is_empty());
    }
}
