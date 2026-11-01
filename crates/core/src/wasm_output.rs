//! A single disclosure for a running command and its streamed output.
use crate::ui::App;
use alloc::{format, string::String};

pub struct WasmOutput {
    index: usize,
    generation: u64,
    command: String,
    pid: u32,
    stdout: String,
    stderr: String,
    status: String,
    output_limit: usize,
    stdout_truncated: bool,
    stderr_truncated: bool,
}

impl WasmOutput {
    pub fn new(app: &mut App, pid: u32, command: String) -> Self {
        app.detail("wasm", String::new(), String::new());
        Self {
            index: app.messages.len() - 1,
            generation: app.transcript_generation,
            command: command.chars().take(4096).collect(),
            pid,
            stdout: String::new(),
            stderr: String::new(),
            status: String::new(),
            output_limit: (app.history_limit / 128).clamp(1024, 64 * 1024),
            stdout_truncated: false,
            stderr_truncated: false,
        }
    }

    pub fn update(&mut self, app: &mut App, stdout: &str, stderr: &str, status: &str) -> bool {
        if stdout.is_empty() && stderr.is_empty() && self.status == status {
            return false;
        }
        self.status = String::from(status);
        // Recreate the disclosure after /clear instead of editing a reused row.
        if self.generation != app.transcript_generation {
            self.stdout.clear();
            self.stderr.clear();
            self.stdout_truncated = false;
            self.stderr_truncated = false;
            app.detail("wasm", String::new(), String::new());
            self.index = app.messages.len() - 1;
            self.generation = app.transcript_generation;
        }
        Self::append_stream(
            &mut self.stdout,
            stdout,
            self.output_limit,
            &mut self.stdout_truncated,
        );
        Self::append_stream(
            &mut self.stderr,
            stderr,
            self.output_limit,
            &mut self.stderr_truncated,
        );
        let message = &mut app.messages[self.index];
        let mut label: String = self.command.chars().take(48).collect();
        if label.len() < self.command.len() {
            label.push('…');
        }
        message.title = format!("WASM: {label} — PID {}: {status}", self.pid);
        message.content = format!("Command: {}", self.command);
        if !self.stdout.is_empty() {
            message.content.push_str("\n\nStandard output:\n");
            message.content.push_str(&self.stdout);
        }
        if self.stdout_truncated {
            message
                .content
                .push_str("\n[Output prefix retained; later stdout omitted]");
        }
        if !self.stderr.is_empty() {
            message.content.push_str("\n\nStandard error:\n");
            message.content.push_str(&self.stderr);
        }
        if self.stderr_truncated {
            message
                .content
                .push_str("\n[Output prefix retained; later stderr omitted]");
        }
        message
            .content
            .push_str(&format!("\n\nPID {}: {status}", self.pid));
        true
    }

    fn append_stream(target: &mut String, chunk: &str, limit: usize, truncated: &mut bool) {
        let available = limit.saturating_sub(target.len());
        let mut end = chunk.len().min(available);
        while !chunk.is_char_boundary(end) {
            end -= 1;
        }
        target.push_str(&chunk[..end]);
        *truncated |= end < chunk.len();
    }

    pub fn status(&self) -> &str {
        &self.status
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Key;
    use ratatui::{buffer::Buffer, layout::Rect};

    fn draw(app: &mut App) -> String {
        let area = Rect::new(0, 0, 100, 30);
        let mut buffer = Buffer::empty(area);
        app.render(area, &mut buffer);
        buffer.content().iter().map(|cell| cell.symbol()).collect()
    }

    #[test]
    fn streamed_output_is_one_collapsed_readable_disclosure_and_keeps_user_choice() {
        let mut app = App::default();
        let mut output = WasmOutput::new(&mut app, 7, "python demo.py".into());
        output.update(&mut app, "hello", "warning\n", "Running");
        output.update(&mut app, " world\n", "", "Completed (exit code 0)");
        assert_eq!(app.messages.len(), 1);
        let collapsed = draw(&mut app);
        assert!(collapsed.contains("[+] WASM: python demo.py"));
        assert!(collapsed.contains("Completed (exit code 0)"));
        assert!(!collapsed.contains("hello") && !collapsed.contains("warning"));
        app.key(Key::Tab);
        app.key(Key::Enter);
        let expanded = draw(&mut app);
        assert!(expanded.contains("Standard output:") && expanded.contains("hello world"));
        assert!(expanded.contains("Standard error:") && expanded.contains("warning"));
        output.update(&mut app, "late output\n", "", "Completed (exit code 0)");
        assert!(app.messages[0].expanded);
        assert!(draw(&mut app).contains("late output"));
        app.key(Key::Enter);
        assert!(!draw(&mut app).contains("late output"));
    }

    #[test]
    fn clear_during_background_execution_does_not_overwrite_other_messages() {
        let mut app = App::default();
        let mut output = WasmOutput::new(&mut app, 8, "ash".into());
        output.update(&mut app, "old", "", "Running");
        app.clear_messages();
        app.message("user", "keep me".into());
        output.update(&mut app, "new", "", "Cancelled");
        assert_eq!(app.messages.len(), 2);
        assert_eq!(app.messages[0].content, "keep me");
        assert!(!app.messages[1].expanded);
        assert!(app.messages[1].content.contains("new"));
        assert!(!app.messages[1].content.contains("old"));
    }

    #[test]
    fn active_process_disclosures_bound_each_stream_and_command_copy() {
        let mut app = App::default();
        app.history_limit = 4096;
        let mut output = WasmOutput::new(&mut app, 9, "x".repeat(100_000));
        output.update(
            &mut app,
            &"中".repeat(100_000),
            &"\u{0001}".repeat(100_000),
            "Running",
        );
        assert!(output.command.len() <= 4096);
        assert!(output.stdout.len() <= 64 * 1024);
        assert!(output.stderr.len() <= 64 * 1024);
        assert!(output.stdout_truncated && output.stderr_truncated);
        assert!(app.messages[0].content.len() < 256 * 1024);
        assert!(app.messages[0].content.contains("later stdout omitted"));
        assert!(app.messages[0].content.contains("later stderr omitted"));
    }
}
