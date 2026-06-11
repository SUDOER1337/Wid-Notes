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
}

impl Buffer {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
            selection: None,
        }
    }

    pub fn new_with_text(text: String) -> Self {
        Self {
            text,
            cursor: 0,
            selection: None,
        }
    }

    pub fn line_col(&self, pos: usize) -> (usize, usize) {
        if self.text.is_empty() {
            return (0, 0);
        }
        let pos = pos.min(self.text.len());
        if !self.text.is_char_boundary(pos) {
            return self.line_col(pos.saturating_sub(1));
        }
        let before = &self.text[..pos];
        let line = before.matches('\n').count();
        let last_nl = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
        let col = self.text[last_nl..pos].chars().count();
        (line, col)
    }

    pub fn byte_from_line_col(&self, line: usize, col: usize) -> usize {
        // Walk newlines to find the start of the target line (avoids Vec allocation)
        let mut byte_pos = 0;
        for _ in 0..line {
            match self.text[byte_pos..].find('\n') {
                Some(i) => byte_pos += i + 1,
                None => return self.text.len(),
            }
        }
        // Find the character at the given column within the line
        let line_text = &self.text[byte_pos..];
        let line_end = line_text.find('\n').unwrap_or(line_text.len());
        let actual_col = line_text[..line_end]
            .char_indices()
            .nth(col)
            .map(|(i, _)| i)
            .unwrap_or(line_end);
        byte_pos + actual_col
    }

    pub fn line_count(&self) -> usize {
        if self.text.is_empty() {
            1
        } else {
            self.text.matches('\n').count() + 1
        }
    }

    pub fn current_line(&self) -> &str {
        let (line, _) = self.line_col(self.cursor);
        self.line_text(line)
    }

    pub fn line_text(&self, idx: usize) -> &str {
        self.text.split('\n').nth(idx).unwrap_or("")
    }

    pub fn line_start(&self, idx: usize) -> usize {
        let mut pos = 0;
        for _ in 0..idx {
            match self.text[pos..].find('\n') {
                Some(i) => pos += i + 1,
                None => return self.text.len(),
            }
        }
        pos
    }

    pub fn line_end(&self, idx: usize) -> usize {
        let start = self.line_start(idx);
        match self.text[start..].find('\n') {
            Some(i) => start + i,
            None => self.text.len(),
        }
    }

    pub fn current_line_idx(&self) -> usize {
        self.line_col(self.cursor).0
    }

    pub fn current_col(&self) -> usize {
        self.line_col(self.cursor).1
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
        let col = text.find(|c: char| !c.is_whitespace()).unwrap_or(0);
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
        if i == cursor {
            self.text.len()
        } else {
            i
        }
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

    pub fn a_word(&self, cursor: usize) -> Range<usize> {
        let inner = self.inner_word(cursor);
        if inner.start == inner.end {
            return inner;
        }
        // Try to include trailing whitespace after the word
        let trailing_ws_end = self.text[inner.end..]
            .find(|c: char| !c.is_whitespace())
            .map(|i| inner.end + i)
            .unwrap_or(self.text.len());
        if trailing_ws_end > inner.end {
            return inner.start..trailing_ws_end;
        }
        // No trailing whitespace — include leading whitespace before the word
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

    pub fn inner_big_word(&self, cursor: usize) -> Range<usize> {
        if self.text.is_empty() {
            return 0..0;
        }
        let pos = cursor.min(self.text.len().saturating_sub(1));
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
        let inner = self.inner_big_word(cursor);
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

    pub fn inner_pair(&self, cursor: usize, open: char, close: char) -> Option<Range<usize>> {
        let pos = cursor.min(self.text.len().saturating_sub(1));
        let ch = self.text[pos..].chars().next()?;
        if ch == open {
            if let Some(end) = self.find_close(pos + 1, open, close) {
                return Some(pos + 1..end);
            }
        } else if ch == close {
            if let Some(start) = self.find_open(pos, open, close) {
                return Some(start + 1..pos);
            }
        }
        self.text[pos..]
            .char_indices()
            .find(|&(_, c)| c == open || c == close)
            .and_then(|(i, c)| {
                if c == open {
                    self.find_close(pos + i + 1, open, close)
                        .map(|e| pos + i + 1..e)
                } else {
                    self.find_open(pos + i, open, close).map(|s| s + 1..pos + i)
                }
            })
    }

    pub fn a_pair(&self, cursor: usize, open: char, close: char) -> Option<Range<usize>> {
        let pos = cursor.min(self.text.len().saturating_sub(1));
        let ch = self.text[pos..].chars().next()?;
        if ch == open {
            if let Some(end) = self.find_close(pos + 1, open, close) {
                return Some(pos..end + 1);
            }
        } else if ch == close {
            if let Some(start) = self.find_open(pos, open, close) {
                return Some(start..pos + 1);
            }
        }
        self.text[pos..]
            .char_indices()
            .find(|&(_, c)| c == open || c == close)
            .and_then(|(i, c)| {
                if c == open {
                    self.find_close(pos + i + 1, open, close)
                        .map(|e| pos + i..e + 1)
                } else {
                    self.find_open(pos + i, open, close).map(|s| s..pos + i + 1)
                }
            })
    }

    pub fn inner_quote(&self, cursor: usize, quote: char) -> Option<Range<usize>> {
        let pos = cursor.min(self.text.len().saturating_sub(1));
        if pos + 1 > self.text.len() {
            return None;
        }
        let before = &self.text[..pos + 1];
        let after = &self.text[pos..];
        let open = before.rfind(quote)?;
        let close = after[1..].find(quote).map(|i| pos + 1 + i)?;
        if close > open {
            Some(open + 1..close)
        } else {
            None
        }
    }

    pub fn a_quote(&self, cursor: usize, quote: char) -> Option<Range<usize>> {
        let pos = cursor.min(self.text.len().saturating_sub(1));
        if pos + 1 > self.text.len() {
            return None;
        }
        let before = &self.text[..pos + 1];
        let after = &self.text[pos..];
        let open = before.rfind(quote)?;
        let close = after[1..].find(quote).map(|i| pos + 1 + i)?;
        if close > open {
            Some(open..close + 1)
        } else {
            None
        }
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
    }

    pub fn delete(&mut self, range: Range<usize>) -> String {
        if range.start >= range.end {
            return String::new();
        }
        let deleted = self.text[range.start..range.end].to_string();
        self.text.drain(range.start..range.end);
        self.cursor = range.start.min(self.text.len());
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
        let ws_to_remove = next_start - end - 1 + (next_text.len() - trimmed.len());
        let last_char_pos = self.byte_before(end);
        let space = if last_char_pos < end && self.text[last_char_pos..end].starts_with(' ') {
            ""
        } else {
            " "
        };
        self.text.drain(end..end + ws_to_remove);
        self.text.insert_str(end, space);
        self.cursor = end + space.len();
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
