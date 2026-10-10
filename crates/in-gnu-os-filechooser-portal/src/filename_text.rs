//! Unicode-safe filename editing state, independent of GPUI and portal authority.
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

const MAX_FILENAME_BYTES: usize = 240;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilenameBuffer {
    text: String,
    selection: Range<usize>, // byte offsets, always UTF-8 char boundaries
    marked: Option<Range<usize>>,
}

impl FilenameBuffer {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let len = text.len();
        Self {
            text,
            selection: len..len,
            marked: None,
        }
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn selection(&self) -> Range<usize> {
        self.selection.clone()
    }
    pub fn mark(&self) -> Option<Range<usize>> {
        self.marked.clone()
    }
    pub fn clear_mark(&mut self) {
        self.marked = None;
    }
    pub fn len_utf16(&self) -> usize {
        self.text.encode_utf16().count()
    }
    pub fn utf16_at(&self, byte: usize) -> usize {
        self.text[..self.clamp_byte(byte)].encode_utf16().count()
    }
    pub fn byte_at_utf16(&self, index: usize) -> usize {
        let mut units = 0;
        for (byte, ch) in self.text.char_indices() {
            if units >= index || units + ch.len_utf16() > index {
                return byte;
            }
            units += ch.len_utf16();
        }
        self.text.len()
    }
    pub fn selected_utf16(&self) -> Range<usize> {
        self.utf16_at(self.selection.start)..self.utf16_at(self.selection.end)
    }
    pub fn set_selection_utf16(&mut self, range: Range<usize>) {
        let a = self.byte_at_utf16(range.start);
        let b = self.byte_at_utf16(range.end);
        self.selection = a.min(b)..a.max(b);
    }
    pub fn range_from_utf16(&self, range: Range<usize>) -> Range<usize> {
        let a = self.byte_at_utf16(range.start);
        let b = self.byte_at_utf16(range.end);
        a.min(b)..a.max(b)
    }
    pub fn replace(
        &mut self,
        range_utf16: Option<Range<usize>>,
        replacement: &str,
        marking: bool,
    ) -> bool {
        // Reject unrepresentable filenames at input ingress, including clipboard and IME.
        if replacement.chars().any(|ch| ch.is_control() || ch == '/') {
            return false;
        }
        let range = range_utf16
            .map(|r| self.range_from_utf16(r))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selection.clone());
        let new_len = self.text.len() - (range.end - range.start) + replacement.len();
        if new_len > MAX_FILENAME_BYTES {
            return false;
        }
        self.text.replace_range(range.clone(), replacement);
        let end = range.start + replacement.len();
        self.selection = end..end;
        self.marked = if marking && !replacement.is_empty() {
            Some(range.start..end)
        } else {
            None
        };
        true
    }
    pub fn set_mark_selection(&mut self, range: Range<usize>) {
        if let Some(marked) = &self.marked {
            let a = self.byte_at_utf16(self.utf16_at(marked.start).saturating_add(range.start));
            let b = self.byte_at_utf16(self.utf16_at(marked.start).saturating_add(range.end));
            self.selection = a.min(b)..a.max(b);
        }
    }
    pub fn select_all(&mut self) {
        self.selection = 0..self.text.len();
    }
    pub fn home(&mut self) {
        self.selection = 0..0;
    }
    pub fn end(&mut self) {
        let n = self.text.len();
        self.selection = n..n;
    }
    pub fn left(&mut self, extend: bool) {
        let pos = if !extend && !self.selection.is_empty() {
            self.selection.start
        } else {
            self.text
                .grapheme_indices(true)
                .map(|(i, _)| i)
                .take_while(|i| *i < self.selection.end)
                .last()
                .unwrap_or(0)
        };
        if extend {
            self.selection = self.selection.start.min(pos)..self.selection.end.max(pos);
        } else {
            self.selection = pos..pos;
        }
    }
    pub fn right(&mut self, extend: bool) {
        let pos = if !extend && !self.selection.is_empty() {
            self.selection.end
        } else {
            self.text
                .grapheme_indices(true)
                .map(|(i, _)| i)
                .find(|i| *i > self.selection.end)
                .unwrap_or(self.text.len())
        };
        if extend {
            self.selection = self.selection.start.min(pos)..self.selection.end.max(pos);
        } else {
            self.selection = pos..pos;
        }
    }
    pub fn backspace(&mut self) -> bool {
        if self.selection.is_empty() {
            let prev = self
                .text
                .grapheme_indices(true)
                .map(|(i, _)| i)
                .take_while(|i| *i < self.selection.start)
                .last()
                .unwrap_or(0);
            self.selection = prev..self.selection.start;
        }
        self.replace(None, "", false)
    }
    pub fn delete_forward(&mut self) -> bool {
        if self.selection.is_empty() {
            let next = self
                .text
                .grapheme_indices(true)
                .map(|(i, _)| i)
                .find(|i| *i > self.selection.end)
                .unwrap_or(self.text.len());
            self.selection = self.selection.end..next;
        }
        self.replace(None, "", false)
    }
    pub fn set_text(&mut self, text: &str) -> bool {
        if text.len() > MAX_FILENAME_BYTES || text.chars().any(|ch| ch == '/' || ch.is_control()) {
            return false;
        }
        self.text = text.to_owned();
        self.end();
        self.marked = None;
        true
    }
    fn clamp_byte(&self, pos: usize) -> usize {
        let mut n = pos.min(self.text.len());
        while n > 0 && !self.text.is_char_boundary(n) {
            n -= 1;
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn utf16_surrogate_and_combining_mark_roundtrip() {
        let a = FilenameBuffer::new("é🦀e\u{301}.txt");
        assert_eq!(a.len_utf16(), 9);
        assert_eq!(a.byte_at_utf16(1), 2);
        assert_eq!(a.byte_at_utf16(2), 2); // middle of surrogate pair -> boundary before emoji
        assert_eq!(a.byte_at_utf16(3), 6);
        assert_eq!(a.utf16_at(6), 3);
    }
    #[test]
    fn pasted_content_replaces_exact_utf16_range() {
        let mut a = FilenameBuffer::new("🐝ok.txt");
        assert!(a.replace(Some(0..2), "N", false));
        assert_eq!(a.text(), "Nok.txt");
        assert_eq!(a.selected_utf16(), 1..1);
    }
    #[test]
    fn ime_mark_and_unmark_commit() {
        let mut a = FilenameBuffer::new("report");
        assert!(a.replace(None, "あ", true));
        assert_eq!(a.mark(), Some(6..9));
        assert!(a.replace(None, "日本", true));
        assert_eq!(a.text(), "report日本");
        assert!(a.mark().is_some());
        a.clear_mark();
        assert!(a.mark().is_none());
    }
    #[test]
    fn grapheme_backspace_does_not_split_emoji_or_combining() {
        let mut a = FilenameBuffer::new("a👨‍👩‍👧‍👦");
        assert!(a.backspace());
        assert_eq!(a.text(), "a");
        assert!(a.backspace());
        assert_eq!(a.text(), "");
        let mut b = FilenameBuffer::new("e\u{301}");
        assert!(b.backspace());
        assert_eq!(b.text(), "");
    }
    #[test]
    fn selection_clipboard_and_invalid_paths() {
        let mut a = FilenameBuffer::new("report.txt");
        a.select_all();
        assert_eq!(a.selection(), 0..10);
        assert!(a.replace(None, "draft.md", false));
        assert_eq!(a.text(), "draft.md");
        assert!(!a.replace(None, "../bad", false));
        assert!(!a.replace(None, "line\nbreak", false));
        assert_eq!(a.text(), "draft.md");
        a.home();
        assert!(a.delete_forward());
        assert_eq!(a.text(), "raft.md");
    }
    #[test]
    fn length_cap_and_choice_of_unicode_boundary() {
        let mut a = FilenameBuffer::new("");
        assert!(!a.replace(None, &"x".repeat(241), false));
        assert!(a.replace(None, &"x".repeat(240), false));
        assert!(!a.replace(None, "ü", false));
        assert_eq!(a.text().len(), 240);
    }
}
