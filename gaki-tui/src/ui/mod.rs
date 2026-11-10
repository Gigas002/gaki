//! Everything drawn on screen. [`draw`] renders one frame of the library
//! window, and `chat::window` one of the stand-alone chat window. The other
//! submodules are one element each: pure builders that render from the app
//! state plus the shared [`crate::ui::theme`].

pub mod account;
pub mod achievements;
pub mod actions;
#[cfg(feature = "art")]
pub mod artwork;
#[cfg(feature = "versioning")]
pub mod branches;
#[cfg(feature = "social")]
pub mod chat;
#[cfg(feature = "cloud")]
pub mod cloud;
#[cfg(feature = "collections")]
pub mod collections;
pub mod config;
#[cfg(any(feature = "install", feature = "social"))]
pub mod confirm;
pub mod depots;
pub mod detail;
pub mod dlc;
#[cfg(feature = "social")]
pub mod friend_add;
#[cfg(feature = "social")]
pub mod friends;
pub mod game_config;
pub mod game_info;
pub mod help;
#[cfg(feature = "install")]
pub mod import;
#[cfg(feature = "install")]
pub mod install_picker;
#[cfg(feature = "install")]
pub mod installs;
#[cfg(feature = "inventory")]
pub mod inventory;
pub mod launch;
pub mod launch_targets;
pub mod library;
pub mod list;
pub mod login;
#[cfg(feature = "market")]
pub mod market;
#[cfg(feature = "market")]
pub mod market_search;
#[cfg(feature = "install")]
pub mod move_game;
pub mod overlays;
pub mod proton;
#[cfg(feature = "install")]
pub mod relink;
pub mod running;
#[cfg(feature = "store")]
pub mod shop;
pub mod status;
#[cfg(feature = "store")]
pub mod store;
pub mod tabs;
pub mod theme;
#[cfg(feature = "social")]
pub mod user_profile;
#[cfg(feature = "versioning")]
pub mod versions;
#[cfg(feature = "inventory")]
pub mod wallet;
#[cfg(feature = "workshop")]
pub mod workshop;

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, ListItem, Paragraph};

use crate::app::browser::Browser;
use crate::app::client::Client;
use crate::app::{App, Mode};
use crate::utils::image::ImgBuf;

/// The art loaded for the highlighted game, if any.
#[derive(Clone, Copy, Default)]
pub struct Art<'a> {
    pub background: Option<&'a ImgBuf>,
    pub cover: Option<&'a ImgBuf>,
}

/// Draw one frame of the library window: the library itself in
/// `Mode::Browse`, and the matching sign-in or status screen otherwise.
pub fn draw(
    frame: &mut Frame,
    app: &App,
    browser: &mut Browser,
    client: &Client,
    art: Art,
    marquee_tick: usize,
) {
    frame.render_widget(Block::default().style(theme::canvas()), frame.area());
    if app.mode == Mode::Browse {
        let account = client.get_account();
        library::draw(
            frame,
            browser,
            account.as_deref(),
            art.background,
            art.cover,
            marquee_tick,
        );
    } else {
        login::draw(frame, app, client);
    }
}

/// A [`Paragraph`] framed by the standard [`theme::panel`] (titled, soft-blue
/// border) over a [`theme::base`] fill — the common scaffolding behind the
/// scrollable list overlays (achievements, depots, inventory, market, launch …).
///
/// `content` is anything convertible into a [`Text`] — a `Vec<Line>` of rows, a
/// single styled `Span`, or a plain `&str` empty-state line — so each builder
/// hands over the body it assembled and shares the block/style wiring.
pub fn paneled_paragraph(content: impl Into<Text<'static>>, title: String) -> Paragraph<'static> {
    Paragraph::new(content)
        .block(theme::panel(title))
        .style(theme::base())
}

/// A shared single-line text-entry overlay: a `label`, the typed `value` (with a
/// trailing caret), an optional `status` line, all framed by `title`.
///
/// This is the common builder behind the move/relink/import path prompts and the
/// friend search/add prompt — a pure builder from its arguments (no `Browser`
/// coupling) so each caller passes the fields it stores.
pub fn prompt_overlay(label: &str, value: &str, status: &str, title: &str) -> Paragraph<'static> {
    let mut lines: Vec<Line<'static>> = vec![
        Line::from(Span::styled(
            label.to_string(),
            Style::default().fg(theme::TEXT),
        )),
        Line::from(vec![
            Span::styled(value.to_string(), theme::accent()),
            Span::styled(
                "_".to_string(),
                Style::default()
                    .fg(theme::ACCENT_BRIGHT)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    if !status.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            status.to_string(),
            Style::default().fg(theme::WARN).add_modifier(Modifier::DIM),
        )));
    }

    Paragraph::new(Text::from(lines))
        .block(theme::panel(title.to_string()))
        .style(theme::base())
        .alignment(Alignment::Left)
}

/// The leading marker for a row in a selectable list overlay: the `▶ ` selection
/// glyph for the highlighted row, or two spaces of padding otherwise so every
/// row stays column-aligned. Centralizes the marker glyph shared by the
/// branches/dlc/proton/running/friends/market-search lists; callers still own the
/// per-list name styling, since that varies (online/installed/focus-aware).
pub fn selection_marker(selected: bool) -> &'static str {
    if selected { "▶ " } else { "  " }
}

/// The single dimmed placeholder row a list overlay shows when it has no
/// entries (e.g. "No DLC.", "No branches."). Returns a one-element `Vec` so it
/// drops straight into the `items` a [`ratatui::widgets::List`] is built from.
pub fn empty_list_rows(message: impl Into<String>) -> Vec<ListItem<'static>> {
    vec![ListItem::new(Line::from(Span::styled(
        message.into(),
        theme::dim(),
    )))]
}

/// A rectangle centered within `area`, sized as a percentage of it. Used to
/// place modal overlays (e.g. the help popup).
pub fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

/// `secs` as `YYYY-MM-DD HH:MM` (UTC).
#[cfg(any(feature = "cloud", feature = "install"))]
pub(crate) fn date_time(secs: u64) -> String {
    let secs = secs as i64;
    let day = secs.rem_euclid(86_400);
    format!(
        "{} {:02}:{:02}",
        libgaki::steam_client::unix_to_ymd(secs),
        day / 3600,
        (day % 3600) / 60
    )
}
