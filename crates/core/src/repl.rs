//! Route direct interpreter input separately from conversation prompts.
use alloc::{collections::VecDeque, string::String, vec::Vec};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    Shell,
    JavaScript,
}

impl Language {
    pub fn slot(self) -> usize {
        match self {
            Self::Shell => 0,
            Self::JavaScript => 1,
        }
    }

    pub fn prompt(self) -> &'static str {
        match self {
            Self::Shell => "$ ",
            Self::JavaScript => "js> ",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Shell => "shell",
            Self::JavaScript => "js",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Conversation,
    Repl(Language),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Submission<'a> {
    Enter(Language),
    InvalidEnter,
    Application,
    Code(Language, &'a str),
}

impl Mode {
    pub fn prompt(self) -> &'static str {
        match self {
            Self::Conversation => "› ",
            Self::Repl(language) => language.prompt(),
        }
    }

    pub fn route(self, source: &str) -> Submission<'_> {
        if let Self::Repl(language) = self {
            return Submission::Code(language, source);
        }
        let mut words = source.split_whitespace();
        if words.next() == Some("/enter") {
            let language = match words.next() {
                None | Some("shell") => Language::Shell,
                Some("js") => Language::JavaScript,
                _ => return Submission::InvalidEnter,
            };
            return if words.next().is_none() {
                Submission::Enter(language)
            } else {
                Submission::InvalidEnter
            };
        }
        Submission::Application
    }
}

const SCROLLBACK_BYTES: usize = 256 * 1024;
const HISTORY_BYTES: usize = 128 * 1024;

#[derive(Default)]
pub struct History {
    entries: VecDeque<String>,
    position: Option<usize>,
    draft: String,
}

impl History {
    fn remember(&mut self, source: &str) {
        self.position = None;
        self.draft.clear();
        if source.trim().is_empty() || self.entries.back().is_some_and(|last| last == source) {
            return;
        }
        self.entries.push_back(source.into());
        while self.entries.len() > 256
            || self.entries.iter().map(String::len).sum::<usize>() > HISTORY_BYTES
        {
            self.entries.pop_front();
        }
    }

    pub fn navigate(&mut self, down: bool, draft: &str) -> Option<String> {
        if self.entries.is_empty() {
            return None;
        }
        let index = match (self.position, down) {
            (None, true) => return None,
            (None, false) => {
                self.draft = draft.into();
                self.entries.len() - 1
            }
            (Some(index), false) => index.saturating_sub(1),
            (Some(index), true) if index + 1 < self.entries.len() => index + 1,
            (Some(_), true) => {
                self.position = None;
                return Some(core::mem::take(&mut self.draft));
            }
        };
        self.position = Some(index);
        Some(self.entries[index].clone())
    }

    pub fn reset_navigation(&mut self) {
        self.position = None;
        self.draft.clear();
    }
}

pub enum Input {
    Bytes(String),
    Eof,
}

/// Local scrollback and line editing state, independent of model messages.
#[derive(Default)]
pub struct Terminal {
    pub text: String,
    pub history: History,
    pub running: bool,
    pub interrupted: bool,
    pub scroll: u16,
    pub revision: u64,
    pub input: VecDeque<Input>,
    pending: Vec<u8>,
    output_cursor: usize,
    escape: String,
}

impl Terminal {
    pub fn begin(&mut self, language: Language, source: &str) {
        self.history.remember(source);
        self.append(language.prompt());
        self.append(source);
        self.append("\n");
        self.running = true;
        self.interrupted = false;
        self.scroll = 0;
    }

    pub fn append(&mut self, text: &str) {
        self.text.push_str(text);
        self.output_cursor = self.text.len();
        self.trim();
        self.revision = self.revision.wrapping_add(1);
    }

    fn trim(&mut self) {
        if self.text.len() > SCROLLBACK_BYTES {
            let mut start = self.text.len() - SCROLLBACK_BYTES;
            while !self.text.is_char_boundary(start) {
                start += 1;
            }
            self.text.drain(..start);
            self.output_cursor = self.output_cursor.saturating_sub(start);
        }
    }

