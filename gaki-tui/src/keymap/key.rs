//! One key as `keybinds.toml` writes it and the terminal reports it.

use std::fmt;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// A key with the modifiers gaki tells apart. Shift is carried by the
/// character itself (`E` is not `e`) and by `BackTab`, so it isn't a flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    pub code: KeyCode,
    pub ctrl: bool,
    pub alt: bool,
}

/// The names `keybinds.toml` accepts for keys that aren't one character,
/// matched case-insensitively. The first spelling is the one shown.
const NAMES: &[(&str, KeyCode)] = &[
    ("Enter", KeyCode::Enter),
    ("Esc", KeyCode::Esc),
    ("Escape", KeyCode::Esc),
    ("Tab", KeyCode::Tab),
    ("BackTab", KeyCode::BackTab),
    ("shift+Tab", KeyCode::BackTab),
    ("Backspace", KeyCode::Backspace),
    ("Delete", KeyCode::Delete),
    ("Del", KeyCode::Delete),
    ("Insert", KeyCode::Insert),
    ("Home", KeyCode::Home),
    ("End", KeyCode::End),
    ("PageUp", KeyCode::PageUp),
    ("PageDown", KeyCode::PageDown),
    ("Up", KeyCode::Up),
    ("Down", KeyCode::Down),
    ("Left", KeyCode::Left),
    ("Right", KeyCode::Right),
    ("Space", KeyCode::Char(' ')),
];

impl Key {
    /// A key with no modifiers.
    pub const fn plain(code: KeyCode) -> Self {
        Self {
            code,
            ctrl: false,
            alt: false,
        }
    }

    pub const fn ctrl(c: char) -> Self {
        Self {
            code: KeyCode::Char(c),
            ctrl: true,
            alt: false,
        }
    }

    /// The terminal's event, normalised: `\n` is `Enter`, and Shift is
    /// dropped (it's already in the character or in `BackTab`).
    pub fn from_event(event: KeyEvent) -> Self {
        let code = match event.code {
            KeyCode::Char('\n') | KeyCode::Char('\r') => KeyCode::Enter,
            code => code,
        };
        Self {
            code,
            ctrl: event.modifiers.contains(KeyModifiers::CONTROL),
            alt: event.modifiers.contains(KeyModifiers::ALT),
        }
    }

    /// Ctrl+C, which always quits.
    pub fn is_interrupt(self) -> bool {
        self == Self::ctrl('c')
    }

    /// Parse `"E"`, `"ctrl+r"`, `"alt+PageDown"`, `"F4"`, `"Space"`, `"+"`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut rest = text;
        let (mut ctrl, mut alt) = (false, false);
        // A lone `+` is the key itself, not a separator.
        while let Some((prefix, tail)) = rest.split_once('+').filter(|(_, t)| !t.is_empty()) {
            match prefix.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => ctrl = true,
                "alt" | "meta" => alt = true,
                // `shift+Tab` is a name of its own.
                "shift" if tail.eq_ignore_ascii_case("tab") => break,
                "shift" => {
                    return Err(format!(
                        "`{text}`: write the shifted character itself (`E`, not `shift+e`)"
                    ));
                }
                _ => return Err(format!("`{text}`: unknown modifier `{prefix}`")),
            }
            rest = tail;
        }
        let code = parse_code(rest).ok_or_else(|| format!("`{text}` is not a key"))?;
        Ok(Self { code, ctrl, alt })
    }
}

fn parse_code(text: &str) -> Option<KeyCode> {
    let mut chars = text.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return Some(KeyCode::Char(c));
    }
    if let Some((_, code)) = NAMES.iter().find(|(n, _)| n.eq_ignore_ascii_case(text)) {
        return Some(*code);
    }
    let n = text
        .strip_prefix(['F', 'f'])
        .and_then(|n| n.parse::<u8>().ok())?;
    (1..=24).contains(&n).then_some(KeyCode::F(n))
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.ctrl {
            f.write_str("ctrl+")?;
        }
        if self.alt {
            f.write_str("alt+")?;
        }
        match self.code {
            KeyCode::Char(' ') => f.write_str("Space"),
            KeyCode::Char(c) => write!(f, "{c}"),
            KeyCode::F(n) => write!(f, "F{n}"),
            code => match NAMES.iter().find(|(_, c)| *c == code) {
                Some((name, _)) => f.write_str(name),
                None => write!(f, "{code:?}"),
            },
        }
    }
}
