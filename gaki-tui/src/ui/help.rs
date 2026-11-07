//! The help overlay (modal popup) listing every key binding this build has.

use ratatui::layout::Alignment;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Paragraph;

use crate::ui::theme;

/// One entry in the help list: either a group header or a key binding.
enum Entry {
    Header(&'static str),
    Binding(&'static str, &'static str),
    Blank,
}

/// Width of a single column (the half the overlay devotes to one entry).
const COL_WIDTH: usize = 44;
/// Width of the key field within a binding column.
const KEY_WIDTH: usize = 16;

/// Render a single entry into the spans for one half of a row.
fn entry_spans(entry: &Entry) -> Vec<Span<'static>> {
    match entry {
        Entry::Header(title) => vec![Span::styled(
            format!("{:<width$}", title, width = COL_WIDTH),
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        )],
        Entry::Binding(keys, desc) => {
            let keys_field = format!("  {:<14}", keys);
            // Clamp the description to whatever space remains in the column so
            // the two columns stay aligned regardless of entry length.
            let desc_width = COL_WIDTH.saturating_sub(KEY_WIDTH);
            let desc: String = desc.chars().take(desc_width).collect();
            vec![
                Span::styled(keys_field, theme::key()),
                Span::styled(
                    format!("{:<width$}", desc, width = desc_width),
                    Style::default().fg(theme::TEXT),
                ),
            ]
        }
        Entry::Blank => vec![Span::raw(" ".repeat(COL_WIDTH))],
    }
}

