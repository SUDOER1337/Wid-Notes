use std::ops::Range;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SelectionMode {
    Charwise,
    Linewise,
}

#[derive(Clone, Debug)]
pub struct Selection {
    pub anchor: usize,
    pub active: usize,
    pub mode: SelectionMode,
}

impl Selection {
    pub fn new(anchor: usize) -> Self {
        Self {
            anchor,
            active: anchor,
            mode: SelectionMode::Charwise,
        }
    }

    pub fn range(&self) -> Range<usize> {
        self.anchor.min(self.active)..self.anchor.max(self.active)
    }

    pub fn start(&self) -> usize {
        self.anchor.min(self.active)
    }

    pub fn end(&self) -> usize {
        self.anchor.max(self.active)
    }
}

pub struct Buffer {
    pub text: String,
    pub cursor: usize,
    pub selection: Option<Selection>,
    line_starts: Vec<usize>,
}

impl Buffer {
    /// Rebuild the line-start byte-offset cache. O(n) in text length; called
    /// once per edit rather than once per query.
    pub fn rebuild_line_starts(&mut self) {
        self.line_starts.clear();
        self.line_starts.push(0);
        for (i, b) in self.text.bytes().enumerate() {
            if b == b'\n' {
                self.line_starts.push(i + 1);
            }
        }
    }

    /// Replace the entire buffer text, resetting cursor and selection.
    /// Use this instead of `buffer.text = …` so line_starts stays valid.
    pub fn set_text(&mut self, text: String) {
        self.text = text;
        self.cursor = 0;
        self.selection = None;
        self.rebuild_line_starts();
    }

    pub fn new() -> Self {
        let mut buf = Self {
            text: String::new(),
            cursor: 0,
            selection: None,
            line_starts: Vec::new(),
        };
        buf.rebuild_line_starts();
        buf
    }

    pub fn new_with_text(text: String) -> Self {
        let mut buf = Self::new();
        buf.text = text;
        buf.rebuild_line_starts();
        buf
    }

    pub fn line_col(&self, pos: usize) -> (usize, usize) {
        let pos = pos.min(self.text.len());
        // Binary search: last line_start that is <= pos
        let line = self
            .line_starts
            .partition_point(|&s| s <= pos)
            .saturating_sub(1);
        let line_start = self.line_starts[line];
        let col = self.text[line_start..pos].chars().count();
        (line, col)
    }

    pub fn byte_from_line_col(&self, line: usize, col: usize) -> usize {
        let start = self.line_start(line);
        let end = self.line_end(line);
        self.text[start..end]
            .char_indices()
            .nth(col)
            .map(|(i, _)| start + i)
            .unwrap_or(end)
    }

    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    pub fn current_line(&self) -> &str {
        let (line, _) = self.line_col(self.cursor);
        self.line_text(line)
    }

    pub fn line_text(&self, idx: usize) -> &str {
        let start = self.line_start(idx);
        let end = self.line_end(idx);
        if start <= end && end <= self.text.len() {
            &self.text[start..end]
        } else {
            ""
        }
    }

    pub fn line_start(&self, idx: usize) -> usize {
        self.line_starts
            .get(idx)
            .copied()
            .unwrap_or(self.text.len())
    }

    pub fn line_end(&self, idx: usize) -> usize {
        match self.line_starts.get(idx + 1) {
            Some(&next) => next.saturating_sub(1), // byte position of the '\n'
            None => self.text.len(),
        }
    }

    pub fn move_left(&self, cursor: usize) -> usize {
        if cursor == 0 {
            return 0;
        }
        let mut p = cursor - 1;
        while !self.text.is_char_boundary(p) {
            p -= 1;
        }
        p
    }

    pub fn move_right(&self, cursor: usize) -> usize {
        if cursor >= self.text.len() {
            return cursor;
        }
        let mut p = cursor + 1;
        while !self.text.is_char_boundary(p) {
            p += 1;
        }
        p
    }

    pub fn move_up(&self, cursor: usize) -> usize {
        let (line, col) = self.line_col(cursor);
        if line == 0 {
            return cursor;
        }
        let prev = self.line_text(line - 1);
        let target = col.min(prev.chars().count());
        self.byte_from_line_col(line - 1, target)
    }

    pub fn move_down(&self, cursor: usize) -> usize {
        let (line, col) = self.line_col(cursor);
        let total = self.line_count();
        if line + 1 >= total {
            return self.text.len();
        }
        let next = self.line_text(line + 1);
        let target = col.min(next.chars().count());
        self.byte_from_line_col(line + 1, target)
    }

    pub fn move_line_start(&self, cursor: usize) -> usize {
        let (line, _) = self.line_col(cursor);
        self.byte_from_line_col(line, 0)
    }

    pub fn move_line_end(&self, cursor: usize) -> usize {
        let (line, _) = self.line_col(cursor);
        let count = self.line_text(line).chars().count();
        self.byte_from_line_col(line, count)
    }

    pub fn move_first_nonblank(&self, cursor: usize) -> usize {
        let (line, _) = self.line_col(cursor);
        let text = self.line_text(line);
        // Use char position (not byte offset) — str::find returns bytes which
        // would be wrong for multibyte leading whitespace.
        let col = text.chars().position(|c| !c.is_whitespace()).unwrap_or(0);
        self.byte_from_line_col(line, col)
    }

    fn is_keyword(c: char) -> bool {
        c.is_alphanumeric() || c == '_'
    }

    fn class(c: char) -> WordClass {
        if c.is_whitespace() {
            WordClass::Space
        } else if Self::is_keyword(c) {
            WordClass::Keyword
        } else {
            WordClass::Other
        }
    }

    pub fn move_word_forward(&self, cursor: usize) -> usize {
        let s = &self.text[cursor..];
        if s.is_empty() {
            return cursor;
        }
        let first = Self::class(s.chars().next().unwrap());
        let i = if first == WordClass::Space {
            s.find(|c: char| !c.is_whitespace())
                .map(|i| i + cursor)
                .unwrap_or(self.text.len())
        } else {
            let end_of_word = s[1..]
                .find(|c| Self::class(c) != first)
                .map(|i| i + 1 + cursor)
                .unwrap_or(self.text.len());

            self.text[end_of_word..]
                .find(|c: char| !c.is_whitespace())
                .map(|i| i + end_of_word)
                .unwrap_or(self.text.len())
        };
        if i == cursor { self.text.len() } else { i }
    }

    pub fn move_word_back(&self, cursor: usize) -> usize {
        if cursor == 0 {
            return 0;
        }
        let before = &self.text[..cursor];
        let trimmed = before.trim_end_matches(|c: char| c.is_whitespace());
        if trimmed.is_empty() {
            return 0;
        }
        let start_of_word = trimmed
            .rfind(|c: char| Self::class(c) != Self::class(trimmed.chars().last().unwrap()))
            .map(|i| i + 1)
            .unwrap_or(0);
        let real_start = trimmed[..start_of_word]
            .rfind(|c: char| c.is_whitespace())
            .map(|i| i + 1)
            .unwrap_or(0);
        let mut p = if start_of_word == 0
            && Self::class(trimmed.chars().next().unwrap_or(' '))
                == Self::class(trimmed.chars().last().unwrap_or(' '))
        {
            real_start
        } else {
            start_of_word
        };
        while !self.text.is_char_boundary(p) {
            p -= 1;
        }
        p
    }

