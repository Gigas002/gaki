//! The footer status bar: counts, active filter/query/sort, session, and a
//! contextual key-hint line.

use ratatui::layout::{Alignment, Rect};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Paragraph;

use crate::app::browser::Browser;
use crate::keymap::{self, LibraryAction as A};
use crate::ui::theme;

/// A dim "  ·  " separator span.
fn sep() -> Span<'static> {
    Span::styled("  ·  ", theme::dim())
}

/// The footer's key hints (line 2): the action whose first key is shown
/// (`None` for the fixed `?`), and its description.
const HINTS: &[(Option<A>, &str)] = &[
    (Some(A::Filter), " filter  "),
    (Some(A::NextTab), " view  "),
    (Some(A::Sort), " sort  "),
    (Some(A::ActionsMenu), " actions  "),
    #[cfg(feature = "install")]
    (Some(A::Install), " install  "),
    (Some(A::Favourite), " fav  "),
    (Some(A::Reload), " refresh  "),
    (None, " help  "),
    (Some(A::Quit), " quit"),
];

/// The Friends tab's hints, like [`HINTS`].
#[cfg(feature = "social")]
const FRIENDS_HINTS: &[(keymap::FriendsAction, &str)] = {
    use keymap::FriendsAction as F;
    &[
        (F::Chat, " chat  "),
        (F::ChatWindow, " window  "),
        (F::Add, " add  "),
        (F::Profile, " profile  "),
        (F::Wishlist, " wishlist  "),
        (F::Remove, " remove  "),
    ]
};

/// The hints for the current view as `([key], description)`, from the live
/// keymap. An action with no key is left out.
fn hints(browser: &Browser) -> Vec<(String, &'static str)> {
    let map = keymap::get();
    let hint = |key: String, desc| (!key.is_empty()).then(|| (format!("[{key}]"), desc));
    #[cfg(feature = "social")]
    if browser.view == crate::app::browser::View::Friends {
        let mut out: Vec<_> = FRIENDS_HINTS
            .iter()
            .filter_map(|&(action, desc)| hint(map.first(action), desc))
            .collect();
        out.extend(hint(map.first(A::NextTab), " view  "));
        out.extend(hint("?".to_string(), " help  "));
        out.extend(hint(map.first(A::Quit), " quit"));
        return out;
    }
    #[cfg(not(feature = "social"))]
    let _ = browser;
    HINTS
        .iter()
        .filter_map(|&(action, desc)| {
            let key = match action {
                Some(action) => map.first(action),
                None => "?".to_string(),
            };
            hint(key, desc)
        })
        .collect()
}

/// The footer's version badge text.
fn version_badge_text() -> String {
    format!("gaki-tui v{}", env!("CARGO_PKG_VERSION"))
}

/// Where the version badge fits in a footer `area`: the bottom row when the
/// key hints leave room for it, else the top row when the status line does,
/// else nowhere (`None`): it never covers either.
pub fn version_badge_area(area: Rect, browser: &Browser, account: Option<&str>) -> Option<Rect> {
    let hints: usize = hints(browser)
        .iter()
        .map(|(k, d)| k.chars().count() + d.chars().count())
        .sum();
    let badge = version_badge_text().chars().count();
    let width = usize::from(area.width);
    let row = |y| Rect {
        y,
        height: 1,
        ..area
    };
    if area.height > 1 && width >= hints + badge + 2 {
        Some(row(area.y + area.height - 1))
    } else if area.height > 0 && width >= status_line(browser, account).width() + badge + 2 {
        Some(row(area.y))
    } else {
        None
    }
}

/// The version badge for the footer's bottom-right corner, in dim text.
pub fn version_badge() -> Paragraph<'static> {
    Paragraph::new(Line::from(Span::styled(version_badge_text(), theme::dim())))
        .alignment(Alignment::Right)
}

/// Build the footer status bar (a borderless two-line footer).
pub fn status_bar(browser: &Browser, account: Option<&str>) -> Paragraph<'static> {
    // --- Line 2: key hints ---
    let mut line2: Vec<Span<'static>> = Vec::new();
    for (cap, desc) in hints(browser) {
        line2.push(Span::styled(cap, theme::key()));
        line2.push(Span::styled(desc.to_string(), theme::dim()));
    }

    let text = Text::from(vec![status_line(browser, account), Line::from(line2)]);

    Paragraph::new(text).style(theme::base())
}

/// The footer's first line: notices, the filter, counts, sort and account.
fn status_line(browser: &Browser, account: Option<&str>) -> Line<'static> {
    let c = browser.counts();

    // --- Line 1: status ---
    let mut line1: Vec<Span<'static>> = Vec::new();

    // A transient notice (e.g. a failed action) takes the front of the line in
    // the warning colour until the next keypress clears it.
    if let Some(notice) = &browser.notice {
        line1.push(Span::styled(notice.clone(), theme::warning()));
        line1.push(sep());
    }

    // Live query, shown prominently when filtering or a query is present.
    if browser.filtering || !browser.query.is_empty() {
        let mut q = format!("/{}", browser.query);
        if browser.filtering {
            q.push('▏');
        }
        line1.push(Span::styled(q, theme::accent()));
        line1.push(sep());
    }

    line1.push(Span::styled(
        format!("{}/{} shown", c.visible, c.total),
        theme::value(),
    ));

    line1.push(sep());
    line1.push(Span::styled(
        format!("● {} installed", c.installed),
        theme::good(),
    ));

    line1.push(sep());
    let updates = if c.checking_updates {
        format!("▲ {} updates (checking…)", c.updates)
    } else {
        format!("▲ {} updates", c.updates)
    };
    line1.push(Span::styled(updates, theme::warning()));

    if c.downloading > 0 {
        line1.push(sep());
        line1.push(Span::styled(
            format!("⬇ {} downloading", c.downloading),
            theme::link(),
        ));
    }

    line1.push(sep());
    line1.push(Span::styled(
        format!("sort: {}", browser.sort.label()),
        theme::dim(),
    ));

    if let Some(a) = account {
        line1.push(sep());
        line1.push(Span::styled(format!("steam: {}", a), theme::accent()));
    }
    Line::from(line1)
}