/// The full list of help entries, shared between rendering and the row count.
fn entries() -> Vec<Entry> {
    vec![
        Entry::Header("Navigation"),
        Entry::Binding("j / \u{2193}", "move down"),
        Entry::Binding("k / \u{2191}", "move up"),
        Entry::Binding("g", "jump to top"),
        Entry::Binding("G", "jump to bottom"),
        Entry::Binding("PageUp/Down", "page up / down"),
        Entry::Binding("mouse wheel", "scroll the list"),
        Entry::Binding("Shift+drag", "select text"),
        Entry::Blank,
        Entry::Header("Filtering"),
        #[cfg(feature = "social")]
        Entry::Binding("Tab / S-Tab", "cycle tabs (+Friends)"),
        #[cfg(feature = "social")]
        Entry::Binding("1 \u{2013} 5", "jump to a tab (5=Friends)"),
        #[cfg(not(feature = "social"))]
        Entry::Binding("Tab / S-Tab", "cycle tabs"),
        #[cfg(not(feature = "social"))]
        Entry::Binding("1 \u{2013} 4", "jump to a tab"),
        Entry::Binding("/", "focus text filter"),
        Entry::Binding("Esc", "clear filter"),
        Entry::Binding("s", "cycle sort"),
        Entry::Blank,
        Entry::Header("Actions"),
        Entry::Binding("Enter", "Actions menu (all)"),
        #[cfg(feature = "versioning")]
        Entry::Binding("V", "versions & pinning"),
        #[cfg(feature = "collections")]
        Entry::Binding("O", "collections"),
        Entry::Binding("a", "achievements"),
        #[cfg(feature = "inventory")]
        Entry::Binding("I", "inventory"),
        #[cfg(feature = "market")]
        Entry::Binding("m", "market listings"),
        #[cfg(feature = "market")]
        Entry::Binding("S", "search community market"),
        #[cfg(feature = "market")]
        Entry::Binding("  Enter / Tab", "  run / price highlighted"),
        Entry::Binding("i", "expand / collapse desc"),
        #[cfg(feature = "install")]
        Entry::Binding("d", "install (pick library)"),
        #[cfg(feature = "install")]
        Entry::Binding("Space", "pause / resume install"),
        #[cfg(feature = "install")]
        Entry::Binding("c", "cancel install"),
        #[cfg(feature = "install")]
        Entry::Binding("x", "uninstall game"),
        #[cfg(feature = "install")]
        Entry::Binding("M", "move install"),
        #[cfg(feature = "install")]
        Entry::Binding("K", "relink install"),
        #[cfg(feature = "install")]
        Entry::Binding("N", "import install"),
        #[cfg(feature = "install")]
        Entry::Binding("v", "verify files"),
        #[cfg(feature = "install")]
        Entry::Binding("U", "update game"),
        Entry::Binding("D", "manage DLC"),
        #[cfg(feature = "versioning")]
        Entry::Binding("b", "beta branches"),
        #[cfg(feature = "depot")]
        Entry::Binding("o", "depots (Enter: files, / filter, b branch, d download)"),
        #[cfg(not(feature = "depot"))]
        Entry::Binding("o", "depots"),
        #[cfg(feature = "install")]
        Entry::Binding("P", "proton (d/i/u)"),
        #[cfg(not(feature = "install"))]
        Entry::Binding("P", "proton (d)"),
        Entry::Binding("R", "running games"),
        #[cfg(feature = "store")]
        Entry::Binding("Y", "store (price, players, news, reviews, similar)"),
        #[cfg(feature = "store")]
        Entry::Binding("J", "steam store (search, deals, events, wishlist)"),
        #[cfg(feature = "install")]
        Entry::Binding(
            "X",
            "installs and backlog (r/R resume, p pause, s stop, f/F forget)",
        ),
        #[cfg(feature = "workshop")]
        Entry::Binding(
            "W",
            "workshop status (i install/update, u uninstall, U update all)",
        ),
        #[cfg(feature = "workshop")]
        Entry::Binding(
            "W then b",
            "browse workshop (Tab sub, F1/F2 rate, F3 comments, F4 install)",
        ),
        #[cfg(feature = "workshop")]
        Entry::Binding("W, b, F3, c", "post a workshop comment"),
        Entry::Binding("Z", "SteamDB page (in Depots / Versions: the depot's)"),
        Entry::Binding("f", "toggle favourite"),
        Entry::Binding("H", "hide game"),
        #[cfg(feature = "cloud")]
        Entry::Binding("C", "cloud saves (s/d/u)"),
        Entry::Binding("L", "launch options"),
        Entry::Binding("E", "Steam store page"),
        Entry::Binding("B", "ProtonDB page"),
        Entry::Binding("T", "Proton issues (GitHub)"),
        Entry::Binding("r", "refresh library"),
        Entry::Binding("A", "account"),
        Entry::Binding("p", "settings"),
        #[cfg(feature = "social")]
        Entry::Binding("F / 5", "open Friends tab"),
        #[cfg(feature = "social")]
        Entry::Binding("c / Enter", "chat (Friends tab)"),
        #[cfg(feature = "social")]
        Entry::Binding("t", "chat in new window"),
        #[cfg(feature = "social")]
        Entry::Binding("a", "add friend (Friends)"),
        #[cfg(feature = "social")]
        Entry::Binding("x", "remove friend (Friends)"),
        Entry::Binding("A then o", "log out"),
        #[cfg(feature = "inventory")]
        Entry::Binding("w", "wallet balance"),
        Entry::Binding("l", "sign in again"),
        Entry::Blank,
        Entry::Header("General"),
        Entry::Binding("?", "toggle this help"),
        Entry::Binding("q", "quit"),
    ]
}

/// The number of rendered rows (each row pairs two entries side by side).
pub fn row_count() -> u16 {
    entries().len().div_ceil(2) as u16
}

/// Build the help overlay content as a two-column paragraph.
///
/// `scroll` is the vertical row offset applied so the overlay can be scrolled
/// when the bindings overflow the popup.
pub fn help(scroll: u16) -> Paragraph<'static> {
    let entries = entries();

    // Split the entries into two roughly equal halves shown side by side.
    let half = entries.len().div_ceil(2);
    let (left, right) = entries.split_at(half);

    let mut lines: Vec<Line<'static>> = Vec::with_capacity(half);
    for (row, entry) in left.iter().enumerate() {
        let mut spans = entry_spans(entry);
        spans.push(Span::raw("  "));
        if let Some(entry) = right.get(row) {
            spans.extend(entry_spans(entry));
        }
        lines.push(Line::from(spans));
    }

    let text = Text::from(lines);

    Paragraph::new(text)
        .block(theme::panel(
            "Help \u{2014} j/k scroll \u{00b7} Esc/q/? close".to_string(),
        ))
        .style(theme::base())
        .alignment(Alignment::Left)
        .scroll((scroll, 0))
}
