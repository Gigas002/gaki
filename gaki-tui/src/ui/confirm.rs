//! The uninstall confirmation overlay (modal popup).

use ratatui::layout::Alignment;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Paragraph;

use crate::ui::theme;

/// Build the uninstall confirmation prompt for `game_name`.
#[cfg(feature = "install")]
pub fn confirm_uninstall(game_name: &str) -> Paragraph<'static> {
    confirm(format!("Uninstall {}?", game_name))
}

/// Build the remove-friend confirmation prompt for `friend_name`.
#[cfg(feature = "social")]
pub fn confirm_remove_friend(friend_name: &str) -> Paragraph<'static> {
    confirm(format!("Remove {}?", friend_name))
}

/// Build the "not installed — install now?" prompt shown when the user tries to
/// launch a game the listing marks as not installed.
#[cfg(feature = "install")]
pub fn confirm_install(game_name: &str) -> Paragraph<'static> {
    confirm(format!("{} is not installed. Install now?", game_name))
}

/// Shared destructive-action confirmation popup: a `prompt` question over a
/// `[y] confirm / [n] cancel` hint, framed by a WARN-accented border/title.
fn confirm(prompt: String) -> Paragraph<'static> {
    let lines = vec![
        Line::from(Span::styled(prompt, theme::value())),
        Line::from(""),
        Line::from(vec![
            Span::styled("[y]", theme::key()),
            Span::styled(" confirm   ", theme::dim()),
            Span::styled("[n]", theme::key()),
            Span::styled(" cancel", theme::dim()),
        ]),
    ];

    // The danger colours signal the destructive action.
    let block = theme::danger_panel("Confirm".to_string());

    Paragraph::new(Text::from(lines))
        .block(block)
        .style(theme::base())
        .alignment(Alignment::Center)
}