    pub fn move_word_end(&self, cursor: usize) -> usize {
        if cursor >= self.text.len() {
            return cursor;
        }
        let s = &self.text[cursor..];
        let first_non_ws = s.find(|c: char| !c.is_whitespace());
        let start = match first_non_ws {
            Some(i) => cursor + i,
            None => return self.text.len(),
        };
        let from_start = &self.text[start..];
        let first = Self::class(from_start.chars().next().unwrap());
        let end = from_start
            .find(|c| Self::class(c) != first)
            .unwrap_or(from_start.len());
        if end == 0 {
            cursor
        } else {
            let mut p = start + end - 1;
            while !self.text.is_char_boundary(p) {
                p -= 1;
            }
            p
        }
    }

    pub fn move_big_word_forward(&self, cursor: usize) -> usize {
        let s = &self.text[cursor..];
        if s.is_empty() {
            return cursor;
        }
        let first_non_ws = s.find(|c: char| !c.is_whitespace());
        let start = match first_non_ws {
            Some(i) => cursor + i,
            None => return self.text.len(),
        };
        let after_word = self.text[start..]
            .find(|c: char| c.is_whitespace())
            .map(|i| i + start)
            .unwrap_or(self.text.len());
        let next = self.text[after_word..]
            .find(|c: char| !c.is_whitespace())
            .map(|i| i + after_word)
            .unwrap_or(self.text.len());
        if next == cursor {
            self.text.len()
        } else {
            next
        }
    }

    pub fn move_big_word_back(&self, cursor: usize) -> usize {
        if cursor == 0 {
            return 0;
        }
        let before = &self.text[..cursor];
        let trimmed = before.trim_end_matches(|c: char| c.is_whitespace());
        if trimmed.is_empty() {
            return 0;
        }
        let word_start = trimmed
            .rfind(|c: char| c.is_whitespace())
            .map(|i| i + 1)
            .unwrap_or(0);
        let mut p = word_start;
        while !self.text.is_char_boundary(p) {
            p -= 1;
        }
        p
    }

    pub fn move_big_word_end(&self, cursor: usize) -> usize {
        if cursor >= self.text.len() {
            return cursor;
        }
        let s = &self.text[cursor..];
        let first_non_ws = s.find(|c: char| !c.is_whitespace());
        let start = match first_non_ws {
            Some(i) => cursor + i,
            None => return self.text.len(),
        };
        let ws = self.text[start..].find(|c: char| c.is_whitespace());
        match ws {
            Some(i) if i > 0 => {
                let mut p = start + i - 1;
                while !self.text.is_char_boundary(p) {
                    p -= 1;
                }
                p
            }
            None => self.text.len(),
            _ => cursor,
        }
    }

    pub fn move_paragraph_forward(&self, cursor: usize) -> usize {
        let after = &self.text[cursor..];
        let blank = after
            .find('\n')
            .and_then(|i| {
                let rest = &after[i..];
                rest.find(|c: char| c != '\n').or(Some(rest.len()))
            })
            .map(|i| cursor + i)
            .unwrap_or(self.text.len());
        let next_blank = self.text[blank..]
            .find("\n\n")
            .map(|i| i + blank)
            .unwrap_or(self.text.len());
        let next_nonblank = self.text[next_blank..]
            .find(|c: char| c != '\n')
            .map(|i| i + next_blank)
            .unwrap_or(self.text.len());
        if next_nonblank == cursor {
            self.text.len()
        } else {
            next_nonblank
        }
    }

    pub fn move_paragraph_back(&self, cursor: usize) -> usize {
        if cursor == 0 {
            return 0;
        }
        let before = &self.text[..cursor];
        let trimmed = before.trim_end_matches('\n');
        if trimmed.is_empty() {
            return 0;
        }

        trimmed
            .rfind("\n\n")
            .map(|i| {
                let after_nl = &trimmed[i + 2..];
                let first_nonblank = after_nl.find(|c: char| c != '\n').unwrap_or(0);
                i + 2 + first_nonblank
            })
            .unwrap_or(0)
    }

    pub fn find_next_char(&self, cursor: usize, c: char) -> Option<usize> {
        let s = &self.text[self.move_right(cursor)..];
        s.find(c).map(|i| cursor + 1 + i)
    }

    pub fn find_prev_char(&self, cursor: usize, c: char) -> Option<usize> {
        if cursor == 0 {
            return None;
        }
        let before = &self.text[..cursor];
        before.rfind(c)
    }

    pub fn find_till_next_char(&self, cursor: usize, c: char) -> Option<usize> {
        self.find_next_char(cursor, c).map(|p| self.move_left(p))
    }

    pub fn find_till_prev_char(&self, cursor: usize, c: char) -> Option<usize> {
        self.find_prev_char(cursor, c).map(|p| self.move_right(p))
    }

    const BRACKET_PAIRS: &'static [(char, char)] = &[('(', ')'), ('[', ']'), ('{', '}')];

    pub fn matching_bracket(&self, cursor: usize) -> Option<usize> {
        let ch = self.text[cursor..].chars().next()?;
        for &(open, close) in Self::BRACKET_PAIRS {
            if ch == open {
                let mut depth = 1;
                let rest = &self.text[cursor + 1..];
                for (i, c) in rest.char_indices() {
                    if c == open {
                        depth += 1;
                    } else if c == close {
                        depth -= 1;
                    }
                    if depth == 0 {
                        return Some(cursor + 1 + i);
                    }
                }
            } else if ch == close {
                let mut depth = 1;
                let before = &self.text[..cursor];
                for (i, c) in before.char_indices().rev() {
                    if c == close {
                        depth += 1;
                    } else if c == open {
                        depth -= 1;
                    }
                    if depth == 0 {
                        return Some(i);
                    }
                }
            }
        }
        None
    }

    pub fn inner_word(&self, cursor: usize) -> Range<usize> {
        if self.text.is_empty() {
            return 0..0;
        }
        let pos = cursor.min(self.text.len().saturating_sub(1));
        let s = &self.text[pos..];
        let first = s.chars().next().unwrap_or(' ');
        if first.is_whitespace() {
            let ws_end = s
                .find(|c: char| !c.is_whitespace())
                .map(|i| pos + i)
                .unwrap_or(self.text.len());
            return pos..ws_end;
        }
        let start = self.text[..pos]
            .rfind(|c: char| Self::class(c) != Self::class(first))
            .map(|i| i + 1)
            .unwrap_or(0);
        let end = self.text[pos..]
            .find(|c: char| Self::class(c) != Self::class(first))
            .map(|i| pos + i)
            .unwrap_or(self.text.len());
        start..end
    }

