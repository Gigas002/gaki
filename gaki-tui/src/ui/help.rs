//! The help overlay (modal popup) listing every key binding this build has.

use ratatui::layout::Alignment;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Paragraph;

use crate::keymap::{self, LibraryAction as A};
use crate::ui::theme;

/// One entry in the help list: a group header, a library action (its keys
/// read from the live keymap), or a fixed binding.
enum Entry {
    Header(&'static str),
    /// Library actions sharing a line; their keys are joined with `/`.
    Keys(&'static [A], &'static str),
    Binding(&'static str, &'static str),
    Blank,
}

/// The key column for `actions`, from the keymap; `—` when none is bound.
fn keys_label(actions: &[A]) -> String {
    let map = keymap::get();
    let labels: Vec<String> = actions
        .iter()
        .map(|&a| map.label(a))
        .filter(|l| !l.is_empty())
        .collect();
    if labels.is_empty() {
        "\u{2014}".to_string()
    } else {
        labels.join("/")
    }
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
        Entry::Keys(..) | Entry::Binding(..) => {
            let (keys, desc) = match entry {
                Entry::Keys(actions, desc) => (keys_label(actions), *desc),
                Entry::Binding(keys, desc) => (keys.to_string(), *desc),
                _ => unreachable!(),
            };
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
        Entry::Keys(&[A::Down], "move down"),
        Entry::Keys(&[A::Up], "move up"),
        Entry::Keys(&[A::Top], "jump to top"),
        Entry::Keys(&[A::Bottom], "jump to bottom"),
        Entry::Keys(&[A::PageUp, A::PageDown], "page up / down"),
        Entry::Binding("mouse wheel", "scroll the list"),
        Entry::Binding("Shift+drag", "select text"),
        Entry::Blank,
        Entry::Header("Filtering"),
        #[cfg(feature = "social")]
        Entry::Keys(&[A::NextTab, A::PreviousTab], "cycle tabs (+Friends)"),
        #[cfg(feature = "social")]
        Entry::Keys(
            &[
                A::TabAll,
                A::TabInstalled,
                A::TabUpdates,
                A::TabFavourites,
                A::TabFriends,
            ],
            "jump to a tab (5=Friends)",
        ),
        #[cfg(not(feature = "social"))]
        Entry::Keys(&[A::NextTab, A::PreviousTab], "cycle tabs"),
        #[cfg(not(feature = "social"))]
        Entry::Keys(
            &[A::TabAll, A::TabInstalled, A::TabUpdates, A::TabFavourites],
            "jump to a tab",
        ),
        Entry::Keys(&[A::Filter], "focus text filter"),
        Entry::Binding("Esc", "clear filter"),
        Entry::Keys(&[A::Sort], "cycle sort"),
        Entry::Blank,
        Entry::Header("Actions"),
        Entry::Keys(&[A::ActionsMenu], "Actions menu (all)"),
        #[cfg(feature = "versioning")]
        Entry::Keys(&[A::Versions], "versions & pinning"),
        #[cfg(feature = "collections")]
        Entry::Keys(
            &[A::Collections],
            "collections (a/r add/remove game, n new, e rename, x delete)",
        ),
        Entry::Keys(&[A::Achievements], "achievements"),
        #[cfg(feature = "inventory")]
        Entry::Keys(&[A::Inventory], "inventory"),
        #[cfg(feature = "market")]
        Entry::Keys(&[A::MarketListings], "market listings"),
        #[cfg(feature = "market")]
        Entry::Keys(&[A::MarketSearch], "search community market"),
        #[cfg(feature = "market")]
        Entry::Binding("  Enter / Tab", "  run / price highlighted"),
        Entry::Keys(&[A::Description], "expand / collapse desc"),
        #[cfg(feature = "install")]
        Entry::Keys(&[A::Install], "install (pick library)"),
        #[cfg(feature = "install")]
        Entry::Keys(&[A::PauseResume], "pause / resume install"),
        #[cfg(feature = "install")]
        Entry::Keys(&[A::CancelInstall], "cancel install"),
        #[cfg(feature = "install")]
        Entry::Keys(&[A::Uninstall], "uninstall game"),
        #[cfg(feature = "install")]
        Entry::Keys(&[A::MoveInstall], "move install"),
        #[cfg(feature = "install")]
        Entry::Keys(&[A::Relink], "relink install"),
        #[cfg(feature = "install")]
        Entry::Keys(&[A::Import], "import install"),
        #[cfg(feature = "install")]
        Entry::Keys(&[A::Verify], "verify files"),
        #[cfg(feature = "install")]
        Entry::Keys(&[A::Update], "update game"),
        Entry::Keys(&[A::Dlc], "manage DLC"),
        #[cfg(feature = "versioning")]
        Entry::Keys(&[A::Branches], "beta branches"),
        #[cfg(feature = "depot")]
        Entry::Keys(
            &[A::Depots],
            "depots (Enter: files, / filter, b branch, d download)",
        ),
        #[cfg(not(feature = "depot"))]
        Entry::Keys(&[A::Depots], "depots"),
        #[cfg(feature = "install")]
        Entry::Keys(&[A::Proton], "proton (d/i/u)"),
        #[cfg(not(feature = "install"))]
        Entry::Keys(&[A::Proton], "proton (d)"),
        Entry::Keys(&[A::Running], "running games"),
        #[cfg(feature = "store")]
        Entry::Keys(
            &[A::Store],
            "store (price, players, news, reviews, similar)",
        ),
        #[cfg(feature = "store")]
        Entry::Keys(
            &[A::SteamStore],
            "steam store (search, deals, events, wishlist)",
        ),
        #[cfg(feature = "install")]
        Entry::Keys(
            &[A::Installs],
            "installs and backlog (r/R resume, p pause, s stop, f/F forget)",
        ),
        #[cfg(feature = "workshop")]
        Entry::Keys(
            &[A::Workshop],
            "workshop status (i install/update, u uninstall, U update all)",
        ),
        #[cfg(feature = "workshop")]
        Entry::Binding(
            "W then b",
            "browse workshop (Tab sub, F1/F2 rate, F3 comments, F4 install)",
        ),
        #[cfg(feature = "workshop")]
        Entry::Binding("W, b, F3, c", "post a workshop comment"),
        Entry::Keys(
            &[A::GameInfo],
            "game info (Tab: details, DRM/files checks, save artwork)",
        ),
        Entry::Keys(
            &[A::SteamDb],
            "SteamDB page (in Depots / Versions: the depot's)",
        ),
        Entry::Keys(&[A::Favourite], "toggle favourite"),
        Entry::Keys(&[A::Hide], "hide game"),
        #[cfg(feature = "cloud")]
        Entry::Keys(
            &[A::Cloud],
            "cloud saves (s/d/u sync, c/l keep cloud/local)",
        ),
        Entry::Keys(&[A::LaunchOptions], "launch options"),
        Entry::Keys(&[A::SteamPage], "Steam store page"),
        Entry::Keys(&[A::ProtonDb], "ProtonDB page"),
        Entry::Keys(&[A::ProtonIssues], "Proton issues (GitHub)"),
        Entry::Keys(&[A::Reload], "refresh library"),
        Entry::Keys(&[A::Account], "account"),
        Entry::Keys(
            &[A::Settings],
            "settings (Tab: keys, profiles, config check, daemons; Enter edit, x default)",
        ),
        #[cfg(feature = "social")]
        Entry::Keys(&[A::Friends, A::TabFriends], "open Friends tab"),
        #[cfg(feature = "social")]
        Entry::Binding("c / Enter", "chat (Friends tab)"),
        #[cfg(feature = "social")]
        Entry::Binding("t", "chat in new window"),
        #[cfg(feature = "social")]
        Entry::Binding("a", "add friend (Friends)"),
        #[cfg(feature = "social")]
        Entry::Binding("v", "friend's profile (Friends)"),
        #[cfg(feature = "social")]
        Entry::Binding("/", "look up a user (Friends)"),
        #[cfg(feature = "social")]
        Entry::Binding("x", "remove friend (Friends)"),
        Entry::Binding("A then o", "log out"),
        #[cfg(feature = "inventory")]
        Entry::Keys(&[A::Wallet], "wallet balance"),
        Entry::Keys(&[A::SignIn], "sign in again"),
        Entry::Blank,
        Entry::Header("General"),
        Entry::Binding("?", "toggle this help"),
        Entry::Keys(&[A::Quit], "quit"),
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
