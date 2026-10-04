use crate::{input::Key, protocol::Message};
use alloc::{format, string::String, vec, vec::Vec};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};

pub struct App {
    pub input: String,
    pub messages: Vec<Message>,
    pub status: String,
    pub workspace: String,
    pub scroll: u16,
    pub quit: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            input: String::new(),
            messages: Vec::new(),
            status: String::from("Ready"),
            workspace: String::from("UEFI workspace"),
            scroll: 0,
            quit: false,
        }
    }
}

impl App {
    pub fn key(&mut self, key: Key) -> Option<String> {
        match key {
            Key::Character(c) => self.input.push(c),
            Key::Backspace => {
                self.input.pop();
            }
            Key::Up => self.scroll = self.scroll.saturating_sub(3),
            Key::Down => self.scroll = self.scroll.saturating_add(3),
            Key::Quit => self.quit = true,
            Key::Enter if !self.input.trim().is_empty() => {
                self.scroll = 0;
                return Some(core::mem::take(&mut self.input));
            }
            _ => {}
        }
        None
    }

    pub fn message(&mut self, role: &str, content: String) {
        self.messages.push(Message {
            role: role.into(),
            content,
        });
    }

    pub fn render(&self, area: Rect, buffer: &mut Buffer) {
        buffer.set_style(area, Style::default().fg(Color::White).bg(Color::Black));
        let rows = Layout::vertical([
            Constraint::Length(2),
            Constraint::Min(3),
            Constraint::Length(3),
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
            let color = if message.role == "user" {
                Color::Cyan
            } else {
                Color::White
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
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll, 0))
            .render(rows[1], buffer);
        Paragraph::new(format!("› {}", self.input))
            .block(
                Block::default()
                    .borders(Borders::TOP | Borders::BOTTOM)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .render(rows[2], buffer);
        Paragraph::new(format!(" {}  •  Enter send  •  ↑↓ scroll", self.status))
            .style(Style::default().fg(Color::DarkGray))
            .render(rows[3], buffer);
    }
}