    fn write_output(&mut self, text: &str) {
        for ch in text.chars() {
            if !self.escape.is_empty() {
                self.escape.push(ch);
                let csi = self.escape.starts_with("\x1b[");
                if csi && self.escape.len() > 2 && ('@'..='~').contains(&ch) {
                    if self.escape == "\x1b[K" || self.escape == "\x1b[0K" {
                        self.text.truncate(self.output_cursor);
                    }
                    self.escape.clear();
                } else if self.escape == "\x1b]" || self.escape.starts_with("\x1b]") {
                    if ch == '\x07' || self.escape.ends_with("\x1b\\") {
                        self.escape.clear();
                    } else if self.escape.len() > 64 {
                        self.escape.truncate(2);
                    }
                } else if !csi || self.escape.len() > 64 {
                    self.escape.clear();
                }
                continue;
            }
            match ch {
                '\x1b' => self.escape.push(ch),
                '\r' => {
                    self.output_cursor = self.text[..self.output_cursor]
                        .rfind('\n')
                        .map_or(0, |index| index + 1)
                }
                '\n' => {
                    self.text.push('\n');
                    self.output_cursor = self.text.len();
                }
                '\x08' => {
                    if let Some((index, ch)) =
                        self.text[..self.output_cursor].char_indices().next_back()
                        && ch != '\n'
                    {
                        self.output_cursor = index;
                    }
                }
                '\t' => {
                    let start = self.text[..self.output_cursor]
                        .rfind('\n')
                        .map_or(0, |index| index + 1);
                    let column =
                        ratatui::text::Line::from(&self.text[start..self.output_cursor]).width();
                    self.write_output(&" ".repeat(8 - column % 8));
                }
                ch if !ch.is_control() => {
                    let end = self.text[self.output_cursor..]
                        .chars()
                        .next()
                        .filter(|ch| *ch != '\n')
                        .map_or(self.output_cursor, |ch| self.output_cursor + ch.len_utf8());
                    let mut bytes = [0; 4];
                    self.text
                        .replace_range(self.output_cursor..end, ch.encode_utf8(&mut bytes));
                    self.output_cursor += ch.len_utf8();
                }
                _ => {}
            }
        }
        self.trim();
        self.revision = self.revision.wrapping_add(1);
    }

    /// Keep UTF-8 fragments until a later write completes the code point.
    pub fn output(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);
        loop {
            match core::str::from_utf8(&self.pending) {
                Ok(_) => {
                    let text = String::from_utf8(core::mem::take(&mut self.pending)).unwrap();
                    self.write_output(&text);
                    break;
                }
                Err(error) => {
                    let valid = error.valid_up_to();
                    let text = String::from_utf8(self.pending.drain(..valid).collect()).unwrap();
                    if !text.is_empty() {
                        self.write_output(&text);
                    }
                    if let Some(length) = error.error_len() {
                        self.pending.drain(..length);
                        self.write_output("�");
                    } else {
                        break;
                    }
                }
            }
        }
    }

    pub fn finish(&mut self) {
        if !self.pending.is_empty() {
            self.append("�");
            self.pending.clear();
        }
        self.running = false;
        self.input.clear();
    }

    pub fn clear_screen(&mut self) {
        self.text.clear();
        self.output_cursor = 0;
        self.scroll = 0;
        self.revision = self.revision.wrapping_add(1);
    }
}

#[derive(Default)]
pub(crate) struct Row {
    pub text: String,
    pub output: Option<(u16, u16)>,
    pub input: Option<(u16, u16)>,
    pub cursor: Option<u16>,
}

/// Wrap at terminal columns, preserving spaces and input selection coordinates.
pub(crate) struct Screen {
    pub rows: Vec<Row>,
    width: u16,
    column: u16,
}

impl Screen {
    pub fn new(width: u16) -> Self {
        Self {
            rows: alloc::vec![Row::default()],
            width: width.max(1),
            column: 0,
        }
    }

    pub fn push(&mut self, text: &str, input: Option<bool>) {
        use unicode_segmentation::UnicodeSegmentation;
        for grapheme in text.graphemes(true) {
            if grapheme == "\t" {
                self.push(&" ".repeat(8 - usize::from(self.column) % 8), input);
                continue;
            }
            if grapheme == "\n" {
                self.mark(input);
                self.rows.push(Row::default());
                self.column = 0;
                continue;
            }
            let width = ratatui::text::Span::raw(grapheme)
                .width()
                .min(self.width as usize) as u16;
            if self.column + width > self.width {
                self.rows.push(Row::default());
                self.column = 0;
            }
            self.mark(input);
            self.rows.last_mut().unwrap().text.push_str(grapheme);
            self.column += width;
            self.mark(input);
        }
    }