    fn extend_with_ws(&self, inner: Range<usize>) -> Range<usize> {
        if inner.start == inner.end {
            return inner;
        }
        let trailing_ws_end = self.text[inner.end..]
            .find(|c: char| !c.is_whitespace())
            .map(|i| inner.end + i)
            .unwrap_or(self.text.len());
        if trailing_ws_end > inner.end {
            return inner.start..trailing_ws_end;
        }
        if inner.start > 0 {
            let before_word_end = self.text[..inner.start]
                .rfind(|c: char| !c.is_whitespace())
                .map(|i| i + 1)
                .unwrap_or(0);
            let leading_ws_start = self.text[..before_word_end.saturating_sub(1)]
                .rfind(|c: char| c.is_whitespace())
                .map(|w| w + 1)
                .unwrap_or(0);
            if leading_ws_start < inner.start {
                return leading_ws_start..inner.end;
            }
        }
        inner
    }

    pub fn a_word(&self, cursor: usize) -> Range<usize> {
        self.extend_with_ws(self.inner_word(cursor))
    }

    pub fn inner_big_word(&self, cursor: usize) -> Range<usize> {
        if self.text.is_empty() {
            return 0..0;
        }
        let pos = cursor.min(self.text.len().saturating_sub(1));
        let ch = self.text[pos..].chars().next().unwrap_or(' ');
        if ch.is_whitespace() {
            let ws_end = self.text[pos..]
                .find(|c: char| !c.is_whitespace())
                .map(|i| pos + i)
                .unwrap_or(self.text.len());
            return pos..ws_end;
        }
        let start = self.text[..pos]
            .rfind(|c: char| c.is_whitespace())
            .map(|i| i + 1)
            .unwrap_or(0);
        let end = self.text[pos..]
            .find(|c: char| c.is_whitespace())
            .map(|i| pos + i)
            .unwrap_or(self.text.len());
        start..end
    }

    pub fn a_big_word(&self, cursor: usize) -> Range<usize> {
        self.extend_with_ws(self.inner_big_word(cursor))
    }

    pub fn inner_paragraph(&self, cursor: usize) -> Range<usize> {
        let start = self.move_paragraph_back(cursor);
        let end = self.move_paragraph_forward(cursor);
        let text = self.text[start..end].trim_end_matches('\n');
        let real_end = start + text.len();
        let para_start = self.text[..start]
            .rfind(|c: char| c != '\n')
            .map(|i| {
                let after_nl = self.text[i..].find(|c: char| c != '\n').unwrap_or(0);
                i + after_nl
            })
            .unwrap_or(0);
        para_start..real_end
    }

    pub fn a_paragraph(&self, cursor: usize) -> Range<usize> {
        let start = self.move_paragraph_back(cursor);
        let end = self.move_paragraph_forward(cursor);
        let real_end = self.text[end..]
            .find(|c: char| c != '\n')
            .map(|i| end + i)
            .unwrap_or(self.text.len());
        start..real_end
    }

    /// Find the nearest pair, returning (open_pos, close_pos) where both are found.
    fn find_pair_bounds(&self, cursor: usize, open: char, close: char) -> Option<(usize, usize)> {
        let pos = cursor.min(self.text.len().saturating_sub(1));
        let ch = self.text[pos..].chars().next()?;
        if ch == open {
            let end = self.find_close(pos + 1, open, close)?;
            return Some((pos, end));
        }
        if ch == close {
            let start = self.find_open(pos, open, close)?;
            return Some((start, pos));
        }
        self.text[pos..]
            .char_indices()
            .find(|&(_, c)| c == open || c == close)
            .and_then(|(i, c)| {
                if c == open {
                    self.find_close(pos + i + 1, open, close)
                        .map(|e| (pos + i, e))
                } else {
                    self.find_open(pos + i, open, close).map(|s| (s, pos + i))
                }
            })
    }

    pub fn inner_pair(&self, cursor: usize, open: char, close: char) -> Option<Range<usize>> {
        self.find_pair_bounds(cursor, open, close)
            .map(|(s, e)| s + 1..e)
    }

    pub fn a_pair(&self, cursor: usize, open: char, close: char) -> Option<Range<usize>> {
        self.find_pair_bounds(cursor, open, close)
            .map(|(s, e)| s..e + 1)
    }

    fn find_quote_bounds(&self, cursor: usize, quote: char) -> Option<(usize, usize)> {
        let pos = cursor.min(self.text.len().saturating_sub(1));
        if pos + 1 > self.text.len() {
            return None;
        }
        let before = &self.text[..pos + 1];
        let after = &self.text[pos..];
        let open = before.rfind(quote)?;
        let close = after[1..].find(quote).map(|i| pos + 1 + i)?;
        if close > open {
            Some((open, close))
        } else {
            None
        }
    }

    pub fn inner_quote(&self, cursor: usize, quote: char) -> Option<Range<usize>> {
        self.find_quote_bounds(cursor, quote)
            .map(|(open, close)| open + 1..close)
    }

    pub fn a_quote(&self, cursor: usize, quote: char) -> Option<Range<usize>> {
        self.find_quote_bounds(cursor, quote)
            .map(|(open, close)| open..close + 1)
    }

    fn find_close(&self, start: usize, open: char, close: char) -> Option<usize> {
        let mut depth = 1;
        for (i, c) in self.text[start..].char_indices() {
            if c == open {
                depth += 1;
            } else if c == close {
                depth -= 1;
            }
            if depth == 0 {
                return Some(start + i);
            }
        }
        None
    }

    fn find_open(&self, end: usize, open: char, close: char) -> Option<usize> {
        let mut depth = 1;
        for (i, c) in self.text[..end].char_indices().rev() {
            if c == close {
                depth += 1;
            } else if c == open {
                depth -= 1;
            }
            if depth == 0 {
                return Some(i);
            }
        }
        None
    }

    pub fn insert(&mut self, pos: usize, s: &str) {
        self.text.insert_str(pos, s);
        self.cursor = pos + s.len();
        self.rebuild_line_starts();
    }

    pub fn delete(&mut self, range: Range<usize>) -> String {
        if range.start >= range.end {
            return String::new();
        }
        let deleted = self.text[range.start..range.end].to_string();
        self.text.drain(range.start..range.end);
        self.cursor = range.start.min(self.text.len());
        self.rebuild_line_starts();
        deleted
    }

    pub fn set_cursor(&mut self, pos: usize) {
        self.cursor = pos.min(self.text.len());
    }

    pub fn start_selection(&mut self) {
        self.selection = Some(Selection::new(self.cursor));
    }

    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    pub fn selection_range(&self) -> Option<Range<usize>> {
        self.selection.as_ref().map(|s| s.range())
    }

