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
        }
    }
}

impl App {
    pub fn key(&mut self, key: Key) -> Option<String> {
        match key {
            Key::Character(c) => self.editor.insert(c),
            Key::Newline => self.editor.insert('\n'),
            Key::Backspace => self.editor.backspace(),
            Key::Delete => self.editor.delete(),
            Key::Left => self.editor.left(),
            Key::Right => self.editor.right(),
            Key::Home => self.editor.home(),
            Key::End => self.editor.end(),
            Key::Up => self.scroll = self.scroll.saturating_add(3),
            Key::Down => self.scroll = self.scroll.saturating_sub(3),
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
        });
    }

    pub fn render(&self, area: Rect, buffer: &mut Buffer) {
        buffer.set_style(area, Style::default().fg(Color::White).bg(Color::Black));
        let before = &self.editor.text[..self.editor.cursor()];
        let after = &self.editor.text[self.editor.cursor()..];
        let mut prompt = Vec::new();
        for line in before.split('\n') {
            prompt.push(Line::from(format!("› {line}")));
        }
        if let Some(line) = prompt.last_mut() {
            line.spans
                .push(Span::styled("▏", Style::default().fg(Color::Cyan)));
            let mut remaining = after.split('\n');
            line.spans
                .push(Span::raw(remaining.next().unwrap_or_default()));
            prompt.extend(remaining.map(|line| Line::from(format!("› {line}"))));
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
        for message in &self.messages {
            let color = match message.role.as_str() {
                "user" => Color::Cyan,
                "tool" => Color::Yellow,
                "error" => Color::LightRed,
                _ => Color::White,
            };
            lines.push(Line::styled(
                format!("  {}", message.role),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ));
            lines.extend(
                message
                    .content
                    .lines()
                    .map(|line| Line::from(format!("  {line}"))),
            );
            lines.push(Line::from(""));
        }
        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        let bottom = paragraph
            .line_count(rows[1].width)
            .saturating_sub(rows[1].height.into())
            .min(u16::MAX as usize) as u16;
        paragraph
            .scroll((bottom.saturating_sub(self.scroll), 0))
            .render(rows[1], buffer);
        let cursor_prefix = before
            .split('\n')
            .map(|line| format!("› {line}"))
            .collect::<Vec<_>>()
            .join("\n");
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
        Paragraph::new(format!(
            " {}  •  Enter send  •  Esc cancel  •  Ctrl+J newline  •  ↑↓ scroll",
            self.status
        ))
        .style(Style::default().fg(Color::DarkGray))
        .render(rows[3], buffer);
    }
}
