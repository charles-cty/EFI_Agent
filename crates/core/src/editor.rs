//! UTF-8 prompt editing at extended grapheme boundaries.
use alloc::string::String;
use unicode_segmentation::UnicodeSegmentation;

pub const MAX_PROMPT_BYTES: usize = 64 * 1024;

#[derive(Default)]
pub struct Editor {
    pub text: String,
    cursor: usize,
}

impl Editor {
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    fn previous(&self) -> usize {
        self.text[..self.cursor]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(index, _)| index)
    }

    fn next(&self) -> usize {
        self.text[self.cursor..]
            .graphemes(true)
            .next()
            .map_or(self.text.len(), |g| self.cursor + g.len())
    }

    pub fn insert(&mut self, character: char) {
        if self.text.len() + character.len_utf8() > MAX_PROMPT_BYTES {
            return;
        }
        self.text.insert(self.cursor, character);
        self.cursor += character.len_utf8();
        // Insertion can join a following combining mark or ZWJ sequence. Move
        // to the end of the resulting grapheme so the next edit stays valid.
        self.normalize_cursor();
    }

    fn normalize_cursor(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.cursor = self
            .text
            .grapheme_indices(true)
            .map(|(index, g)| index + g.len())
            .find(|end| *end >= self.cursor)
            .unwrap_or(self.text.len());
    }

    pub fn left(&mut self) {
        self.cursor = self.previous();
    }
    pub fn right(&mut self) {
        self.cursor = self.next();
    }
    pub fn home(&mut self) {
        self.cursor = self.text[..self.cursor]
            .rfind('\n')
            .map_or(0, |index| index + 1);
    }
    pub fn end(&mut self) {
        self.cursor = self.text[self.cursor..]
            .find('\n')
            .map_or(self.text.len(), |index| self.cursor + index);
    }
    pub fn backspace(&mut self) {
        let start = self.previous();
        self.text.replace_range(start..self.cursor, "");
        self.cursor = start;
        self.normalize_cursor();
    }
    pub fn delete(&mut self) {
        let end = self.next();
        self.text.replace_range(self.cursor..end, "");
        self.normalize_cursor();
    }
    pub fn take(&mut self) -> String {
        self.cursor = 0;
        core::mem::take(&mut self.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edits_graphemes_and_lines_without_splitting_utf8() {
        let mut editor = Editor::default();
        for c in "left 中 e\u{301} 👩\u{200d}💻\nlast".chars() {
            editor.insert(c);
        }
        editor.home();
        editor.left();
        editor.backspace();
        assert_eq!(editor.text, "left 中 e\u{301} \nlast");
        editor.left();
        editor.backspace();
        assert_eq!(editor.text, "left 中  \nlast");
        editor.home();
        editor.right();
        editor.delete();
        editor.insert('X');
        assert_eq!(editor.text, "lXft 中  \nlast");
        editor.end();
        assert_eq!(editor.cursor(), "lXft 中  ".len());
    }
    #[test]
    fn insertion_before_combining_mark_keeps_cursor_on_boundary() {
        let mut editor = Editor::default();
        editor.insert('\u{301}');
        editor.home();
        editor.insert('e');
        assert_eq!(editor.cursor(), editor.text.len());
        editor.backspace();
        assert!(editor.text.is_empty());
        editor.delete();
        editor.left();
        editor.right();
        assert_eq!(editor.cursor(), 0);
    }

    #[test]
    fn structured_random_edits_keep_grapheme_boundaries() {
        let alphabet = ['a', '中', '\u{301}', '👩', '\u{200d}', '💻', '🇦', '🇧', '\n'];
        let mut seed = 731u32;
        let mut editor = Editor::default();
        for _ in 0..12000 {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            match seed % 12 {
                0 => editor.left(),
                1 => editor.right(),
                2 => editor.home(),
                3 => editor.end(),
                4 => editor.delete(),
                5 => editor.backspace(),
                _ => editor.insert(alphabet[(seed as usize / 12) % alphabet.len()]),
            }
            let cursor = editor.cursor();
            assert!(editor.text.is_char_boundary(cursor));
            assert!(
                cursor == editor.text.len()
                    || editor
                        .text
                        .grapheme_indices(true)
                        .any(|(index, _)| index == cursor)
            );
            if editor.text.len() > 200 {
                editor.take();
            }
        }
    }
}
