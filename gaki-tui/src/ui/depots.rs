//! The depots overlay (modal popup): the selected game's depots, each with its
//! id/name and human-readable size. Read-only; j/k scroll.

use crate::utils::bytes::convert;

use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::browser::Browser;
use crate::ui::paneled_paragraph;
use crate::ui::theme;

/// Build the depots overlay from the browser's open depots state. Each row shows
/// the depot name (falling back to its id) and the (human-readable) size; the
/// `depots_scroll` offset selects the first visible row. Renders a single "No
/// depots." line when empty.
pub fn depots(browser: &Browser) -> Paragraph<'static> {
    let lines: Vec<Line<'static>> = if browser.depots.is_empty() {
        vec![Line::from(Span::styled(
            "No depots.".to_string(),
            theme::dim(),
        ))]
    } else {
        browser
            .depots
            .iter()
            .skip(browser.depots_scroll)
            .map(|depot| {
                let label = if depot.name.is_empty() {
                    depot.id.to_string()
                } else {
                    format!("{} ({})", depot.name, depot.id)
                };
                Line::from(vec![
                    Span::styled(label, theme::value()),
                    Span::raw("  ".to_string()),
                    Span::styled(convert(depot.size as f64), theme::dim()),
                ])
            })
            .collect()
    };

    paneled_paragraph(lines, format!("Depots ({})", browser.depots.len()))
}
