use std::fmt::{self, Display, Formatter};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum KeyPress {
    Char(char),
    Escape,
    Enter,
    Backspace,
    Delete,
    Tab,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Home,
    End,
    PageUp,
    PageDown,
    Ctrl(char),
    F(u8),
}

impl Display for KeyPress {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            KeyPress::Char(c) => write!(f, "{}", c),
            KeyPress::Escape => write!(f, "Esc"),
            KeyPress::Enter => write!(f, "Enter"),
            KeyPress::Backspace => write!(f, "Backspace"),
            KeyPress::Delete => write!(f, "Delete"),
            KeyPress::Tab => write!(f, "Tab"),
            KeyPress::ArrowLeft => write!(f, "Left"),
            KeyPress::ArrowRight => write!(f, "Right"),
            KeyPress::ArrowUp => write!(f, "Up"),
            KeyPress::ArrowDown => write!(f, "Down"),
            KeyPress::Home => write!(f, "Home"),
            KeyPress::End => write!(f, "End"),
            KeyPress::PageUp => write!(f, "PageUp"),
            KeyPress::PageDown => write!(f, "PageDown"),
            KeyPress::Ctrl(c) => write!(f, "Ctrl+{}", c),
            KeyPress::F(n) => write!(f, "F{}", n),
        }
    }
}

impl KeyPress {
    pub fn from_egui(ev: &egui::Event) -> Option<Self> {
        match ev {
            egui::Event::Text(t) => {
                let mut chars = t.chars();
                let c = chars.next()?;
                if chars.next().is_none() && !c.is_control() {
                    return Some(KeyPress::Char(c));
                }
                None
            }
            egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } => {
                if modifiers.ctrl || modifiers.mac_cmd {
                    if let Some(c) = key_char(*key) {
                        return Some(KeyPress::Ctrl(c));
                    }
                }
                match key {
                    egui::Key::Escape => Some(KeyPress::Escape),
                    egui::Key::Enter => Some(KeyPress::Enter),
                    egui::Key::Backspace => Some(KeyPress::Backspace),
                    egui::Key::Delete => Some(KeyPress::Delete),
                    egui::Key::Tab => Some(KeyPress::Tab),
                    egui::Key::ArrowLeft => Some(KeyPress::ArrowLeft),
                    egui::Key::ArrowRight => Some(KeyPress::ArrowRight),
                    egui::Key::ArrowUp => Some(KeyPress::ArrowUp),
                    egui::Key::ArrowDown => Some(KeyPress::ArrowDown),
                    egui::Key::Home => Some(KeyPress::Home),
                    egui::Key::End => Some(KeyPress::End),
                    egui::Key::PageUp => Some(KeyPress::PageUp),
                    egui::Key::PageDown => Some(KeyPress::PageDown),
                    egui::Key::F1 => Some(KeyPress::F(1)),
                    egui::Key::F2 => Some(KeyPress::F(2)),
                    egui::Key::F3 => Some(KeyPress::F(3)),
                    egui::Key::F4 => Some(KeyPress::F(4)),
                    egui::Key::F5 => Some(KeyPress::F(5)),
                    egui::Key::F6 => Some(KeyPress::F(6)),
                    egui::Key::F7 => Some(KeyPress::F(7)),
                    egui::Key::F8 => Some(KeyPress::F(8)),
                    egui::Key::F9 => Some(KeyPress::F(9)),
                    egui::Key::F10 => Some(KeyPress::F(10)),
                    egui::Key::F11 => Some(KeyPress::F(11)),
                    egui::Key::F12 => Some(KeyPress::F(12)),
                    egui::Key::F13 => Some(KeyPress::F(13)),
                    egui::Key::F14 => Some(KeyPress::F(14)),
                    egui::Key::F15 => Some(KeyPress::F(15)),
                    egui::Key::F16 => Some(KeyPress::F(16)),
                    egui::Key::F17 => Some(KeyPress::F(17)),
                    egui::Key::F18 => Some(KeyPress::F(18)),
                    egui::Key::F19 => Some(KeyPress::F(19)),
                    egui::Key::F20 => Some(KeyPress::F(20)),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    pub fn from_egui_event(ev: &egui::Event) -> Option<Self> {
        Self::from_egui(ev)
    }

    pub fn is_char(&self, c: char) -> bool {
        matches!(self, KeyPress::Char(x) if *x == c)
    }
}

fn key_char(key: egui::Key) -> Option<char> {
    match key {
        egui::Key::A => Some('a'),
        egui::Key::B => Some('b'),
        egui::Key::C => Some('c'),
        egui::Key::D => Some('d'),
        egui::Key::E => Some('e'),
        egui::Key::F => Some('f'),
        egui::Key::G => Some('g'),
        egui::Key::H => Some('h'),
        egui::Key::I => Some('i'),
        egui::Key::J => Some('j'),
        egui::Key::K => Some('k'),
        egui::Key::L => Some('l'),
        egui::Key::M => Some('m'),
        egui::Key::N => Some('n'),
        egui::Key::O => Some('o'),
        egui::Key::P => Some('p'),
        egui::Key::Q => Some('q'),
        egui::Key::R => Some('r'),
        egui::Key::S => Some('s'),
        egui::Key::T => Some('t'),
        egui::Key::U => Some('u'),
        egui::Key::V => Some('v'),
        egui::Key::W => Some('w'),
        egui::Key::X => Some('x'),
        egui::Key::Y => Some('y'),
        egui::Key::Z => Some('z'),
        egui::Key::Num0 => Some('0'),
        egui::Key::Num1 => Some('1'),
        egui::Key::Num2 => Some('2'),
        egui::Key::Num3 => Some('3'),
        egui::Key::Num4 => Some('4'),
        egui::Key::Num5 => Some('5'),
        egui::Key::Num6 => Some('6'),
        egui::Key::Num7 => Some('7'),
        egui::Key::Num8 => Some('8'),
        egui::Key::Num9 => Some('9'),
        egui::Key::Minus => Some('-'),
        egui::Key::Equals => Some('='),
        egui::Key::Backslash => Some('\\'),
        egui::Key::Slash => Some('/'),
        egui::Key::Period => Some('.'),
        egui::Key::Comma => Some(','),
        egui::Key::Semicolon => Some(';'),
        egui::Key::Quote => Some('\''),
        egui::Key::OpenBracket => Some('['),
        egui::Key::CloseBracket => Some(']'),
        egui::Key::Backtick => Some('`'),
        _ => None,
    }
}