    pub fn replace_char(&mut self, pos: usize, c: char) {
        if pos >= self.text.len() || !self.text.is_char_boundary(pos) {
            return;
        }
        let byte_len = self.text[pos..]
            .chars()
            .next()
            .map(|ch| ch.len_utf8())
            .unwrap_or(1);
        self.text.drain(pos..pos + byte_len);
        self.text.insert(pos, c);
    }

    pub fn toggle_case_at(&mut self, pos: usize) {
        if pos >= self.text.len() || !self.text.is_char_boundary(pos) {
            return;
        }
        if let Some(c) = self.text[pos..].chars().next() {
            let byte_len = c.len_utf8();
            let toggled: String = if c.is_uppercase() {
                c.to_lowercase().collect()
            } else {
                c.to_uppercase().collect()
            };
            self.text.drain(pos..pos + byte_len);
            self.text.insert_str(pos, &toggled);
            self.cursor = pos + toggled.len();
            // Multi-char case mappings (e.g. ß→SS) change byte offsets of
            // subsequent lines.
            if toggled.len() != byte_len {
                self.rebuild_line_starts();
            }
        }
    }

    pub fn join_lines(&mut self, pos: usize) {
        let (line, _) = self.line_col(pos);
        let total = self.line_count();
        if line + 1 >= total {
            return;
        }
        let end = self.line_end(line);
        let next_start = self.line_start(line + 1);
        let next_text = self.line_text(line + 1);
        let trimmed = next_text.trim_start();
        let ws_to_remove = next_start - end + (next_text.len() - trimmed.len());
        let last_char_pos = self.byte_before(end);
        let space = if last_char_pos < end && self.text[last_char_pos..end].starts_with(' ') {
            ""
        } else {
            " "
        };
        self.text.drain(end..end + ws_to_remove);
        self.text.insert_str(end, space);
        self.cursor = end + space.len();
        self.rebuild_line_starts();
    }

    pub fn undo_snapshot(&self) -> Snapshot {
        Snapshot {
            text: self.text.clone(),
            cursor: self.cursor,
        }
    }

    pub fn restore_snapshot(&mut self, snap: &Snapshot) {
        self.text = snap.text.clone();
        self.cursor = snap.cursor;
        self.rebuild_line_starts();
    }

    pub fn byte_before(&self, pos: usize) -> usize {
        if pos == 0 {
            return 0;
        }
        let mut p = pos - 1;
        while !self.text.is_char_boundary(p) {
            p -= 1;
        }
        p
    }
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub text: String,
    pub cursor: usize,
}

#[derive(Clone, Copy, PartialEq)]
enum WordClass {
    Space,
    Keyword,
    Other,
}

