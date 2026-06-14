mod key;

pub use key::KeyPress;

use crate::editor::{Buffer, SelectionMode, Snapshot};
use std::collections::{HashMap, VecDeque};
use std::ops::Range;

/// State to preserve when switching pages (undo stacks, registers, dirty tracking)
pub struct PageState {
    pub undo_stack: VecDeque<Snapshot>,
    pub redo_stack: VecDeque<Snapshot>,
    pub registers: Registers,
    pub saved_snapshot: Option<Snapshot>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    Normal,
    Insert,
    Visual(SelectionMode),
    CommandLine,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Operator {
    Delete,
    Yank,
    Change,
    Indent,
    Outdent,
    ToggleCase,
    Lowercase,
    Uppercase,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Pending {
    None,
    AwaitingDouble(char),               // d→d, y→y, g→g
    AwaitingMotion(Operator),           // d{motion}, y{motion}
    AwaitingTextObject(Operator, bool), // d{i,a}, y{i,a} — true=inner, false=outer
    AwaitingFindChar,
    AwaitingTillChar,
    AwaitingFindBackChar,
    AwaitingTillBackChar,
    AwaitingReplaceChar,
    AwaitingRegister,
}

#[derive(Clone, Copy, Debug)]
struct FindState {
    ch: char,
    reverse: bool,
    till: bool,
}

#[derive(Clone)]
pub(crate) struct Registers {
    unnamed: String,
    named: HashMap<char, String>,
    small_delete: String,
    last_insert: String,
}

impl Registers {
    fn new() -> Self {
        Self {
            unnamed: String::new(),
            named: HashMap::new(),
            small_delete: String::new(),
            last_insert: String::new(),
        }
    }

    fn set(&mut self, name: Option<char>, value: String) {
        match name {
            None | Some('"') => self.unnamed = value,
            Some('-') => self.small_delete = value,
            Some(c) if c.is_ascii_lowercase() => {
                self.named.insert(c, value.clone());
                self.unnamed = value;
            }
            _ => self.unnamed = value,
        }
    }

    fn get(&self, name: Option<char>) -> &str {
        match name {
            None | Some('"') => &self.unnamed,
            Some('-') => &self.small_delete,
            Some(c) if c.is_ascii_lowercase() => self.named.get(&c).unwrap_or(&self.unnamed),
            Some('.') => &self.last_insert,
            _ => &self.unnamed,
        }
    }
}

#[derive(Clone, Debug)]
struct InsertChange {
    text: String,
    cursor_before: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CommandAction {
    None,
    Save,
    Quit,
    SaveQuit,
    ForceQuit,
    SaveForceQuit,
    Open(String),
}

#[derive(Clone, Debug)]
enum ChangeRecord {
    Insert(InsertChange),
    Delete { text: String },
    Replace { pos: usize, new: char },
    Join { pos: usize },
    ToggleCase { pos: usize },
    Indent { line: usize, count: usize },
    Outdent { line: usize, count: usize },
    ToggleCaseRange(std::ops::Range<usize>),
    LowercaseRange(std::ops::Range<usize>),
    UppercaseRange(std::ops::Range<usize>),
    Yank,
}

const MAX_UNDO: usize = 200;
/// Number of lines to scroll with Ctrl+F / Ctrl+B.
const SCROLL_LINES: usize = 20;

pub struct VimState {
    pub mode: Mode,
    count: Option<usize>,
    count_str: String,
    operator: Option<Operator>,
    op_register: Option<char>,
    pending: Pending,
    last_find: Option<FindState>,
    undo_stack: VecDeque<Snapshot>,
    redo_stack: VecDeque<Snapshot>,
    registers: Registers,
    last_change: Option<ChangeRecord>,
    pub command_line: String,
    pub search_pattern: String,
    last_search_forward: bool,
    last_insert_text: String,
    last_insert_len: usize,
    insert_cursor_start: usize,
    pub command_action: CommandAction,
    saved_snapshot: Option<Snapshot>,
}

impl VimState {
    pub fn new() -> Self {
        Self {
            mode: Mode::Normal,
            count: None,
            count_str: String::new(),
            operator: None,
            op_register: None,
            pending: Pending::None,
            last_find: None,
            undo_stack: VecDeque::with_capacity(MAX_UNDO),
            redo_stack: VecDeque::new(),
            registers: Registers::new(),
            last_change: None,
            command_line: String::new(),
            search_pattern: String::new(),
            last_search_forward: true,
            last_insert_text: String::new(),
            last_insert_len: 0,
            insert_cursor_start: 0,
            command_action: CommandAction::None,
            saved_snapshot: None,
        }
    }

    /// Take the per-page state (undo stacks, registers, saved snapshot) for preservation
    pub fn take_page_state(&mut self) -> PageState {
        PageState {
            undo_stack: std::mem::take(&mut self.undo_stack),
            redo_stack: std::mem::take(&mut self.redo_stack),
            registers: std::mem::replace(&mut self.registers, Registers::new()),
            saved_snapshot: self.saved_snapshot.take(),
        }
    }

    /// Restore a previously preserved page state
    pub fn restore_page_state(&mut self, state: PageState) {
        self.undo_stack = state.undo_stack;
        self.redo_stack = state.redo_stack;
        self.registers = state.registers;
        self.saved_snapshot = state.saved_snapshot;
    }

    pub fn handle_key(&mut self, key: KeyPress, buffer: &mut Buffer) {
        match self.mode {
            Mode::Normal => self.handle_normal(key, buffer),
            Mode::Insert => self.handle_insert(key, buffer),
            Mode::Visual(_) => self.handle_visual(key, buffer),
            Mode::CommandLine => self.handle_command_line(key, buffer),
        }
    }

    // ── Normal mode ──

    fn handle_normal(&mut self, key: KeyPress, buffer: &mut Buffer) {
        // Parse accumulated count before any dispatch
        if !self.count_str.is_empty() && self.count.is_none() {
            self.count = self.count_str.parse::<usize>().ok();
        }

        // Handle pending states first
        match self.pending {
            Pending::AwaitingDouble(c) => {
                self.pending = Pending::None;
                if key.is_char(c) {
                    match c {
                        'd' => self.delete_line(buffer),
                        'y' => self.yank_line(buffer),
                        'g' => self.move_to_line(1, buffer),
                        '>' => self.indent_line(buffer),
                        '<' => self.outdent_line(buffer),
                        _ => {}
                    }
                    return;
                }
                // g~, gu, gU via g + ~/u/U
                if c == 'g' {
                    match key {
                        KeyPress::Char('~') => {
                            self.operator = Some(Operator::ToggleCase);
                            self.pending = Pending::AwaitingMotion(Operator::ToggleCase);
                            return;
                        }
                        KeyPress::Char('u') => {
                            self.operator = Some(Operator::Lowercase);
                            self.pending = Pending::AwaitingMotion(Operator::Lowercase);
                            return;
                        }
                        KeyPress::Char('U') => {
                            self.operator = Some(Operator::Uppercase);
                            self.pending = Pending::AwaitingMotion(Operator::Uppercase);
                            return;
                        }
                        _ => {}
                    }
                }
                // Fall through — transition to AwaitingMotion if operator is active
                if let Some(op) = self.operator {
                    self.pending = Pending::AwaitingMotion(op);
                    self.handle_normal(key, buffer);
                    return;
                }
                self.pending = Pending::None;
            }
            Pending::AwaitingMotion(op) => {
                // i/a within AwaitingMotion redirect to text object
                match key {
                    KeyPress::Char('i') => {
                        self.pending = Pending::AwaitingTextObject(op, true);
                        return;
                    }
                    KeyPress::Char('a') => {
                        self.pending = Pending::AwaitingTextObject(op, false);
                        return;
                    }
                    _ => {
                        if let Some(motion) = self.resolve_motion(key, buffer) {
                            self.execute_operator(op, motion, buffer);
                        }
                        self.pending = Pending::None;
                        self.operator = None;
                        self.count = None;
                        self.count_str.clear();
                        return;
                    }
                }
            }
            Pending::AwaitingTextObject(op, is_inner) => {
                if let Some(range) = self.resolve_text_object(key, buffer, is_inner) {
                    self.execute_operator_on_range(op, range, buffer);
                }
                self.pending = Pending::None;
                self.operator = None;
                self.count = None;
                self.count_str.clear();
                return;
            }
            Pending::AwaitingFindChar => {
                self.pending = Pending::None;
                if let KeyPress::Char(c) = key {
                    self.last_find = Some(FindState {
                        ch: c,
                        reverse: false,
                        till: false,
                    });
                    let count = self.count.unwrap_or(1);
                    let mut pos = buffer.cursor;
                    for _ in 0..count {
                        match buffer.find_next_char(pos, c) {
                            Some(p) => pos = p,
                            None => break,
                        }
                    }
                    buffer.set_cursor(pos);
                }
                self.count = None;
                self.count_str.clear();
                return;
            }
            Pending::AwaitingTillChar => {
                self.pending = Pending::None;
                if let KeyPress::Char(c) = key {
                    self.last_find = Some(FindState {
                        ch: c,
                        reverse: false,
                        till: true,
                    });
                    let count = self.count.unwrap_or(1);
                    let mut pos = buffer.cursor;
                    for _ in 0..count {
                        match buffer.find_till_next_char(pos, c) {
                            Some(p) => pos = p,
                            None => break,
                        }
                    }
                    buffer.set_cursor(pos);
                }
                self.count = None;
                self.count_str.clear();
                return;
            }
            Pending::AwaitingFindBackChar => {
                self.pending = Pending::None;
                if let KeyPress::Char(c) = key {
                    self.last_find = Some(FindState {
                        ch: c,
                        reverse: true,
                        till: false,
                    });
                    let count = self.count.unwrap_or(1);
                    let mut pos = buffer.cursor;
                    for _ in 0..count {
                        match buffer.find_prev_char(pos, c) {
                            Some(p) => pos = p,
                            None => break,
                        }
                    }
                    buffer.set_cursor(pos);
                }
                self.count = None;
                self.count_str.clear();
                return;
            }
            Pending::AwaitingTillBackChar => {
                self.pending = Pending::None;
                if let KeyPress::Char(c) = key {
                    self.last_find = Some(FindState {
                        ch: c,
                        reverse: true,
                        till: true,
                    });
                    let count = self.count.unwrap_or(1);
                    let mut pos = buffer.cursor;
                    for _ in 0..count {
                        match buffer.find_till_prev_char(pos, c) {
                            Some(p) => pos = p,
                            None => break,
                        }
                    }
                    buffer.set_cursor(pos);
                }
                self.count = None;
                self.count_str.clear();
                return;
            }
            Pending::AwaitingReplaceChar => {
                self.pending = Pending::None;
                if let KeyPress::Char(c) = key {
                    self.save_undo(buffer);
                    buffer.replace_char(buffer.cursor, c);
                    log::info!("Replace char with '{}'", c);
                    let mc = ChangeRecord::Replace {
                        pos: buffer.cursor,
                        new: c,
                    };
                    self.last_change = Some(mc);
                }
                self.count = None;
                self.count_str.clear();
                return;
            }
            Pending::AwaitingRegister => {
                self.pending = Pending::None;
                if let KeyPress::Char(c) = key {
                    self.op_register = Some(c);
                }
                return;
            }
            Pending::None => {}
        }

        // Accumulate count
        if let KeyPress::Char(c) = key {
            if c.is_ascii_digit() && c != '0' || !self.count_str.is_empty() && c == '0' {
                self.count_str.push(c);
                return;
            }
        }

        match key {
            KeyPress::Escape => {
                buffer.clear_selection();
                self.count = None;
                self.count_str.clear();
                self.operator = None;
                self.pending = Pending::None;
            }
            KeyPress::Char('i') => {
                if let Some(op) = self.operator {
                    self.pending = Pending::AwaitingTextObject(op, true);
                } else {
                    self.save_undo(buffer);
                    self.enter_insert_mode(buffer);
                }
            }
            KeyPress::Char('I') => {
                let pos = buffer.move_first_nonblank(buffer.cursor);
                buffer.set_cursor(pos);
                self.save_undo(buffer);
                self.enter_insert_mode(buffer);
            }
            KeyPress::Char('a') => {
                if let Some(op) = self.operator {
                    self.pending = Pending::AwaitingTextObject(op, false);
                } else {
                    let pos = buffer.move_right(buffer.cursor);
                    buffer.set_cursor(pos);
                    self.save_undo(buffer);
                    self.enter_insert_mode(buffer);
                }
            }
            KeyPress::Char('A') => {
                let pos = buffer.move_line_end(buffer.cursor);
                buffer.set_cursor(pos);
                self.save_undo(buffer);
                self.enter_insert_mode(buffer);
            }
            KeyPress::Char('o') => {
                self.save_undo(buffer);
                let (line, _) = buffer.line_col(buffer.cursor);
                let end = buffer.line_end(line);
                let pos = if end == buffer.text.len() {
                    end
                } else {
                    end + 1
                };
                buffer.insert(pos, "\n");
                buffer.set_cursor(pos + 1);
                self.enter_insert_mode(buffer);
            }
            KeyPress::Char('O') => {
                self.save_undo(buffer);
                let (line, _) = buffer.line_col(buffer.cursor);
                let start = buffer.line_start(line);
                buffer.insert(start, "\n");
                buffer.set_cursor(start);
                self.enter_insert_mode(buffer);
            }
            KeyPress::Char('v') => {
                buffer.start_selection();
                self.mode = Mode::Visual(SelectionMode::Charwise);
                log::info!("Mode → VISUAL");
            }
            KeyPress::Char('V') => {
                let (line, _) = buffer.line_col(buffer.cursor);
                let start = buffer.line_start(line);
                let end = buffer.line_end(line);
                buffer.start_selection();
                if let Some(ref mut s) = buffer.selection {
                    s.mode = SelectionMode::Linewise;
                    s.anchor = start;
                    s.active = end;
                }
                self.mode = Mode::Visual(SelectionMode::Linewise);
                log::info!("Mode → VISUAL LINE");
            }
            KeyPress::Char(':') => {
                self.mode = Mode::CommandLine;
                self.command_line.clear();
                self.command_line.push(':');
                log::info!("Mode → CMD");
            }
            KeyPress::Char('/') => {
                self.mode = Mode::CommandLine;
                self.command_line.clear();
                self.command_line.push('/');
                log::info!("Mode → CMD (search forward)");
            }
            KeyPress::Char('?') => {
                self.mode = Mode::CommandLine;
                self.command_line.clear();
                self.command_line.push('?');
                log::info!("Mode → CMD (search backward)");
            }
            KeyPress::Char('n') => {
                let pattern = self.search_pattern.clone();
                let forward = self.last_search_forward;
                if !pattern.is_empty() {
                    self.search_in_buffer(&pattern, forward, buffer);
                }
                self.count = None;
                self.count_str.clear();
            }
            KeyPress::Char('N') => {
                let pattern = self.search_pattern.clone();
                let forward = self.last_search_forward;
                if !pattern.is_empty() {
                    self.search_in_buffer(&pattern, !forward, buffer);
                }
                self.count = None;
                self.count_str.clear();
            }
            KeyPress::Char('h') | KeyPress::ArrowLeft => {
                let count = self.count.unwrap_or(1);
                let mut pos = buffer.cursor;
                for _ in 0..count {
                    pos = buffer.move_left(pos);
                }
                buffer.set_cursor(pos);
                self.count = None;
                self.count_str.clear();
            }
            KeyPress::Char('j') | KeyPress::ArrowDown => {
                let count = self.count.unwrap_or(1);
                let mut pos = buffer.cursor;
                for _ in 0..count {
                    pos = buffer.move_down(pos);
                }
                buffer.set_cursor(pos);
                self.count = None;
                self.count_str.clear();
            }
            KeyPress::Char('k') | KeyPress::ArrowUp => {
                let count = self.count.unwrap_or(1);
                let mut pos = buffer.cursor;
                for _ in 0..count {
                    pos = buffer.move_up(pos);
                }
                buffer.set_cursor(pos);
                self.count = None;
                self.count_str.clear();
            }
            KeyPress::Char('l') | KeyPress::ArrowRight => {
                let count = self.count.unwrap_or(1);
                let mut pos = buffer.cursor;
                for _ in 0..count {
                    pos = buffer.move_right(pos);
                }
                buffer.set_cursor(pos);
                self.count = None;
                self.count_str.clear();
            }
            KeyPress::Char('w') => {
                let count = self.count.unwrap_or(1);
                let mut pos = buffer.cursor;
                for _ in 0..count {
                    pos = buffer.move_word_forward(pos);
                }
                buffer.set_cursor(pos);
                self.count = None;
                self.count_str.clear();
            }
            KeyPress::Char('W') => {
                let count = self.count.unwrap_or(1);
                let mut pos = buffer.cursor;
                for _ in 0..count {
                    pos = buffer.move_big_word_forward(pos);
                }
                buffer.set_cursor(pos);
                self.count = None;
                self.count_str.clear();
            }
            KeyPress::Char('b') => {
                let count = self.count.unwrap_or(1);
                let mut pos = buffer.cursor;
                for _ in 0..count {
                    pos = buffer.move_word_back(pos);
                }
                buffer.set_cursor(pos);
                self.count = None;
                self.count_str.clear();
            }
            KeyPress::Char('B') => {
                let count = self.count.unwrap_or(1);
                let mut pos = buffer.cursor;
                for _ in 0..count {
                    pos = buffer.move_big_word_back(pos);
                }
                buffer.set_cursor(pos);
                self.count = None;
                self.count_str.clear();
            }
            KeyPress::Char('e') => {
                let count = self.count.unwrap_or(1);
                let mut pos = buffer.cursor;
                for _ in 0..count {
                    pos = buffer.move_word_end(pos);
                }
                buffer.set_cursor(pos);
                self.count = None;
                self.count_str.clear();
            }
            KeyPress::Char('E') => {
                let count = self.count.unwrap_or(1);
                let mut pos = buffer.cursor;
                for _ in 0..count {
                    pos = buffer.move_big_word_end(pos);
                }
                buffer.set_cursor(pos);
                self.count = None;
                self.count_str.clear();
            }
            KeyPress::Char('0') => {
                buffer.set_cursor(buffer.move_line_start(buffer.cursor));
                self.count = None;
                self.count_str.clear();
            }
            KeyPress::Char('$') | KeyPress::End => {
                let pos = buffer.move_line_end(buffer.cursor);
                buffer.set_cursor(pos);
                self.count = None;
                self.count_str.clear();
            }
            KeyPress::Char('^') | KeyPress::Home => {
                let pos = buffer.move_first_nonblank(buffer.cursor);
                buffer.set_cursor(pos);
                self.count = None;
                self.count_str.clear();
            }
            KeyPress::Char('{') => {
                let pos = buffer.move_paragraph_back(buffer.cursor);
                buffer.set_cursor(pos);
                self.count = None;
                self.count_str.clear();
            }
            KeyPress::Char('}') => {
                let pos = buffer.move_paragraph_forward(buffer.cursor);
                buffer.set_cursor(pos);
                self.count = None;
                self.count_str.clear();
            }
            KeyPress::Char('g') => {
                self.pending = Pending::AwaitingDouble('g');
            }
            KeyPress::Char('G') => {
                let line = self.count.unwrap_or(buffer.line_count());
                self.move_to_line(line, buffer);
                self.count = None;
                self.count_str.clear();
            }
            KeyPress::Char('f') => {
                self.pending = Pending::AwaitingFindChar;
            }
            KeyPress::Char('F') => {
                self.pending = Pending::AwaitingFindBackChar;
            }
            KeyPress::Char('t') => {
                self.pending = Pending::AwaitingTillChar;
            }
            KeyPress::Char('T') => {
                self.pending = Pending::AwaitingTillBackChar;
            }
            KeyPress::Char(';') => {
                if let Some(fs) = self.last_find {
                    let count = self.count.unwrap_or(1);
                    let mut pos = buffer.cursor;
                    for _ in 0..count {
                        pos = if fs.reverse {
                            if fs.till {
                                buffer.find_till_prev_char(pos, fs.ch)
                            } else {
                                buffer.find_prev_char(pos, fs.ch)
                            }
                        } else {
                            if fs.till {
                                buffer.find_till_next_char(pos, fs.ch)
                            } else {
                                buffer.find_next_char(pos, fs.ch)
                            }
                        }
                        .unwrap_or(pos);
                    }
                    buffer.set_cursor(pos);
                }
                self.count = None;
                self.count_str.clear();
            }
            KeyPress::Char(',') => {
                if let Some(fs) = self.last_find {
                    let count = self.count.unwrap_or(1);
                    let mut pos = buffer.cursor;
                    for _ in 0..count {
                        pos = if !fs.reverse {
                            if fs.till {
                                buffer.find_till_prev_char(pos, fs.ch)
                            } else {
                                buffer.find_prev_char(pos, fs.ch)
                            }
                        } else {
                            if fs.till {
                                buffer.find_till_next_char(pos, fs.ch)
                            } else {
                                buffer.find_next_char(pos, fs.ch)
                            }
                        }
                        .unwrap_or(pos);
                    }
                    buffer.set_cursor(pos);
                }
                self.count = None;
                self.count_str.clear();
            }
            KeyPress::Char('%') => {
                if let Some(pos) = buffer.matching_bracket(buffer.cursor) {
                    buffer.set_cursor(pos);
                }
                self.count = None;
                self.count_str.clear();
            }
            KeyPress::Char('"') => {
                self.pending = Pending::AwaitingRegister;
            }
            KeyPress::Char('x') => {
                self.save_undo(buffer);
                let count = self.count.unwrap_or(1);
                let mut deleted = String::new();
                let mut pos = buffer.cursor;
                for _ in 0..count {
                    let next = buffer.move_right(pos);
                    if next == pos {
                        break;
                    }
                    deleted.push_str(&buffer.text[pos..next]);
                    pos = next;
                }
                if !deleted.is_empty() {
                    buffer.delete(buffer.cursor..pos);
                    log::info!("Delete {} char(s)", count);
                    self.registers.set(self.op_register, deleted);
                }
                self.count = None;
                self.count_str.clear();
                self.op_register = None;
            }
            KeyPress::Char('d') => {
                self.operator = Some(Operator::Delete);
                self.pending = Pending::AwaitingDouble('d');
            }
            KeyPress::Char('y') => {
                self.operator = Some(Operator::Yank);
                self.pending = Pending::AwaitingDouble('y');
            }
            KeyPress::Char('c') => {
                self.operator = Some(Operator::Change);
                self.pending = Pending::AwaitingDouble('c');
            }
            KeyPress::Char('p') => {
                let text = self.registers.get(self.op_register).to_string();
                if !text.is_empty() {
                    self.save_undo(buffer);
                    log::info!("Paste after ({}-char reg)", text.len());
                    if text.ends_with('\n') {
                        let (line, _) = buffer.line_col(buffer.cursor);
                        let next_line = line + 1;
                        let pos = if next_line < buffer.line_count() {
                            buffer.line_start(next_line)
                        } else {
                            buffer.text.len()
                        };
                        buffer.insert(pos, &text);
                        buffer.set_cursor(pos + text.len());
                    } else {
                        let pos = buffer.move_right(buffer.cursor);
                        buffer.insert(pos, &text);
                        buffer.set_cursor(pos + text.len());
                    }
                }
                self.op_register = None;
                self.count = None;
                self.count_str.clear();
            }
            KeyPress::Char('P') => {
                let text = self.registers.get(self.op_register).to_string();
                if !text.is_empty() {
                    self.save_undo(buffer);
                    log::info!("Paste before ({}-char reg)", text.len());
                    if text.ends_with('\n') {
                        let (line, _) = buffer.line_col(buffer.cursor);
                        let pos = buffer.line_start(line);
                        buffer.insert(pos, &text);
                        buffer.set_cursor(pos);
                    } else {
                        buffer.insert(buffer.cursor, &text);
                        buffer.set_cursor(buffer.cursor);
                    }
                }
                self.op_register = None;
                self.count = None;
                self.count_str.clear();
            }
            KeyPress::Char('u') => {
                self.undo(buffer);
                self.count = None;
                self.count_str.clear();
            }
            KeyPress::Ctrl('r') => {
                self.redo(buffer);
                self.count = None;
                self.count_str.clear();
            }
            KeyPress::Char('J') => {
                self.save_undo(buffer);
                let start_pos = buffer.cursor;
                let count = self.count.unwrap_or(1);
                log::info!("Join {} line(s)", count);
                let mut pos = buffer.cursor;
                for _ in 0..count {
                    buffer.join_lines(pos);
                    pos = buffer.cursor;
                }
                self.last_change = Some(ChangeRecord::Join { pos: start_pos });
                self.count = None;
                self.count_str.clear();
            }
            KeyPress::Char('D') => {
                self.save_undo(buffer);
                let end = buffer.move_line_end(buffer.cursor);
                if end > buffer.cursor {
                    let deleted = buffer.delete(buffer.cursor..end);
                    log::info!("Delete to EOL ({} chars)", deleted.len());
                    self.registers.set(self.op_register, deleted);
                }
                self.op_register = None;
                self.count = None;
                self.count_str.clear();
            }
            KeyPress::Char('C') => {
                self.save_undo(buffer);
                let end = buffer.move_line_end(buffer.cursor);
                if end > buffer.cursor {
                    let deleted = buffer.delete(buffer.cursor..end);
                    log::info!("Change to EOL ({} chars)", deleted.len());
                    self.registers.set(self.op_register, deleted);
                }
                self.op_register = None;
                self.enter_insert_mode(buffer);
            }
            KeyPress::Char('Y') => {
                self.yank_line(buffer);
                self.count = None;
                self.count_str.clear();
            }
            KeyPress::Char('~') => {
                self.save_undo(buffer);
                let count = self.count.unwrap_or(1);
                log::info!("Toggle case {} char(s)", count);
                let mut pos = buffer.cursor;
                for _ in 0..count {
                    buffer.toggle_case_at(pos);
                    pos = buffer.move_right(pos);
                }
                buffer.set_cursor(pos);
                self.last_change = Some(ChangeRecord::ToggleCase { pos: buffer.cursor });
                self.count = None;
                self.count_str.clear();
            }
            KeyPress::Char('r') => {
                self.pending = Pending::AwaitingReplaceChar;
            }
            KeyPress::Char('.') => {
                if let Some(ref change) = self.last_change.clone() {
                    log::info!("Repeat last change");
                    self.replay_change(change, buffer);
                }
                self.count = None;
                self.count_str.clear();
            }
            KeyPress::Char('>') => {
                self.operator = Some(Operator::Indent);
                self.pending = Pending::AwaitingDouble('>');
            }
            KeyPress::Char('<') => {
                self.operator = Some(Operator::Outdent);
                self.pending = Pending::AwaitingDouble('<');
            }
            KeyPress::Ctrl('f') => {
                let (cur_line, _) = buffer.line_col(buffer.cursor);
                let target = (cur_line + SCROLL_LINES).min(buffer.line_count().saturating_sub(1));
                buffer.set_cursor(buffer.line_start(target));
                self.count = None;
                self.count_str.clear();
            }
            KeyPress::Ctrl('b') => {
                let (cur_line, _) = buffer.line_col(buffer.cursor);
                let target = cur_line.saturating_sub(SCROLL_LINES);
                buffer.set_cursor(buffer.line_start(target));
                self.count = None;
                self.count_str.clear();
            }
            KeyPress::Enter => {
                let pos = buffer.move_line_start(buffer.cursor);
                let next = buffer.move_down(pos);
                buffer.set_cursor(next);
                self.count = None;
                self.count_str.clear();
            }
            KeyPress::Backspace => {
                let pos = buffer.move_up(buffer.cursor);
                let eol = buffer.move_line_end(pos);
                buffer.set_cursor(eol);
                self.count = None;
                self.count_str.clear();
            }
            _ => {
                self.count = None;
                self.count_str.clear();
            }
        }
    }

    // ── Insert mode ──

    fn handle_insert(&mut self, key: KeyPress, buffer: &mut Buffer) {
        match key {
            KeyPress::Escape | KeyPress::Ctrl('c') | KeyPress::Ctrl('[') => {
                if !self.last_insert_text.is_empty() {
                    let change = InsertChange {
                        text: self.last_insert_text.clone(),
                        cursor_before: self.insert_cursor_start,
                    };
                    self.last_change = Some(ChangeRecord::Insert(change));
                }
                self.last_insert_text.clear();
                self.mode = Mode::Normal;
                log::info!("Mode → NORMAL");
                let pos = buffer.move_left(buffer.cursor);
                buffer.set_cursor(pos);
            }
            KeyPress::Enter => {
                // Capture indent level from the CURRENT line before inserting '\n'.
                let indent = {
                    let line = buffer.current_line();
                    line.len() - line.trim_start().len()
                };
                buffer.insert(buffer.cursor, "\n");
                for _ in 0..indent {
                    buffer.insert(buffer.cursor, " ");
                }
                self.last_insert_len += 1 + indent;
            }
            KeyPress::Backspace => {
                if buffer.cursor > 0 {
                    let prev = buffer.byte_before(buffer.cursor);
                    buffer.delete(prev..buffer.cursor);
                    if !self.last_insert_text.is_empty() {
                        self.last_insert_text.pop();
                    }
                }
            }
            KeyPress::Delete => {
                if buffer.cursor < buffer.text.len() {
                    let next = buffer.move_right(buffer.cursor);
                    buffer.delete(buffer.cursor..next);
                }
            }
            KeyPress::Tab => {
                buffer.insert(buffer.cursor, "    ");
                self.last_insert_text.push_str("    ");
                self.last_insert_len += 4;
            }
            KeyPress::ArrowLeft => {
                let pos = buffer.move_left(buffer.cursor);
                buffer.set_cursor(pos);
            }
            KeyPress::ArrowRight => {
                let pos = buffer.move_right(buffer.cursor);
                buffer.set_cursor(pos);
            }
            KeyPress::ArrowUp => {
                let pos = buffer.move_up(buffer.cursor);
                buffer.set_cursor(pos);
            }
            KeyPress::ArrowDown => {
                let pos = buffer.move_down(buffer.cursor);
                buffer.set_cursor(pos);
            }
            KeyPress::Home => {
                let pos = buffer.move_line_start(buffer.cursor);
                buffer.set_cursor(pos);
            }
            KeyPress::End => {
                let pos = buffer.move_line_end(buffer.cursor);
                buffer.set_cursor(pos);
            }
            KeyPress::Char(c) => {
                buffer.insert(buffer.cursor, &c.to_string());
                self.last_insert_text.push(c);
                self.last_insert_len += 1;
            }
            KeyPress::Ctrl('w') => {
                let pos = buffer.move_word_back(buffer.cursor);
                if pos < buffer.cursor {
                    buffer.delete(pos..buffer.cursor);
                }
            }
            KeyPress::Ctrl('u') => {
                let (line, _) = buffer.line_col(buffer.cursor);
                let start = buffer.line_start(line);
                if start < buffer.cursor {
                    buffer.delete(start..buffer.cursor);
                }
            }
            _ => {}
        }
    }

    // ── Visual mode ──

    fn handle_visual(&mut self, key: KeyPress, buffer: &mut Buffer) {
        // Parse accumulated count before any dispatch
        if !self.count_str.is_empty() && self.count.is_none() {
            self.count = self.count_str.parse::<usize>().ok();
        }

        // Handle pending states
        match self.pending {
            Pending::AwaitingDouble(c) => {
                self.pending = Pending::None;
                if c == 'g' && key.is_char('g') {
                    let pos = buffer.byte_from_line_col(0, 0);
                    buffer.set_cursor(pos);
                }
                self.count = None;
                self.count_str.clear();
                return;
            }
            Pending::AwaitingFindChar => {
                self.pending = Pending::None;
                if let KeyPress::Char(c) = key {
                    self.last_find = Some(FindState {
                        ch: c,
                        reverse: false,
                        till: false,
                    });
                    let count = self.count.unwrap_or(1);
                    let mut pos = buffer.cursor;
                    for _ in 0..count {
                        match buffer.find_next_char(pos, c) {
                            Some(p) => pos = p,
                            None => break,
                        }
                    }
                    buffer.set_cursor(pos);
                }
                self.count = None;
                self.count_str.clear();
                return;
            }
            Pending::AwaitingTillChar => {
                self.pending = Pending::None;
                if let KeyPress::Char(c) = key {
                    self.last_find = Some(FindState {
                        ch: c,
                        reverse: false,
                        till: true,
                    });
                    let count = self.count.unwrap_or(1);
                    let mut pos = buffer.cursor;
                    for _ in 0..count {
                        match buffer.find_till_next_char(pos, c) {
                            Some(p) => pos = p,
                            None => break,
                        }
                    }
                    buffer.set_cursor(pos);
                }
                self.count = None;
                self.count_str.clear();
                return;
            }
            Pending::AwaitingFindBackChar => {
                self.pending = Pending::None;
                if let KeyPress::Char(c) = key {
                    self.last_find = Some(FindState {
                        ch: c,
                        reverse: true,
                        till: false,
                    });
                    let count = self.count.unwrap_or(1);
                    let mut pos = buffer.cursor;
                    for _ in 0..count {
                        match buffer.find_prev_char(pos, c) {
                            Some(p) => pos = p,
                            None => break,
                        }
                    }
                    buffer.set_cursor(pos);
                }
                self.count = None;
                self.count_str.clear();
                return;
            }
            Pending::AwaitingTillBackChar => {
                self.pending = Pending::None;
                if let KeyPress::Char(c) = key {
                    self.last_find = Some(FindState {
                        ch: c,
                        reverse: true,
                        till: true,
                    });
                    let count = self.count.unwrap_or(1);
                    let mut pos = buffer.cursor;
                    for _ in 0..count {
                        match buffer.find_till_prev_char(pos, c) {
                            Some(p) => pos = p,
                            None => break,
                        }
                    }
                    buffer.set_cursor(pos);
                }
                self.count = None;
                self.count_str.clear();
                return;
            }
            _ => {
                self.pending = Pending::None;
                self.count = None;
                self.count_str.clear();
            }
        }

        match key {
            KeyPress::Escape | KeyPress::Char('v') => {
                buffer.clear_selection();
                self.mode = Mode::Normal;
                log::info!("Mode → NORMAL");
                self.count = None;
                self.count_str.clear();
                return;
            }
            KeyPress::Char('V') => {
                if let Some(ref mut sel) = buffer.selection {
                    sel.mode = if sel.mode == SelectionMode::Linewise {
                        SelectionMode::Charwise
                    } else {
                        SelectionMode::Linewise
                    };
                }
                return;
            }
            _ => {}
        }

        // In linewise visual, extend selection by line
        let is_linewise = buffer
            .selection
            .as_ref()
            .map(|s| s.mode == SelectionMode::Linewise)
            .unwrap_or(false);

        // Resolve motion
        let new_cursor = match key {
            KeyPress::Char('h') | KeyPress::ArrowLeft => {
                let count = self.count.unwrap_or(1);
                let mut pos = buffer.cursor;
                for _ in 0..count {
                    pos = buffer.move_left(pos);
                }
                pos
            }
            KeyPress::Char('j') | KeyPress::ArrowDown => {
                let count = self.count.unwrap_or(1);
                let mut pos = buffer.cursor;
                for _ in 0..count {
                    pos = buffer.move_down(pos);
                }
                pos
            }
            KeyPress::Char('k') | KeyPress::ArrowUp => {
                let count = self.count.unwrap_or(1);
                let mut pos = buffer.cursor;
                for _ in 0..count {
                    pos = buffer.move_up(pos);
                }
                pos
            }
            KeyPress::Char('l') | KeyPress::ArrowRight => {
                let count = self.count.unwrap_or(1);
                let mut pos = buffer.cursor;
                for _ in 0..count {
                    pos = buffer.move_right(pos);
                }
                pos
            }
            KeyPress::Char('w') => {
                let count = self.count.unwrap_or(1);
                let mut pos = buffer.cursor;
                for _ in 0..count {
                    pos = buffer.move_word_forward(pos);
                }
                pos
            }
            KeyPress::Char('b') => {
                let count = self.count.unwrap_or(1);
                let mut pos = buffer.cursor;
                for _ in 0..count {
                    pos = buffer.move_word_back(pos);
                }
                pos
            }
            KeyPress::Char('e') => {
                let count = self.count.unwrap_or(1);
                let mut pos = buffer.cursor;
                for _ in 0..count {
                    pos = buffer.move_word_end(pos);
                }
                pos
            }
            KeyPress::Char('0') | KeyPress::Home => buffer.move_line_start(buffer.cursor),
            KeyPress::Char('$') | KeyPress::End => buffer.move_line_end(buffer.cursor),
            KeyPress::Char('^') => buffer.move_first_nonblank(buffer.cursor),
            KeyPress::Char('G') => {
                let line = self.count.unwrap_or(buffer.line_count());
                let line = line.max(1).min(buffer.line_count());
                buffer.byte_from_line_col(line - 1, 0)
            }
            KeyPress::Char('g') => {
                self.pending = Pending::AwaitingDouble('g');
                return;
            }
            KeyPress::Char('{') => buffer.move_paragraph_back(buffer.cursor),
            KeyPress::Char('}') => buffer.move_paragraph_forward(buffer.cursor),
            KeyPress::Char('n') => {
                let mut pos = buffer.cursor;
                if !self.search_pattern.is_empty() {
                    self.last_search_forward = true;
                    let start = buffer.cursor + 1;
                    if start < buffer.text.len() {
                        let search_area = &buffer.text[start..];
                        if let Some(found) = search_area.find(&self.search_pattern) {
                            pos = start + found;
                        }
                    }
                }
                pos
            }
            KeyPress::Char('N') => {
                let mut pos = buffer.cursor;
                if !self.search_pattern.is_empty() && buffer.cursor > 0 {
                    self.last_search_forward = false;
                    let before = &buffer.text[..buffer.cursor];
                    if let Some(found) = before.rfind(&self.search_pattern) {
                        pos = found;
                    }
                }
                pos
            }
            KeyPress::Char('f') => {
                self.pending = Pending::AwaitingFindChar;
                return;
            }
            KeyPress::Char('F') => {
                self.pending = Pending::AwaitingFindBackChar;
                return;
            }
            KeyPress::Char('t') => {
                self.pending = Pending::AwaitingTillChar;
                return;
            }
            KeyPress::Char('T') => {
                self.pending = Pending::AwaitingTillBackChar;
                return;
            }
            KeyPress::Char(';') | KeyPress::Char(',') => {
                // Re-use last find
                if let Some(fs) = self.last_find {
                    let rev = key == KeyPress::Char(',');
                    let mut pos = buffer.cursor;
                    for _ in 0..self.count.unwrap_or(1) {
                        let r = if rev ^ fs.reverse {
                            if fs.till {
                                buffer.find_till_prev_char(pos, fs.ch)
                            } else {
                                buffer.find_prev_char(pos, fs.ch)
                            }
                        } else {
                            if fs.till {
                                buffer.find_till_next_char(pos, fs.ch)
                            } else {
                                buffer.find_next_char(pos, fs.ch)
                            }
                        };
                        pos = r.unwrap_or(pos);
                    }
                    pos
                } else {
                    buffer.cursor
                }
            }
            KeyPress::Char('%') => buffer
                .matching_bracket(buffer.cursor)
                .unwrap_or(buffer.cursor),
            _ => buffer.cursor,
        };

        self.count = None;
        self.count_str.clear();

        if is_linewise {
            let (line, _) = buffer.line_col(new_cursor);
            let start = buffer.line_start(line);
            let end = buffer.line_end(line);
            if let Some(ref mut sel) = buffer.selection {
                sel.active = end;
                buffer.cursor = start;
            }
        } else {
            if let Some(ref mut sel) = buffer.selection {
                sel.active = new_cursor;
            }
            buffer.cursor = new_cursor;
        }

        // Handle operator keys
        match key {
            KeyPress::Char('d') | KeyPress::Char('x') => {
                if let Some(range) = buffer.selection_range() {
                    self.save_undo(buffer);
                    let deleted = buffer.delete(range);
                    log::info!("Visual delete {} chars", deleted.len());
                    self.registers.set(self.op_register, deleted);
                }
                buffer.clear_selection();
                self.mode = Mode::Normal;
                log::info!("Mode → NORMAL");
                self.op_register = None;
            }
            KeyPress::Char('y') => {
                if let Some(range) = buffer.selection_range() {
                    let text = buffer.text[range].to_string();
                    log::info!("Visual yank {} chars", text.len());
                    self.registers.set(self.op_register, text);
                }
                buffer.clear_selection();
                self.mode = Mode::Normal;
                log::info!("Mode → NORMAL");
                self.op_register = None;
            }
            KeyPress::Char('c') => {
                if let Some(range) = buffer.selection_range() {
                    self.save_undo(buffer);
                    let deleted = buffer.delete(range);
                    log::info!("Visual change {} chars", deleted.len());
                    self.registers.set(self.op_register, deleted);
                    self.enter_insert_mode(buffer);
                }
                buffer.clear_selection();
                self.op_register = None;
            }
            KeyPress::Char('~') => {
                if let Some(range) = buffer.selection_range() {
                    self.save_undo(buffer);
                    let mut pos = range.start;
                    while pos < range.end && pos < buffer.text.len() {
                        if buffer.text.is_char_boundary(pos) {
                            if let Some(c) = buffer.text[pos..].chars().next() {
                                buffer.toggle_case_at(pos);
                                pos += c.len_utf8();
                                continue;
                            }
                        }
                        pos += 1;
                    }
                    self.last_change = Some(ChangeRecord::ToggleCaseRange(range));
                    log::info!("Visual toggle case");
                }
                buffer.clear_selection();
                self.mode = Mode::Normal;
                log::info!("Mode → NORMAL");
            }
            KeyPress::Char('>') => {
                if let Some(range) = buffer.selection_range() {
                    self.save_undo(buffer);
                    let start_line = buffer.line_col(range.start).0;
                    let end_line = buffer.line_col(range.end.saturating_sub(1)).0;
                    for l in start_line..=end_line {
                        let ls = buffer.line_start(l);
                        buffer.insert(ls, "    ");
                    }
                    log::info!("Visual indent lines {}-{}", start_line + 1, end_line + 1);
                    self.last_change = Some(ChangeRecord::Indent {
                        line: start_line,
                        count: end_line - start_line + 1,
                    });
                }
                buffer.clear_selection();
                self.mode = Mode::Normal;
                log::info!("Mode → NORMAL");
            }
            KeyPress::Char('<') => {
                if let Some(range) = buffer.selection_range() {
                    self.save_undo(buffer);
                    let start_line = buffer.line_col(range.start).0;
                    let end_line = buffer.line_col(range.end.saturating_sub(1)).0;
                    for l in start_line..=end_line {
                        let ls = buffer.line_start(l);
                        if buffer.text[ls..].starts_with("    ") {
                            buffer.delete(ls..ls + 4);
                        } else if buffer.text[ls..].starts_with('\t') {
                            buffer.delete(ls..ls + 1);
                        }
                    }
                    log::info!("Visual outdent lines {}-{}", start_line + 1, end_line + 1);
                    self.last_change = Some(ChangeRecord::Outdent {
                        line: start_line,
                        count: end_line - start_line + 1,
                    });
                }
                buffer.clear_selection();
                self.mode = Mode::Normal;
                log::info!("Mode → NORMAL");
            }
            _ => {}
        }
    }

    // ── Command line mode ──

    fn handle_command_line(&mut self, key: KeyPress, buffer: &mut Buffer) {
        match key {
            KeyPress::Escape | KeyPress::Ctrl('c') => {
                self.mode = Mode::Normal;
                log::info!("Mode → NORMAL");
                self.command_line.clear();
            }
            KeyPress::Enter => {
                let cmd = self.command_line.clone();
                self.mode = Mode::Normal;
                log::info!("Mode → NORMAL");
                self.execute_command(&cmd, buffer);
                self.command_line.clear();
            }
            KeyPress::Backspace => {
                self.command_line.pop();
            }
            KeyPress::Delete => {
                // no-op in command line
            }
            KeyPress::Ctrl('u') => {
                let prefix = self.command_line.chars().next().unwrap_or(':');
                self.command_line.clear();
                self.command_line.push(prefix);
            }
            KeyPress::Ctrl('w') => {
                let trimmed = self.command_line.trim_end();
                let pos = trimmed.rfind([' ', '/']).unwrap_or(1);
                self.command_line.truncate(pos);
            }
            KeyPress::Char(c) => {
                self.command_line.push(c);
            }
            _ => {}
        }
    }

    // ── Command execution ──

    fn execute_command(&mut self, cmd: &str, buffer: &mut Buffer) {
        let trimmed = cmd.trim();
        if trimmed.is_empty() {
            return;
        }
        log::info!("Cmd: {}", trimmed);

        let first = trimmed.chars().next().unwrap_or(':');
        match first {
            ':' => {
                let rest = trimmed[1..].trim().to_lowercase();
                if rest == "w" || rest == "write" {
                    self.command_action = CommandAction::Save;
                } else if rest == "q" || rest == "quit" {
                    self.command_action = CommandAction::Quit;
                } else if rest == "wq" || rest == "x" {
                    self.command_action = CommandAction::SaveQuit;
                } else if rest == "q!" || rest == "quit!" {
                    self.command_action = CommandAction::ForceQuit;
                } else if rest == "wq!" {
                    self.command_action = CommandAction::SaveForceQuit;
                } else if rest.starts_with("e ") || rest.starts_with("edit ") {
                    let name = if rest.starts_with("edit ") {
                        rest[5..].trim()
                    } else {
                        rest[2..].trim()
                    };
                    if !name.is_empty() {
                        self.command_action = CommandAction::Open(name.to_string());
                    }
                } else if rest == "help" {
                    let text = concat!(
                        "# Vim Help\n\n",
                        ":w    - save\n",
                        ":q    - quit\n",
                        ":wq   - save and quit\n",
                        ":e n  - open note\n",
                        "/pat  - search forward\n",
                        "?pat  - search backward\n",
                        "u     - undo\n",
                        "Ctrl+r - redo\n",
                        ".:     - repeat last change\n"
                    )
                    .to_string();
                    self.save_undo(buffer);
                    buffer.set_text(text);
                } else if rest == "noh" || rest == "nohlsearch" {
                    // Clear search highlights
                }
            }
            '/' | '?' => {
                let pattern = trimmed[1..].to_string();
                self.search_pattern = pattern.clone();
                if !pattern.is_empty() {
                    self.search_in_buffer(&pattern, first == '/', buffer);
                }
            }
            _ => {}
        }
    }

    fn search_in_buffer(&mut self, pattern: &str, forward: bool, buffer: &mut Buffer) {
        if pattern.is_empty() {
            return;
        }
        self.last_search_forward = forward;
        log::info!("Search: '{}' forward={}", pattern, forward);
        if forward {
            let start = buffer.cursor + 1;
            if start >= buffer.text.len() {
                return;
            }
            let search_area = &buffer.text[start..];
            if let Some(pos) = search_area.find(pattern) {
                buffer.set_cursor(start + pos);
            }
        } else {
            if buffer.cursor == 0 {
                return;
            }
            let before = &buffer.text[..buffer.cursor];
            if let Some(pos) = before.rfind(pattern) {
                buffer.set_cursor(pos);
            }
        }
    }

    // ── Internal helpers ──

    fn enter_insert_mode(&mut self, buffer: &mut Buffer) {
        self.mode = Mode::Insert;
        log::info!("Mode → INSERT");
        self.last_insert_text.clear();
        self.last_insert_len = 0;
        self.insert_cursor_start = buffer.cursor;
        self.count = None;
        self.count_str.clear();
        self.operator = None;
        self.pending = Pending::None;
    }

    fn delete_line(&mut self, buffer: &mut Buffer) {
        self.save_undo(buffer);
        let count = self.count.unwrap_or(1);
        let (line, _) = buffer.line_col(buffer.cursor);
        let end_line = (line + count - 1).min(buffer.line_count() - 1);
        let start = buffer.line_start(line);
        let end = if end_line + 1 < buffer.line_count() {
            buffer.line_start(end_line + 1)
        } else {
            buffer.line_end(end_line)
        };
        let deleted = buffer.delete(start..end);
        log::info!("Delete lines {}-{}", line + 1, end_line + 1);
        self.registers.set(self.op_register, deleted.clone());
        self.op_register = None;
        self.last_change = Some(ChangeRecord::Delete { text: deleted });
        self.count = None;
        self.count_str.clear();
    }

    fn yank_line(&mut self, buffer: &mut Buffer) {
        let count = self.count.unwrap_or(1);
        let (line, _) = buffer.line_col(buffer.cursor);
        let end_line = (line + count - 1).min(buffer.line_count() - 1);
        let start = buffer.line_start(line);
        let end = if end_line + 1 < buffer.line_count() {
            buffer.line_start(end_line + 1)
        } else {
            buffer.text.len()
        };
        let text = buffer.text[start..end].to_string();
        let register_text = if text.ends_with('\n') {
            text[..text.len() - 1].to_string()
        } else {
            text.clone()
        };
        log::info!("Yank lines {}-{}", line + 1, end_line + 1);
        self.registers.set(self.op_register, register_text);
        self.op_register = None;
        let mc = ChangeRecord::Yank;
        self.last_change = Some(mc);
        self.count = None;
        self.count_str.clear();
    }

    fn indent_line(&mut self, buffer: &mut Buffer) {
        self.save_undo(buffer);
        let count = self.count.unwrap_or(1);
        let (line, _) = buffer.line_col(buffer.cursor);
        log::info!("Indent {} lines starting at line {}", count, line + 1);
        for l in line..line + count {
            if l >= buffer.line_count() {
                break;
            }
            let ls = buffer.line_start(l);
            buffer.insert(ls, "    ");
        }
        self.last_change = Some(ChangeRecord::Indent { line, count });
        self.count = None;
        self.count_str.clear();
    }

    fn outdent_line(&mut self, buffer: &mut Buffer) {
        self.save_undo(buffer);
        let count = self.count.unwrap_or(1);
        let (line, _) = buffer.line_col(buffer.cursor);
        log::info!("Outdent {} lines starting at line {}", count, line + 1);
        for l in line..line + count {
            if l >= buffer.line_count() {
                break;
            }
            let ls = buffer.line_start(l);
            if buffer.text[ls..].starts_with("    ") {
                buffer.delete(ls..ls + 4);
            } else if buffer.text[ls..].starts_with('\t') {
                buffer.delete(ls..ls + 1);
            }
        }
        self.last_change = Some(ChangeRecord::Outdent { line, count });
        self.count = None;
        self.count_str.clear();
    }

    fn move_to_line(&mut self, line: usize, buffer: &mut Buffer) {
        let line = line.max(1).min(buffer.line_count());
        buffer.set_cursor(buffer.byte_from_line_col(line - 1, 0));
    }

    fn resolve_motion(&self, key: KeyPress, buffer: &Buffer) -> Option<std::ops::Range<usize>> {
        let count = self.count.unwrap_or(1);
        let mut end = buffer.cursor;
        for _ in 0..count {
            end = match key {
                KeyPress::Char('h') | KeyPress::ArrowLeft => buffer.move_left(end),
                KeyPress::Char('j') | KeyPress::ArrowDown => buffer.move_down(end),
                KeyPress::Char('k') | KeyPress::ArrowUp => buffer.move_up(end),
                KeyPress::Char('l') | KeyPress::ArrowRight => buffer.move_right(end),
                KeyPress::Char('w') => buffer.move_word_forward(end),
                KeyPress::Char('W') => buffer.move_big_word_forward(end),
                KeyPress::Char('b') => buffer.move_word_back(end),
                KeyPress::Char('B') => buffer.move_big_word_back(end),
                KeyPress::Char('e') => buffer.move_word_end(end),
                KeyPress::Char('E') => buffer.move_big_word_end(end),
                KeyPress::Char('0') => buffer.move_line_start(end),
                KeyPress::Char('$') | KeyPress::End => buffer.move_line_end(end),
                KeyPress::Char('^') | KeyPress::Home => buffer.move_first_nonblank(end),
                KeyPress::Char('{') => buffer.move_paragraph_back(end),
                KeyPress::Char('}') => buffer.move_paragraph_forward(end),
                KeyPress::Char('G') => {
                    let line = self.count.unwrap_or(buffer.line_count());
                    return Some(
                        buffer.cursor
                            ..buffer
                                .byte_from_line_col(line.max(1).min(buffer.line_count()) - 1, 0),
                    );
                }
                KeyPress::Char('%') => buffer.matching_bracket(end).unwrap_or(end),
                KeyPress::Enter => buffer.move_down(buffer.line_end(end)),
                KeyPress::Backspace => buffer.move_up(buffer.line_start(end)),
                _ => {
                    return None;
                }
            };
        }
        if end == buffer.cursor {
            return None;
        }
        Some(buffer.cursor.min(end)..buffer.cursor.max(end))
    }

    fn resolve_text_object(
        &self,
        key: KeyPress,
        buffer: &Buffer,
        is_inner: bool,
    ) -> Option<Range<usize>> {
        match key {
            KeyPress::Char('w') => {
                if is_inner {
                    Some(buffer.inner_word(buffer.cursor))
                } else {
                    Some(buffer.a_word(buffer.cursor))
                }
            }
            KeyPress::Char('W') => {
                if is_inner {
                    Some(buffer.inner_big_word(buffer.cursor))
                } else {
                    Some(buffer.a_big_word(buffer.cursor))
                }
            }
            KeyPress::Char('p') => {
                if is_inner {
                    Some(buffer.inner_paragraph(buffer.cursor))
                } else {
                    Some(buffer.a_paragraph(buffer.cursor))
                }
            }
            KeyPress::Char('(') | KeyPress::Char(')') => {
                if is_inner {
                    buffer.inner_pair(buffer.cursor, '(', ')')
                } else {
                    buffer.a_pair(buffer.cursor, '(', ')')
                }
            }
            KeyPress::Char('[') | KeyPress::Char(']') => {
                if is_inner {
                    buffer.inner_pair(buffer.cursor, '[', ']')
                } else {
                    buffer.a_pair(buffer.cursor, '[', ']')
                }
            }
            KeyPress::Char('{') | KeyPress::Char('}') => {
                if is_inner {
                    buffer.inner_pair(buffer.cursor, '{', '}')
                } else {
                    buffer.a_pair(buffer.cursor, '{', '}')
                }
            }
            KeyPress::Char('\'') => {
                if is_inner {
                    buffer.inner_quote(buffer.cursor, '\'')
                } else {
                    buffer.a_quote(buffer.cursor, '\'')
                }
            }
            KeyPress::Char('"') => {
                if is_inner {
                    buffer.inner_quote(buffer.cursor, '"')
                } else {
                    buffer.a_quote(buffer.cursor, '"')
                }
            }
            KeyPress::Char('`') => {
                if is_inner {
                    buffer.inner_quote(buffer.cursor, '`')
                } else {
                    buffer.a_quote(buffer.cursor, '`')
                }
            }
            _ => None,
        }
    }

    fn execute_operator(
        &mut self,
        op: Operator,
        range: std::ops::Range<usize>,
        buffer: &mut Buffer,
    ) {
        self.execute_operator_on_range(op, range, buffer);
    }

    fn execute_operator_on_range(
        &mut self,
        op: Operator,
        range: std::ops::Range<usize>,
        buffer: &mut Buffer,
    ) {
        if range.start >= range.end {
            return;
        }
        self.save_undo(buffer);
        let op_name = match op {
            Operator::Delete => "Delete",
            Operator::Yank => "Yank",
            Operator::Change => "Change",
            Operator::ToggleCase => "ToggleCase",
            Operator::Lowercase => "Lowercase",
            Operator::Uppercase => "Uppercase",
            Operator::Indent => "Indent",
            Operator::Outdent => "Outdent",
        };
        log::info!("{} range {}-{}", op_name, range.start, range.end);
        match op {
            Operator::Delete => {
                let deleted = buffer.delete(range.clone());
                self.registers.set(self.op_register, deleted.clone());
                self.last_change = Some(ChangeRecord::Delete { text: deleted });
            }
            Operator::Yank => {
                let text = buffer.text[range].to_string();
                self.registers.set(self.op_register, text);
            }
            Operator::Change => {
                let deleted = buffer.delete(range);
                self.registers.set(self.op_register, deleted);
                self.enter_insert_mode(buffer);
            }
            Operator::ToggleCase => {
                let mut pos = range.start;
                while pos < range.end && pos < buffer.text.len() {
                    if buffer.text.is_char_boundary(pos) {
                        if let Some(c) = buffer.text[pos..].chars().next() {
                            buffer.toggle_case_at(pos);
                            pos += c.len_utf8();
                            continue;
                        }
                    }
                    pos += 1;
                }
                self.last_change = Some(ChangeRecord::ToggleCaseRange(range));
            }
            Operator::Lowercase => {
                let safe_start = (range.start..buffer.text.len())
                    .find(|&i| buffer.text.is_char_boundary(i))
                    .unwrap_or(range.start);
                let safe_end = (range.end..=buffer.text.len())
                    .find(|&i| i == buffer.text.len() || buffer.text.is_char_boundary(i))
                    .unwrap_or(range.end);
                if safe_start < safe_end {
                    let s: String = buffer.text[safe_start..safe_end]
                        .chars()
                        .flat_map(|c| c.to_lowercase())
                        .collect();
                    buffer.text.drain(safe_start..safe_end);
                    buffer.text.insert_str(safe_start, &s);
                    buffer.cursor = safe_start + s.len();
                    buffer.rebuild_line_starts();
                }
                self.last_change = Some(ChangeRecord::LowercaseRange(range));
            }
            Operator::Uppercase => {
                let safe_start = (range.start..buffer.text.len())
                    .find(|&i| buffer.text.is_char_boundary(i))
                    .unwrap_or(range.start);
                let safe_end = (range.end..=buffer.text.len())
                    .find(|&i| i == buffer.text.len() || buffer.text.is_char_boundary(i))
                    .unwrap_or(range.end);
                if safe_start < safe_end {
                    let s: String = buffer.text[safe_start..safe_end]
                        .chars()
                        .flat_map(|c| c.to_uppercase())
                        .collect();
                    buffer.text.drain(safe_start..safe_end);
                    buffer.text.insert_str(safe_start, &s);
                    buffer.cursor = safe_start + s.len();
                    buffer.rebuild_line_starts();
                }
                self.last_change = Some(ChangeRecord::UppercaseRange(range));
            }
            Operator::Indent => {
                let start_line = buffer.line_col(range.start).0;
                let end_line = buffer.line_col(range.end.saturating_sub(1)).0;
                for l in start_line..=end_line {
                    let ls = buffer.line_start(l);
                    buffer.insert(ls, "    ");
                }
                self.last_change = Some(ChangeRecord::Indent {
                    line: start_line,
                    count: end_line - start_line + 1,
                });
            }
            Operator::Outdent => {
                let start_line = buffer.line_col(range.start).0;
                let end_line = buffer.line_col(range.end.saturating_sub(1)).0;
                for l in start_line..=end_line {
                    let ls = buffer.line_start(l);
                    if buffer.text[ls..].starts_with("    ") {
                        buffer.delete(ls..ls + 4);
                    } else if buffer.text[ls..].starts_with('\t') {
                        buffer.delete(ls..ls + 1);
                    }
                }
                self.last_change = Some(ChangeRecord::Outdent {
                    line: start_line,
                    count: end_line - start_line + 1,
                });
            }
        }
        self.op_register = None;
    }

    fn save_undo(&mut self, buffer: &Buffer) {
        if self.undo_stack.len() >= MAX_UNDO {
            self.undo_stack.pop_front(); // O(1) with VecDeque
        }
        self.undo_stack.push_back(buffer.undo_snapshot());
        self.redo_stack.clear();
        log::info!("Saved undo snapshot (depth {})", self.undo_stack.len());
    }

    fn undo(&mut self, buffer: &mut Buffer) {
        if let Some(snap) = self.undo_stack.pop_back() {
            self.redo_stack.push_back(buffer.undo_snapshot());
            buffer.restore_snapshot(&snap);
            log::info!("Undo (depth {})", self.undo_stack.len());
        }
    }

    fn redo(&mut self, buffer: &mut Buffer) {
        if let Some(snap) = self.redo_stack.pop_back() {
            self.undo_stack.push_back(buffer.undo_snapshot());
            buffer.restore_snapshot(&snap);
            log::info!("Redo (depth {})", self.redo_stack.len());
        }
    }

    fn replay_change(&mut self, change: &ChangeRecord, buffer: &mut Buffer) {
        log::info!("Repeat: {:?}", change);
        self.save_undo(buffer);
        match change {
            ChangeRecord::Insert(ic) => {
                buffer.insert(ic.cursor_before, &ic.text);
                buffer.set_cursor(ic.cursor_before + ic.text.len());
            }
            ChangeRecord::Delete { text, .. } => {
                // Repeat a delete by removing the same byte-length of text
                // forward from the current cursor position.
                let end = (buffer.cursor + text.len()).min(buffer.text.len());
                // Advance end to the next char boundary if needed.
                let end = (end..=buffer.text.len())
                    .find(|&i| buffer.text.is_char_boundary(i))
                    .unwrap_or(buffer.text.len());
                if end > buffer.cursor {
                    buffer.delete(buffer.cursor..end);
                }
            }
            ChangeRecord::Replace { pos, new } => {
                buffer.replace_char(*pos, *new);
            }
            ChangeRecord::Join { pos, .. } => {
                buffer.join_lines(*pos);
            }
            ChangeRecord::ToggleCase { pos } => {
                buffer.toggle_case_at(*pos);
            }
            ChangeRecord::Indent { line, count } => {
                for _ in 0..*count {
                    let ls = buffer.line_start(*line);
                    buffer.insert(ls, "    ");
                }
            }
            ChangeRecord::Outdent { line, count } => {
                for _ in 0..*count {
                    let ls = buffer.line_start(*line);
                    if buffer.text[ls..].starts_with("    ") {
                        buffer.delete(ls..ls + 4);
                    } else if buffer.text[ls..].starts_with('\t') {
                        buffer.delete(ls..ls + 1);
                    }
                }
            }
            ChangeRecord::ToggleCaseRange(range) => {
                let mut pos = range.start;
                while pos < range.end && pos < buffer.text.len() {
                    if buffer.text.is_char_boundary(pos) {
                        if let Some(c) = buffer.text[pos..].chars().next() {
                            buffer.toggle_case_at(pos);
                            pos += c.len_utf8();
                            continue;
                        }
                    }
                    pos += 1;
                }
            }
            ChangeRecord::LowercaseRange(range) => {
                let safe_start = (range.start..buffer.text.len())
                    .find(|&i| buffer.text.is_char_boundary(i))
                    .unwrap_or(range.start);
                let safe_end = (range.end..=buffer.text.len())
                    .find(|&i| i == buffer.text.len() || buffer.text.is_char_boundary(i))
                    .unwrap_or(range.end);
                if safe_start < safe_end {
                    let s: String = buffer.text[safe_start..safe_end]
                        .chars()
                        .flat_map(|c| c.to_lowercase())
                        .collect();
                    buffer.text.drain(safe_start..safe_end);
                    buffer.text.insert_str(safe_start, &s);
                    buffer.cursor = safe_start + s.len();
                    buffer.rebuild_line_starts();
                }
            }
            ChangeRecord::UppercaseRange(range) => {
                let safe_start = (range.start..buffer.text.len())
                    .find(|&i| buffer.text.is_char_boundary(i))
                    .unwrap_or(range.start);
                let safe_end = (range.end..=buffer.text.len())
                    .find(|&i| i == buffer.text.len() || buffer.text.is_char_boundary(i))
                    .unwrap_or(range.end);
                if safe_start < safe_end {
                    let s: String = buffer.text[safe_start..safe_end]
                        .chars()
                        .flat_map(|c| c.to_uppercase())
                        .collect();
                    buffer.text.drain(safe_start..safe_end);
                    buffer.text.insert_str(safe_start, &s);
                    buffer.cursor = safe_start + s.len();
                    buffer.rebuild_line_starts();
                }
            }
            ChangeRecord::Yank => {} // no-op on replay
        }
    }

    pub fn mark_saved(&mut self, buffer: &Buffer) {
        self.saved_snapshot = Some(buffer.undo_snapshot());
    }

    pub fn needs_save(&self, buffer: &Buffer) -> bool {
        match &self.saved_snapshot {
            Some(snap) => snap.text != buffer.text,
            None => !buffer.text.is_empty(),
        }
    }
}
