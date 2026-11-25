//! Colors for terminal output, and tables that stay aligned with them.
//!
//! Colors are on when stdout is a terminal, `NO_COLOR` is unset or empty,
//! `TERM` isn't `dumb`, and the command runs in this process: output the
//! daemon relays goes to a client whose terminal it can't see, so it stays
//! plain.

use std::io::IsTerminal;
use std::sync::OnceLock;

const BOLD: &str = "1";
const BLUE_BOLD: &str = "1;34";
const GREEN_BOLD: &str = "1;32";
const YELLOW_BOLD: &str = "1;33";
const RED_BOLD: &str = "1;31";

/// Whether stdout gets colors.
pub fn enabled() -> bool {
    static TERMINAL: OnceLock<bool> = OnceLock::new();
    let terminal = *TERMINAL.get_or_init(|| {
        wants_color(
            std::env::var_os("NO_COLOR").as_deref(),
            std::env::var_os("TERM").as_deref(),
            std::io::stdout().is_terminal(),
        )
    });
    terminal && !super::in_daemon()
}

/// The environment's verdict: a terminal, no `NO_COLOR`, not a dumb one.
fn wants_color(
    no_color: Option<&std::ffi::OsStr>,
    term: Option<&std::ffi::OsStr>,
    terminal: bool,
) -> bool {
    terminal && no_color.is_none_or(|v| v.is_empty()) && term.is_none_or(|t| t != "dumb")
}

fn paint(code: &str, text: &str) -> String {
    paint_if(enabled(), code, text)
}

fn paint_if(color: bool, code: &str, text: &str) -> String {
    if color {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_owned()
    }
}

pub fn bold(text: &str) -> String {
    paint(BOLD, text)
}

/// For what's in place: "ok", "moved", "linked".
pub fn green(text: &str) -> String {
    paint(GREEN_BOLD, text)
}

/// For what's worth a look: warnings, "not moved yet".
pub fn yellow(text: &str) -> String {
    paint(YELLOW_BOLD, text)
}

/// For what's wrong.
pub fn red(text: &str) -> String {
    paint(RED_BOLD, text)
}

/// `:: text`, a heading line.
pub fn heading(text: &str) -> String {
    format!("{} {}", paint(BLUE_BOLD, "::"), bold(text))
}

/// Characters on screen, without escape codes.
fn visible_len(text: &str) -> usize {
    let mut len = 0;
    let mut escape = false;
    for c in text.chars() {
        match (escape, c) {
            (false, '\x1b') => escape = true,
            (true, 'm') => escape = false,
            (true, _) => {}
            (false, _) => len += 1,
        }
    }
    len
}

/// Rows as lines with aligned columns, two spaces apart; the last column
/// isn't padded.
pub fn table(rows: &[Vec<String>]) -> Vec<String> {
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    let widths: Vec<usize> = (0..columns)
        .map(|c| {
            rows.iter()
                .filter_map(|row| row.get(c))
                .map(|cell| visible_len(cell))
                .max()
                .unwrap_or(0)
        })
        .collect();
    rows.iter()
        .map(|row| {
            let mut line = String::new();
            for (c, cell) in row.iter().enumerate() {
                line.push_str(cell);
                if c + 1 < row.len() {
                    let pad = widths[c] - visible_len(cell) + 2;
                    line.extend(std::iter::repeat_n(' ', pad));
                }
            }
            line
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    #[test]
    fn colors_need_a_terminal_and_respect_no_color() {
        assert!(wants_color(None, Some(OsStr::new("xterm")), true));
        assert!(wants_color(Some(OsStr::new("")), None, true));
        assert!(!wants_color(None, None, false));
        assert!(!wants_color(Some(OsStr::new("1")), None, true));
        assert!(!wants_color(None, Some(OsStr::new("dumb")), true));
    }

    #[test]
    fn tables_align_on_what_shows() {
        let rows = vec![
            vec!["A".to_owned(), "STATE".to_owned(), "x".to_owned()],
            vec![
                "long".to_owned(),
                paint_if(true, GREEN_BOLD, "ok"),
                "y".to_owned(),
            ],
        ];
        let lines = table(&rows);
        assert_eq!(lines[0], "A     STATE  x");
        assert_eq!(
            lines[1],
            format!("long  {}     y", paint_if(true, GREEN_BOLD, "ok"))
        );
        assert_eq!(visible_len(&lines[1]), lines[0].len());
    }
}