impl Default for Buffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    // Tests grouped by feature.  Each function below constructs its own
    // Buffer so tests are self-contained and independent.

    use super::*;

    // --- Buffer creation & basics -------------------------------------------

    #[test]
    fn new_buffer_empty() {
        let b = Buffer::new();
        assert!(b.text.is_empty());
        assert_eq!(b.cursor, 0);
        assert!(b.selection.is_none());
        assert_eq!(b.line_count(), 1);
    }

    #[test]
    fn new_with_text_tracks_starts() {
        let b = Buffer::new_with_text("hello\nworld\n".into());
        assert_eq!(b.line_count(), 3);
        assert_eq!(b.line_starts, vec![0, 6, 12]);
    }

    #[test]
    fn set_text_resets_cursor_and_selection() {
        let mut b = Buffer::new_with_text("abc".into());
        b.cursor = 2;
        b.start_selection();
        b.set_text("x".into());
        assert_eq!(b.cursor, 0);
        assert!(b.selection.is_none());
        assert_eq!(b.text, "x");
    }

    #[test]
    fn line_count_single() {
        let b = Buffer::new_with_text("no newline".into());
        assert_eq!(b.line_count(), 1);
    }

    #[test]
    fn line_count_empty_string() {
        let b = Buffer::new();
        assert_eq!(b.line_count(), 1);
    }

    // --- line_col / byte_from_line_col -------------------------------------

    #[test]
    fn line_col_at_start() {
        let b = Buffer::new_with_text("abc\ndef".into());
        assert_eq!(b.line_col(0), (0, 0));
    }

    #[test]
    fn line_col_at_end_of_first_line() {
        let b = Buffer::new_with_text("abc\ndef".into());
        assert_eq!(b.line_col(3), (0, 3)); // points at newline
    }

    #[test]
    fn line_col_second_line() {
        let b = Buffer::new_with_text("abc\ndef".into());
        assert_eq!(b.line_col(6), (1, 2)); // 'f'? Wait, let's check
        // a(0) b(1) c(2) \n(3) d(4) e(5) f(6)
        // line_col(6): partition_point([0,4]) = 2, line=1, col=text[4..6].chars() = "ef" = 2
        assert_eq!(b.line_col(4), (1, 0)); // 'd' = start of line 1
        assert_eq!(b.line_col(5), (1, 1)); // 'e'
    }

    #[test]
    fn line_col_beyond_len_clamps() {
        let b = Buffer::new_with_text("hi".into());
        assert_eq!(b.line_col(100), (0, 2));
    }

    #[test]
    fn line_col_multibyte_col() {
        // 'café' — é is 2 bytes, col should still be 4
        let b = Buffer::new_with_text("café".into());
        assert_eq!(b.line_col(5), (0, 4));
    }

    #[test]
    fn byte_from_line_col_first_line() {
        let b = Buffer::new_with_text("abc\ndef".into());
        assert_eq!(b.byte_from_line_col(0, 1), 1);
    }

    #[test]
    fn byte_from_line_col_second_line_col_0() {
        let b = Buffer::new_with_text("abc\ndef".into());
        assert_eq!(b.byte_from_line_col(1, 0), 4);
    }

    #[test]
    fn byte_from_line_col_clamps_to_line_end() {
        let b = Buffer::new_with_text("abc\ndef".into());
        assert_eq!(b.byte_from_line_col(0, 99), 3); // clamped to end of line (before \n)
    }

    #[test]
    fn byte_from_line_col_multibyte() {
        let b = Buffer::new_with_text("café".into());
        // 'c'(0) 'a'(1) 'f'(2) 'é'(3-4)
        assert_eq!(b.byte_from_line_col(0, 3), 3); // 'c' = col 3 (0-indexed)
    }

    #[test]
    fn byte_from_line_col_multi_line_multibyte() {
        let b = Buffer::new_with_text("hé\nllo".into());
        // h(0) é(1-2) \n(3) l(4) l(5) o(6)
        assert_eq!(b.byte_from_line_col(0, 1), 1); // é = byte 1
        assert_eq!(b.byte_from_line_col(1, 2), 6);
    }

    // --- Cursor movement ----------------------------------------------------

    #[test]
    fn move_left_zero_stays() {
        let b = Buffer::new_with_text("abc".into());
        assert_eq!(b.move_left(0), 0);
    }

    #[test]
    fn move_left_ok() {
        let b = Buffer::new_with_text("abc".into());
        assert_eq!(b.move_left(2), 1);
    }

    #[test]
    fn move_left_multibyte_skips_trailing() {
        let b = Buffer::new_with_text("café".into());
        // é is bytes 3-4
        assert_eq!(b.move_left(5), 3); // lands on byte 3 (start of é)
        assert_eq!(b.move_left(3), 2); // 'f'
    }

    #[test]
    fn move_right_at_len_stays() {
        let b = Buffer::new_with_text("abc".into());
        assert_eq!(b.move_right(3), 3);
    }

    #[test]
    fn move_right_ok() {
        let b = Buffer::new_with_text("abc".into());
        assert_eq!(b.move_right(1), 2);
    }

    #[test]
    fn move_right_multibyte() {
        let b = Buffer::new_with_text("café".into());
        assert_eq!(b.move_right(3), 5); // skip over é (2 bytes)
    }

    #[test]
    fn move_up_stays_on_first_line() {
        let b = Buffer::new_with_text("a\nb".into());
        assert_eq!(b.move_up(0), 0);
    }

    #[test]
    fn move_up_ok() {
        let b = Buffer::new_with_text("abc\ndef".into());
        let result = b.move_up(5); // cursor on 'e' (line 1, col 1)
        assert_eq!(b.line_col(result), (0, 1)); // 'b'
    }

    #[test]
    fn move_up_shorter_line_clamps() {
        let b = Buffer::new_with_text("a\nbcde".into());
        // cursor at 'c' (line 1, col 1), prev line only has 'a' (1 char) → clamps to col 0
        // but byte_from_line_col(0, 1) → line_end(0)=1, text[0..1]="a", .nth(1) → None → unwrap_or(1)
        let result = b.move_up(3); // byte 3 = 'c'
        assert_eq!(b.line_col(result), (0, 1)); // lands at byte 1 (\n) but col 1 on line 0
    }

    #[test]
    fn move_down_stays_on_last_line() {
        let b = Buffer::new_with_text("a\nb".into());
        assert_eq!(b.move_down(3), 3); // text.len() = 3
    }

    #[test]
    fn move_down_ok() {
        let b = Buffer::new_with_text("abc\ndef".into());
        let result = b.move_down(1); // cursor on 'b' (line 0, col 1)
        assert_eq!(b.line_col(result), (1, 1)); // 'e'
    }

    #[test]
    fn move_down_shorter_line_clamps() {
        let b = Buffer::new_with_text("abc\nd".into());
        let result = b.move_down(2); // cursor on 'c' (line 0, col 2)
        assert_eq!(b.line_col(result), (1, 1)); // 'd' (col 1, shrunk from 2)
    }

    #[test]
    fn move_line_start_zero() {
        let b = Buffer::new_with_text("abc\n".into());
        assert_eq!(b.move_line_start(3), 0);
    }

    #[test]
    fn move_line_start_second_line() {
        let b = Buffer::new_with_text("ab\ncd".into());
        assert_eq!(b.move_line_start(4), 3);
    }

    #[test]
    fn move_line_end_first_line() {
        let b = Buffer::new_with_text("abc\nd".into());
        // 3 chars on line 0 → col 3 → byte 3 (right before \n)
        // Actually move_line_end gives you the last char position, so col = char count = 3 → byte 3
        assert_eq!(b.move_line_end(0), 3);
    }

    #[test]
    fn move_line_end_last_line() {
        let b = Buffer::new_with_text("a\nbc".into());
        assert_eq!(b.move_line_end(4), 4); // end of text is also end of line
    }

    #[test]
    fn move_first_nonblank_from_blank() {
        let b = Buffer::new_with_text("   abc".into());
        assert_eq!(b.move_first_nonblank(5), 3);
    }

    #[test]
    fn move_first_nonblank_already_at_text() {
        let b = Buffer::new_with_text("abc".into());
        assert_eq!(b.move_first_nonblank(0), 0);
    }

    #[test]
    fn move_first_nonblank_all_whitespace_stays() {
        let b = Buffer::new_with_text("   ".into());
        assert_eq!(b.move_first_nonblank(0), 0);
    }

    // --- Word movement ------------------------------------------------------

    #[test]
    fn move_word_forward_basic() {
        let b = Buffer::new_with_text("hello world".into());
        assert_eq!(b.move_word_forward(0), 6); // 'w'
    }

    #[test]
    fn move_word_forward_at_end_stays() {
        let b = Buffer::new_with_text("hi".into());
        assert_eq!(b.move_word_forward(2), 2);
    }

    #[test]
    fn move_word_forward_skips_punct() {
        let b = Buffer::new_with_text("abc,def".into());
        // moves to start of next word-class: keyword→other (comma at byte 3)
        assert_eq!(b.move_word_forward(0), 3); // comma
        assert_eq!(b.move_word_forward(7), 7);
    }

    #[test]
    fn move_word_back_basic() {
        let b = Buffer::new_with_text("hello world".into());
        assert_eq!(b.move_word_back(11), 6); // start of 'world'
    }

    #[test]
    fn move_word_back_at_zero_stays() {
        let b = Buffer::new_with_text("hi".into());
        assert_eq!(b.move_word_back(0), 0);
    }

    #[test]
    fn move_word_back_mid_word() {
        let b = Buffer::new_with_text("hello world".into());
        assert_eq!(b.move_word_back(8), 6); // 'wor|ld' → start of world
    }

    #[test]
    fn move_word_end_basic() {
        let b = Buffer::new_with_text("hello world".into());
        assert_eq!(b.move_word_end(0), 4); // 'o'
    }

    #[test]
    fn move_word_end_on_whitespace() {
        let b = Buffer::new_with_text("hello world".into());
        // cursor at ' ' (byte 5)
        assert_eq!(b.move_word_end(5), 10); // 'd'
    }

    #[test]
    fn move_word_end_at_end_stays() {
        let b = Buffer::new_with_text("hi".into());
        assert_eq!(b.move_word_end(2), 2);
    }

    #[test]
    fn move_big_word_forward_basic() {
        let b = Buffer::new_with_text("hello(world) foo".into());
        assert_eq!(b.move_big_word_forward(0), 13); // start of 'foo'
    }

    #[test]
    fn move_big_word_back_basic() {
        let b = Buffer::new_with_text("hello(world) foo".into());
        assert_eq!(b.move_big_word_back(14), 13); // start of 'foo'
    }

    #[test]
    fn move_big_word_end_basic() {
        let b = Buffer::new_with_text("hello(world) foo".into());
        assert_eq!(b.move_big_word_end(13), 16); // end of text (last WORD has no following ws)
    }

    #[test]
    fn move_word_forward_empty_buffer() {
        let b = Buffer::new();
        assert_eq!(b.move_word_forward(0), 0);
    }

    // --- Paragraph movement -------------------------------------------------

    #[test]
    fn move_paragraph_forward_basic() {
        let b = Buffer::new_with_text("para1\n\npara2".into());
        // cursor at start → skip first para → land at start of para2
        assert_eq!(b.move_paragraph_forward(0), 7);
    }

    #[test]
    fn move_paragraph_back_basic() {
        let b = Buffer::new_with_text("para1\n\npara2".into());
        // cursor at end → skip back → land at start of para2
        let pos = b.move_paragraph_back(12);
        assert_eq!(b.text[pos..].to_string(), "para2");
    }

    #[test]
    fn move_paragraph_forward_single_para() {
        let b = Buffer::new_with_text("just one para".into());
        assert_eq!(b.move_paragraph_forward(0), 13); // end of text
    }

    #[test]
    fn move_paragraph_back_at_zero() {
        let b = Buffer::new_with_text("a\n\nb".into());
        assert_eq!(b.move_paragraph_back(0), 0);
    }

    // --- Find / till chars --------------------------------------------------

    #[test]
    fn find_next_char_ok() {
        let b = Buffer::new_with_text("find the f".into());
        // skips the char at cursor, so first 'f' (byte 0) is skipped
        assert_eq!(b.find_next_char(0, 'f'), Some(9));
        assert_eq!(b.find_next_char(1, 'f'), Some(9));
    }

    #[test]
    fn find_next_char_missing() {
        let b = Buffer::new_with_text("abc".into());
        assert_eq!(b.find_next_char(0, 'z'), None);
    }

    #[test]
    fn find_prev_char_ok() {
        let b = Buffer::new_with_text("find the f".into());
        // f at byte 0 and byte 9; searching backward from 10 → finds byte 9
        assert_eq!(b.find_prev_char(10, 'f'), Some(9));
        // searching backward from 9 → skips byte 9, finds byte 0
        assert_eq!(b.find_prev_char(9, 'f'), Some(0));
    }

    #[test]
    fn find_prev_char_at_zero() {
        let b = Buffer::new_with_text("abc".into());
        assert_eq!(b.find_prev_char(0, 'a'), None);
    }

    #[test]
    fn find_till_next_char_ok() {
        let b = Buffer::new_with_text("abracadabra".into());
        // find next 'c' → byte 4, till = left 1 → byte 3
        assert_eq!(b.find_till_next_char(0, 'c'), Some(3));
    }

    #[test]
    fn find_till_prev_char_ok() {
        let b = Buffer::new_with_text("abracadabra".into());
        // find prev 'c' from end → byte 4, till = right 1 → byte 5
        assert_eq!(b.find_till_prev_char(11, 'c'), Some(5));
    }

    // --- Matching brackets --------------------------------------------------

    fn bracket_setup() -> Buffer {
        Buffer::new_with_text("(a[b]c)".into())
    }

    #[test]
    fn matching_bracket_forward() {
        let b = bracket_setup();
        assert_eq!(b.matching_bracket(0), Some(6)); // ( → )
    }

    #[test]
    fn matching_bracket_backward() {
        let b = bracket_setup();
        assert_eq!(b.matching_bracket(6), Some(0)); // ) → (
    }

    #[test]
    fn matching_bracket_nested() {
        let b = bracket_setup();
        assert_eq!(b.matching_bracket(2), Some(4)); // [ → ]
    }

    #[test]
    fn matching_bracket_none() {
        let b = Buffer::new_with_text("no brackets".into());
        assert_eq!(b.matching_bracket(0), None);
    }

    #[test]
    fn matching_bracket_unmatched() {
        let b = Buffer::new_with_text("({".into());
        assert_eq!(b.matching_bracket(0), None);
    }

    // --- Text objects -------------------------------------------------------

    // -- inner_word --

    #[test]
    fn inner_word_mid_word() {
        let b = Buffer::new_with_text("hello world".into());
        assert_eq!(b.inner_word(1), 0..5); // entire "hello"
    }

    #[test]
    fn inner_word_on_whitespace() {
        let b = Buffer::new_with_text("a   b".into());
        let r = b.inner_word(2);
        assert!(b.text[r].chars().all(|c| c.is_whitespace()));
    }

    #[test]
    fn inner_word_punct() {
        let b = Buffer::new_with_text("abc,def".into());
        assert_eq!(b.inner_word(3), 3..4); // just the comma
    }

    #[test]
    fn inner_word_empty() {
        let b = Buffer::new();
        assert_eq!(b.inner_word(0), 0..0);
    }

    // -- a_word --

    #[test]
    fn a_word_includes_trailing_ws() {
        let b = Buffer::new_with_text("hello world".into());
        assert_eq!(b.a_word(1), 0..6); // "hello "
    }

    #[test]
    fn a_word_last_word_no_trailing_ws() {
        let b = Buffer::new_with_text("hello world".into());
        assert_eq!(b.a_word(8), 0..11); // includes leading whitespace since no trailing ws
    }

    #[test]
    fn a_word_on_ws_includes_leading_ws() {
        let b = Buffer::new_with_text("hello   world".into());
        // cursor on the 3 spaces (byte 6)
        let r = b.a_word(6);
        assert_eq!(&b.text[r], "hello   "); // includes preceding word + spaces
    }

    #[test]
    fn a_word_single_word() {
        let b = Buffer::new_with_text("hello".into());
        assert_eq!(b.a_word(2), 0..5);
    }

    // -- inner_big_word --

    #[test]
    fn inner_big_word_mid() {
        let b = Buffer::new_with_text("hello(world) foo".into());
        // WORDs: "hello(world)" then "foo"
        assert_eq!(b.inner_big_word(1), 0..12);
    }

    #[test]
    fn inner_big_word_empty() {
        let b = Buffer::new();
        assert_eq!(b.inner_big_word(0), 0..0);
    }

    // -- a_big_word --

    #[test]
    fn a_big_word_includes_trailing_ws() {
        let b = Buffer::new_with_text("hello(world) foo".into());
        assert_eq!(b.a_big_word(1), 0..13); // "hello(world) " includes trailing ws
    }

    // -- inner_paragraph --

    #[test]
    fn inner_paragraph_mid_text() {
        let b = Buffer::new_with_text("p1\n\np2\n\np3".into());
        let r = b.inner_paragraph(4);
        assert_eq!(&b.text[r], "p1\n\np2");
    }

    // -- a_paragraph --

    #[test]
    fn a_paragraph_includes_blank_lines() {
        let b = Buffer::new_with_text("p1\n\np2".into());
        let r = b.a_paragraph(4);
        // starts at first para, includes blank lines up to next para
        assert!(r.start <= 4);
        assert!(r.end > 4);
    }

    // -- inner_pair / a_pair --

    #[test]
    fn inner_pair_on_open() {
        let b = Buffer::new_with_text("(hello)".into());
        assert_eq!(b.inner_pair(0, '(', ')'), Some(1..6));
    }

    #[test]
    fn inner_pair_on_close() {
        let b = Buffer::new_with_text("(hello)".into());
        assert_eq!(b.inner_pair(6, '(', ')'), Some(1..6));
    }

    #[test]
    fn inner_pair_inside() {
        let b = Buffer::new_with_text("(hello)".into());
        assert_eq!(b.inner_pair(3, '(', ')'), Some(1..6));
    }

    #[test]
    fn a_pair_includes_brackets() {
        let b = Buffer::new_with_text("(hello)".into());
        assert_eq!(b.a_pair(3, '(', ')'), Some(0..7));
    }

    #[test]
    fn inner_pair_nested() {
        let b = Buffer::new_with_text("(a(b)c)".into());
        // cursor on inner '(' → inner = a(b)
        let r = b.inner_pair(2, '(', ')');
        assert!(r.is_some());
        assert_eq!(&b.text[r.unwrap()], "b");
    }

    #[test]
    fn inner_pair_no_match() {
        let b = Buffer::new_with_text("no brackets".into());
        assert_eq!(b.inner_pair(0, '(', ')'), None);
    }

    // -- inner_quote / a_quote --

    #[test]
    fn inner_quote_basic() {
        let b = Buffer::new_with_text("\"hello\"".into());
        assert_eq!(b.inner_quote(3, '"'), Some(1..6));
    }

    #[test]
    fn inner_quote_outside() {
        let b = Buffer::new_with_text("\"hello\"".into());
        assert_eq!(b.inner_quote(0, '"'), Some(1..6));
    }

    #[test]
    fn a_quote_includes_quotes() {
        let b = Buffer::new_with_text("\"hello\"".into());
        assert_eq!(b.a_quote(3, '"'), Some(0..7));
    }

    #[test]
    fn inner_quote_no_match() {
        let b = Buffer::new_with_text("hello".into());
        assert_eq!(b.inner_quote(0, '"'), None);
    }

    // --- Editing ------------------------------------------------------------

    #[test]
    fn insert_basic() {
        let mut b = Buffer::new_with_text("helo".into());
        b.insert(3, "l");
        assert_eq!(b.text, "hello");
        assert_eq!(b.cursor, 4);
    }

    #[test]
    fn insert_at_end() {
        let mut b = Buffer::new_with_text("hi".into());
        b.insert(2, "!");
        assert_eq!(b.text, "hi!");
    }

    #[test]
    fn insert_updates_line_starts() {
        let mut b = Buffer::new_with_text("ab".into());
        b.insert(1, "\n");
        assert_eq!(b.line_count(), 2);
    }

    #[test]
    fn delete_basic() {
        let mut b = Buffer::new_with_text("hello".into());
        let deleted = b.delete(1..4);
        assert_eq!(deleted, "ell");
        assert_eq!(b.text, "ho");
    }

    #[test]
    fn delete_empty_range() {
        let mut b = Buffer::new_with_text("hello".into());
        assert!(b.delete(2..2).is_empty());
        assert_eq!(b.text, "hello");
    }

    #[test]
    fn delete_rebuilds_starts() {
        let mut b = Buffer::new_with_text("a\nb\nc".into());
        b.delete(1..4); // removes "\nb\n"
        assert_eq!(b.text, "ac");
        assert_eq!(b.line_count(), 1);
    }

    #[test]
    fn replace_char_basic() {
        let mut b = Buffer::new_with_text("cat".into());
        b.replace_char(1, 'o');
        assert_eq!(b.text, "cot");
    }

    #[test]
    fn replace_char_out_of_bounds() {
        let mut b = Buffer::new_with_text("hi".into());
        b.replace_char(10, 'x');
        assert_eq!(b.text, "hi"); // unchanged
    }

    #[test]
    fn toggle_case_upper_to_lower() {
        let mut b = Buffer::new_with_text("Hello".into());
        b.toggle_case_at(0);
        assert_eq!(b.text, "hello");
        assert_eq!(b.cursor, 1);
    }

    #[test]
    fn toggle_case_lower_to_upper() {
        let mut b = Buffer::new_with_text("hello".into());
        b.toggle_case_at(0);
        assert_eq!(b.text, "Hello");
    }

    #[test]
    fn toggle_case_non_alpha_unchanged() {
        let mut b = Buffer::new_with_text("123".into());
        b.toggle_case_at(0);
        assert_eq!(b.text, "123");
        assert_eq!(b.cursor, 1);
    }

    #[test]
    fn toggle_case_sharp_s() {
        let mut b = Buffer::new_with_text("straße".into());
        // 'ß' is at byte 4 (s=0, t=1, r=2, a=3, ß=4-5, e=6), toggle → "SS"
        b.toggle_case_at(4);
        assert_eq!(b.text, "straSSe");
        // cursor = 4 + 2 = 6 (past the inserted "SS")
        assert_eq!(b.cursor, 6);
    }

    #[test]
    fn join_lines_basic() {
        let mut b = Buffer::new_with_text("abc\ndef".into());
        b.join_lines(0);
        assert_eq!(b.text, "abc def");
    }

    #[test]
    fn join_lines_last_line_noop() {
        let mut b = Buffer::new_with_text("a\nb\nc".into());
        b.join_lines(4); // cursor on last line
        assert_eq!(b.text, "a\nb\nc");
    }

    #[test]
    fn join_lines_removes_leading_ws() {
        let mut b = Buffer::new_with_text("abc\n   def".into());
        b.join_lines(0);
        assert_eq!(b.text, "abc def");
    }

    #[test]
    fn join_lines_uses_one_space() {
        let mut b = Buffer::new_with_text("abc\ndef".into());
        b.join_lines(0);
        assert_eq!(b.text, "abc def");
    }

    #[test]
    fn join_lines_no_extra_space_if_ends_with_space() {
        let mut b = Buffer::new_with_text("abc \ndef".into());
        b.join_lines(0);
        assert_eq!(b.text, "abc def");
    }

    // --- Snapshot / restore -------------------------------------------------

    #[test]
    fn snapshot_restore() {
        let mut b = Buffer::new_with_text("original".into());
        let snap = b.undo_snapshot();
        b.set_text("modified".into());
        b.restore_snapshot(&snap);
        assert_eq!(b.text, "original");
        assert_eq!(b.cursor, 0);
    }

    #[test]
    fn snapshot_independent() {
        let mut b = Buffer::new_with_text("first".into());
        let snap = b.undo_snapshot();
        b.set_text("second".into());
        b.restore_snapshot(&snap);
        assert_eq!(b.text, "first");

        // The snapshot itself should still be usable
        let snap2 = b.undo_snapshot();
        b.restore_snapshot(&snap2);
        assert_eq!(b.text, "first");
    }

    // --- Selection ----------------------------------------------------------

    #[test]
    fn selection_new() {
        let sel = Selection::new(5);
        assert_eq!(sel.anchor, 5);
        assert_eq!(sel.active, 5);
        assert_eq!(sel.mode, SelectionMode::Charwise);
    }

    #[test]
    fn selection_range_forward() {
        let sel = Selection {
            anchor: 2,
            active: 7,
            mode: SelectionMode::Charwise,
        };
        assert_eq!(sel.range(), 2..7);
    }

    #[test]
    fn selection_range_backward() {
        let sel = Selection {
            anchor: 7,
            active: 2,
            mode: SelectionMode::Charwise,
        };
        assert_eq!(sel.range(), 2..7);
    }

    #[test]
    fn buffer_start_selection() {
        let mut b = Buffer::new_with_text("hello".into());
        b.cursor = 3;
        b.start_selection();
        assert!(b.selection.is_some());
        let sel = b.selection.as_ref().unwrap();
        assert_eq!(sel.anchor, 3);
        assert_eq!(sel.active, 3);
    }

    #[test]
    fn buffer_clear_selection() {
        let mut b = Buffer::new_with_text("hello".into());
        b.cursor = 1;
        b.start_selection();
        b.clear_selection();
        assert!(b.selection.is_none());
    }

    #[test]
    fn buffer_selection_range() {
        let mut b = Buffer::new_with_text("hello world".into());
        b.cursor = 6;
        b.start_selection();
        // selection anchor is 6, active is 6
        assert_eq!(b.selection_range(), Some(6..6));
    }

    // --- Multi-byte UTF-8 edge cases ----------------------------------------

    #[test]
    fn move_left_emoji() {
        let b = Buffer::new_with_text("a😀c".into());
        // 😀 is 4 bytes
        assert_eq!(b.move_left(5), 1);
    }

    #[test]
    fn move_right_emoji() {
        let b = Buffer::new_with_text("a😀c".into());
        assert_eq!(b.move_right(1), 5);
    }

    #[test]
    fn line_col_emoji() {
        let b = Buffer::new_with_text("😀hi".into());
        // 😀 = bytes 0-3, h=4, i=5
        assert_eq!(b.line_col(5), (0, 2)); // col 2 = 'i'
    }

    #[test]
    fn byte_from_line_col_emoji() {
        let b = Buffer::new_with_text("a😀c".into());
        // col 0='a'(0), col 1='😀'(1), col 2='c'(5)
        assert_eq!(b.byte_from_line_col(0, 1), 1);
    }

    #[test]
    fn inner_word_emoji() {
        // Chinese chars are "keyword" class (alphanumeric-like in Vim world)
        let b = Buffer::new_with_text("中文测试".into());
        // "中文测试" = all keyword — the full string is one word
        let r = b.inner_word(0);
        assert_eq!(r, 0..12);
        assert_eq!(&b.text[r], "中文测试");
    }

    #[test]
    fn delete_multibyte() {
        let mut b = Buffer::new_with_text("a😀c".into());
        let deleted = b.delete(1..5);
        assert_eq!(deleted, "😀");
        assert_eq!(b.text, "ac");
    }

    #[test]
    fn replace_char_multibyte_with_ascii() {
        let mut b = Buffer::new_with_text("😀bc".into());
        b.replace_char(0, 'x');
        assert_eq!(b.text, "xbc");
    }

    // --- Edge cases & specials ----------------------------------------------

    #[test]
    fn byte_before_zero() {
        let b = Buffer::new_with_text("abc".into());
        assert_eq!(b.byte_before(0), 0);
    }

    #[test]
    fn byte_before_mid() {
        let b = Buffer::new_with_text("abc".into());
        assert_eq!(b.byte_before(2), 1);
    }

    #[test]
    fn byte_before_multibyte() {
        let b = Buffer::new_with_text("a😀c".into());
        assert_eq!(b.byte_before(5), 1); // 😀 is 4 bytes
    }

    #[test]
    fn current_line_basic() {
        let b = Buffer::new_with_text("hello\nworld".into());
        assert_eq!(b.current_line(), "hello");
    }

    #[test]
    fn line_text_out_of_range() {
        let b = Buffer::new();
        assert!(b.line_text(99).is_empty());
    }

    #[test]
    fn line_text_middle() {
        let b = Buffer::new_with_text("zero\none\ntwo".into());
        assert_eq!(b.line_text(1), "one");
    }

    #[test]
    fn set_cursor_clamps() {
        let mut b = Buffer::new_with_text("hi".into());
        b.set_cursor(100);
        assert_eq!(b.cursor, 2);
    }

    #[test]
    fn default_equals_new() {
        let b1 = Buffer::default();
        let b2 = Buffer::new();
        assert!(b1.text.is_empty());
        assert_eq!(b1.line_count(), b2.line_count());
    }

    #[test]
    fn selection_mode_debug_and_clone() {
        let sel = Selection::new(3);
        let cloned = sel.clone();
        assert_eq!(sel.anchor, cloned.anchor);
        assert_eq!(format!("{:?}", SelectionMode::Charwise), "Charwise");
        assert_eq!(format!("{:?}", SelectionMode::Linewise), "Linewise");
    }

    #[test]
    fn inner_big_word_on_ws() {
        let b = Buffer::new_with_text("a   b".into());
        // cursor in spaces → from cursor to end of whitespace
        let r = b.inner_big_word(2);
        assert_eq!(&b.text[r], "  "); // spaces from pos 2 to pos 4
    }

    #[test]
    fn a_big_word_trailing_ws() {
        let b = Buffer::new_with_text("hello@world foo".into());
        let r = b.a_big_word(1);
        assert_eq!(&b.text[r], "hello@world "); // includes trailing ws
    }

    #[test]
    fn inner_pair_on_square() {
        let b = Buffer::new_with_text("[test]".into());
        assert_eq!(b.inner_pair(0, '[', ']'), Some(1..5));
    }

    #[test]
    fn a_pair_on_curly() {
        let b = Buffer::new_with_text("{x}".into());
        assert_eq!(b.a_pair(1, '{', '}'), Some(0..3));
    }

    #[test]
    fn a_quote_outside() {
        let b = Buffer::new_with_text("'hi'".into());
        assert_eq!(b.a_quote(0, '\''), Some(0..4));
    }

    #[test]
    fn inner_quote_multiple_pairs() {
        let b = Buffer::new_with_text("\"a\" \"b\"".into());
        // cursor in first pair
        assert_eq!(b.inner_quote(1, '"'), Some(1..2));
        // cursor inside second pair (byte 5 = 'b')
        assert_eq!(b.inner_quote(5, '"'), Some(5..6));
    }

    #[test]
    fn replace_on_non_boundary_stays() {
        let mut b = Buffer::new_with_text("a😀c".into());
        // byte 2 is NOT a char boundary (inside 😀)
        b.replace_char(2, 'x');
        assert_eq!(b.text, "a😀c"); // unchanged
    }

    #[test]
    fn toggle_case_on_non_boundary_stays() {
        let mut b = Buffer::new_with_text("a😀c".into());
        b.toggle_case_at(2);
        assert_eq!(b.text, "a😀c");
    }

    #[test]
    fn join_single_line_noop() {
        let mut b = Buffer::new_with_text("hello".into());
        b.join_lines(0);
        assert_eq!(b.text, "hello");
    }
}