    fn mark(&mut self, input: Option<bool>) {
        let row = self.rows.last_mut().unwrap();
        let range = match input {
            Some(true) => &mut row.input,
            Some(false) => &mut row.output,
            None => return,
        };
        let start = range.map_or(self.column, |(first, _)| first);
        *range = Some((start, self.column));
    }

    pub fn cursor(&mut self) {
        if self.column == self.width {
            self.rows.push(Row::default());
            self.column = 0;
        }
        self.rows.last_mut().unwrap().cursor = Some(self.column);
        self.push("▏", None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_restores_drafts_and_keeps_languages_separate() {
        let mut shell = Terminal::default();
        let mut js = Terminal::default();
        shell.begin(Language::Shell, "echo first");
        shell.begin(Language::Shell, "echo second");
        js.begin(Language::JavaScript, "1 + 1");
        assert_eq!(
            shell.history.navigate(false, "unfinished"),
            Some("echo second".into())
        );
        assert_eq!(shell.history.navigate(false, ""), Some("echo first".into()));
        assert_eq!(shell.history.navigate(true, ""), Some("echo second".into()));
        assert_eq!(shell.history.navigate(true, ""), Some("unfinished".into()));
        assert_eq!(js.history.navigate(false, ""), Some("1 + 1".into()));
    }

    #[test]
    fn output_preserves_newlines_and_split_utf8_with_bounded_scrollback() {
        let mut terminal = Terminal::default();
        terminal.begin(Language::Shell, "printf example");
        terminal.output(&[0xe4, 0xb8]);
        terminal.output(&[0xad, b'\n', b'x']);
        terminal.finish();
        assert_eq!(terminal.text, "$ printf example\n中\nx");
        terminal.output(&[0xff, 0xc2]);
        terminal.finish();
        assert!(terminal.text.ends_with("x��"));
        terminal.append(&"中".repeat(SCROLLBACK_BYTES));
        assert!(terminal.text.len() <= SCROLLBACK_BYTES);
        assert!(terminal.text.chars().all(|ch| ch == '中'));
    }

    #[test]
    fn terminal_controls_and_column_wrapping_preserve_layout() {
        let mut terminal = Terminal::default();
        terminal.output(b"long progress\rOK\x1b[K\n\x1b[31mred\x1b[0m\tX");
        assert_eq!(terminal.text, "OK\nred     X");
        let mut screen = Screen::new(8);
        screen.push("ab cd ef 中", Some(false));
        screen.push("$ ", None);
        screen.push("e\u{301}z", Some(true));
        screen.cursor();
        assert_eq!(
            screen
                .rows
                .iter()
                .map(|row| row.text.as_str())
                .collect::<Vec<_>>(),
            ["ab cd ef", " 中$ e\u{301}z▏"]
        );
        assert_eq!(screen.rows[1].input, Some((5, 7)));
        assert_eq!(screen.rows[1].cursor, Some(7));
    }

    #[test]
    fn repl_passes_all_source_to_the_interpreter_unchanged() {
        for language in [Language::Shell, Language::JavaScript] {
            let mode = Mode::Repl(language);
            for source in [
                "/bin/ls -l",
                "/leave",
                " /leave\n",
                "/enter",
                "/enter js",
                "/enter python",
                "/help",
                "/clear",
                "/quit",
                "/exit",
                "exit 7",
                "quit(9)",
                "/hello/.test('hello')",
                " /entering ",
                "/leave; echo kept",
                "/clear extra",
                "echo 'a b'\n  echo 中",
                "let value = 7;\nvalue + 1",
            ] {
                assert_eq!(mode.route(source), Submission::Code(language, source));
            }
        }
    }

    #[test]
    fn enter_requires_a_supported_language_and_no_extra_arguments() {
        let mode = Mode::Conversation;
        assert_eq!(mode.route("/enter"), Submission::Enter(Language::Shell));
        assert_eq!(
            mode.route(" /enter shell\n"),
            Submission::Enter(Language::Shell)
        );
        assert_eq!(
            mode.route("/enter js"),
            Submission::Enter(Language::JavaScript)
        );
        for source in ["/enter python", "/enter shell extra", "/enter js code"] {
            assert_eq!(mode.route(source), Submission::InvalidEnter);
        }
        assert_eq!(mode.route("/bin/ls"), Submission::Application);
        assert_eq!(mode.route("hello"), Submission::Application);
    }
}
