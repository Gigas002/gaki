//! The wallet overlay (modal popup) showing the Steam Wallet balance.

use ratatui::layout::Alignment;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Paragraph;

use crate::app::browser::Browser;
use crate::ui::theme;

/// Build the wallet overlay content from the fetched balance.
pub fn wallet(browser: &Browser) -> Paragraph<'static> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    match &browser.wallet_info {
        Some(info) => {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                info.formatted.clone(),
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));
            if !info.country.is_empty() {
                lines.push(Line::from(Span::styled(info.country.clone(), theme::dim())));
            }
        }
        None => {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("unavailable", theme::dim())));
        }
    }

    Paragraph::new(Text::from(lines))
        .block(theme::panel(
            "Steam Wallet — press any key to close".to_string(),
        ))
        .style(theme::base())
        .alignment(Alignment::Center)
}
