//! The help overlay (modal popup) listing every key binding this build has.

use ratatui::layout::Alignment;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Paragraph;

#[cfg(feature = "social")]
use crate::keymap::FriendsAction as Fr;
use crate::keymap::{self, LibraryAction as A, OverlayAction as O};
use crate::ui::theme;

/// One entry in the help list: a group header, a library action (its keys
/// read from the live keymap), or a fixed binding.
enum Entry {
    Header(&'static str),
    /// Keys read from the keymap, and what they do.
    Keys(String, &'static str),
    Binding(&'static str, &'static str),
    Blank,
}

/// The key column for library `actions`, from the keymap; `—` when none
/// is bound.
#[cfg(feature = "social")]
fn friends(action: Fr) -> String {
    match keymap::get().label(action) {
        label if label.is_empty() => "\u{2014}".to_string(),
        label => label,
    }
}

fn lib(actions: &[A]) -> String {
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
                Entry::Keys(keys, desc) => (keys.clone(), *desc),
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
        Entry::Keys(lib(&[A::Down]), "move down"),
        Entry::Keys(lib(&[A::Up]), "move up"),
        Entry::Keys(lib(&[A::Top]), "jump to top"),
        Entry::Keys(lib(&[A::Bottom]), "jump to bottom"),
        Entry::Keys(lib(&[A::PageUp, A::PageDown]), "page up / down"),
        Entry::Binding("mouse wheel", "scroll the list"),
        Entry::Binding("Shift+drag", "select text"),
        Entry::Blank,
        Entry::Header("Filtering"),
        #[cfg(feature = "social")]
        Entry::Keys(lib(&[A::NextTab, A::PreviousTab]), "cycle tabs (+Friends)"),
        #[cfg(feature = "social")]
        Entry::Keys(
            lib(&[
                A::TabAll,
                A::TabInstalled,
                A::TabUpdates,
                A::TabFavourites,
                A::TabFriends,
            ]),
            "jump to a tab (last: Friends)",
        ),
        #[cfg(not(feature = "social"))]
        Entry::Keys(lib(&[A::NextTab, A::PreviousTab]), "cycle tabs"),
        #[cfg(not(feature = "social"))]
        Entry::Keys(
            lib(&[A::TabAll, A::TabInstalled, A::TabUpdates, A::TabFavourites]),
            "jump to a tab",
        ),
        Entry::Keys(lib(&[A::Filter]), "focus text filter"),
        Entry::Binding("Esc", "clear filter"),
        Entry::Keys(lib(&[A::Sort]), "cycle sort"),
        Entry::Blank,
        Entry::Header("Actions"),
        Entry::Keys(lib(&[A::ActionsMenu]), "Actions menu (all)"),
        #[cfg(feature = "versioning")]
        Entry::Keys(lib(&[A::Versions]), "versions & pinning"),
        #[cfg(feature = "collections")]
        Entry::Keys(lib(&[A::Collections]), "collections"),
        Entry::Keys(lib(&[A::Achievements]), "achievements"),
        #[cfg(feature = "inventory")]
        Entry::Keys(lib(&[A::Inventory]), "inventory"),
        #[cfg(feature = "market")]
        Entry::Keys(lib(&[A::MarketListings]), "market listings"),
        #[cfg(feature = "market")]
        Entry::Keys(lib(&[A::MarketSearch]), "search community market"),
        Entry::Keys(lib(&[A::Description]), "expand / collapse desc"),
        #[cfg(feature = "install")]
        Entry::Keys(lib(&[A::Install]), "install (pick library)"),
        #[cfg(feature = "install")]
        Entry::Keys(lib(&[A::PauseResume]), "pause / resume install"),
        #[cfg(feature = "install")]
        Entry::Keys(lib(&[A::CancelInstall]), "cancel install"),
        #[cfg(feature = "install")]
        Entry::Keys(lib(&[A::Uninstall]), "uninstall game"),
        #[cfg(feature = "install")]
        Entry::Keys(lib(&[A::MoveInstall]), "move install"),
        #[cfg(feature = "install")]
        Entry::Keys(lib(&[A::Relink]), "relink install"),
        #[cfg(feature = "install")]
        Entry::Keys(lib(&[A::Import]), "import install"),
        #[cfg(feature = "install")]
        Entry::Keys(lib(&[A::Verify]), "verify files"),
        #[cfg(feature = "install")]
        Entry::Keys(lib(&[A::Update]), "update game"),
        Entry::Keys(lib(&[A::Dlc]), "manage DLC"),
        #[cfg(feature = "versioning")]
        Entry::Keys(lib(&[A::Branches]), "beta branches"),
        #[cfg(feature = "depot")]
        Entry::Keys(lib(&[A::Depots]), "depots and their files"),
        #[cfg(not(feature = "depot"))]
        Entry::Keys(lib(&[A::Depots]), "depots"),
        Entry::Keys(lib(&[A::Proton]), "proton runtimes"),
        Entry::Keys(lib(&[A::Running]), "running games"),
        #[cfg(feature = "store")]
        Entry::Keys(
            lib(&[A::Store]),
            "store (price, players, news, reviews, similar)",
        ),
        #[cfg(feature = "store")]
        Entry::Keys(
            lib(&[A::SteamStore]),
            "steam store (search, deals, events, wishlist)",
        ),
        #[cfg(feature = "install")]
        Entry::Keys(lib(&[A::Installs]), "installs, backlog, duplicates"),
        #[cfg(feature = "workshop")]
        Entry::Keys(lib(&[A::Workshop]), "workshop (status, browse, comments)"),
        Entry::Keys(lib(&[A::GameInfo]), "game info (details, checks, artwork)"),
        Entry::Keys(lib(&[A::SteamDb]), "SteamDB page"),
        Entry::Keys(lib(&[A::Favourite]), "toggle favourite"),
        Entry::Keys(lib(&[A::Hide]), "hide game"),
        #[cfg(feature = "cloud")]
        Entry::Keys(lib(&[A::Cloud]), "cloud saves"),
        Entry::Keys(lib(&[A::LaunchOptions]), "launch options"),
        Entry::Keys(lib(&[A::SteamPage]), "Steam store page"),
        Entry::Keys(lib(&[A::ProtonDb]), "ProtonDB page"),
        Entry::Keys(lib(&[A::ProtonIssues]), "Proton issues (GitHub)"),
        Entry::Keys(lib(&[A::Reload]), "refresh library"),
        Entry::Keys(lib(&[A::Account]), "account (and log out)"),
        Entry::Keys(
            lib(&[A::Settings]),
            "settings, profiles, check, daemons, clean",
        ),
        #[cfg(feature = "social")]
        Entry::Keys(lib(&[A::Friends, A::TabFriends]), "open Friends tab"),
        #[cfg(feature = "social")]
        Entry::Keys(friends(Fr::Chat), "chat (Friends tab)"),
        #[cfg(feature = "social")]
        Entry::Keys(friends(Fr::ChatWindow), "chat in new window"),
        #[cfg(feature = "social")]
        Entry::Keys(friends(Fr::Add), "add friend (Friends)"),
        #[cfg(feature = "social")]
        Entry::Keys(friends(Fr::Profile), "friend's profile (Friends)"),
        #[cfg(feature = "social")]
        Entry::Keys(friends(Fr::Lookup), "look up a user (Friends)"),
        #[cfg(feature = "social")]
        Entry::Keys(friends(Fr::Remove), "remove friend (Friends)"),
        #[cfg(feature = "inventory")]
        Entry::Keys(lib(&[A::Wallet]), "wallet balance"),
        Entry::Keys(lib(&[A::SignIn]), "sign in again"),
        Entry::Blank,
        Entry::Header("General"),
        Entry::Binding("?", "toggle this help"),
        Entry::Keys(lib(&[A::Quit]), "quit"),
        Entry::Blank,
        // docs/STEAM-RULES.md, kept to the column's 28 characters.
        Entry::Header("Steam rules"),
        Entry::Binding("unofficial", "Valve hasn't authorized gaki"),
        Entry::Binding("read-only", "never buys, sells or trades"),
        Entry::Binding("no bots", "don't script chat or market"),
        Entry::Binding("details", "docs/STEAM-RULES.md"),
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
        .block(theme::panel({
            let map = keymap::get();
            let mut close = vec!["Esc".to_string()];
            close.extend(map.keys(O::Close).iter().map(ToString::to_string));
            close.push("?".to_string());
            format!(
                "Help \u{2014} {} scroll \u{00b7} {} close",
                crate::keymap::pair(O::Down, O::Up),
                close.join("/")
            )
        }))
        .style(theme::base())
        .alignment(Alignment::Left)
        .scroll((scroll, 0))
}

#[cfg(test)]
mod tests;
