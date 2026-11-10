//! The Community Market search overlay (modal popup): a single-line query input
//! over a scrollable, selectable list of resolved results. Both the search and
//! the per-result price lookup run off the UI thread; this widget is a pure
//! builder that renders whatever the worker threads have published into the
//! browse state.
//!
//! Like the Friends panel, the results list uses a *whole-list scroll* that keeps
//! the highlighted row (`browser.market_results_index`) visible: the window start
//! is derived from the index so the highlight never falls off the bottom edge.

use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Paragraph;

use crate::app::browser::Browser;
use crate::keymap::{MarketSearchAction as Ms, hints, key_of};
use crate::ui::theme;

/// Build the market search overlay content from the browse state: a query input
/// line (with a trailing caret), an optional status line, then the resolved
/// results. `visible_rows` is the height of the overlay's content area; the
/// caller passes the inner height so the result window never overflows.
pub fn market_search(browser: &Browser, visible_rows: usize) -> Paragraph<'static> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    // Query input line with a blinking-style caret.
    lines.push(Line::from(vec![
        Span::styled("Search: ".to_string(), theme::value()),
        Span::styled(browser.market_query.clone(), theme::accent()),
        Span::styled("_".to_string(), theme::cursor()),
    ]));

    // Status / price line (search progress, errors, or a price summary).
    if !browser.market_search_status.is_empty() {
        lines.push(Line::from(Span::styled(
            browser.market_search_status.clone(),
            theme::status(),
        )));
    }

    lines.push(Line::from(""));

    let results = &browser.market_results;
    if results.is_empty() {
        lines.push(Line::from(Span::styled(
            "Type a query and press [Enter] to search.".to_string(),
            theme::dim(),
        )));
    } else {
        // Reserve the rows already used by the header (input + optional status +
        // blank) so the result window fits inside the overlay.
        let header_rows = lines.len();
        let list_rows = visible_rows.saturating_sub(header_rows).max(1);

        // Whole-list scroll: keep the highlighted absolute index in the window.
        let index = browser.market_results_index;
        let start = if index >= list_rows {
            index + 1 - list_rows
        } else {
            0
        };

        for (i, r) in results.iter().enumerate().skip(start).take(list_rows) {
            let selected = i == index;
            let marker = crate::ui::selection_marker(selected);
            let name_style = if selected {
                theme::selection()
            } else {
                theme::value()
            };

            let mut spans = vec![
                Span::styled(marker, theme::accent()),
                Span::styled(r.display_name(), name_style),
            ];

            let price = r.price_text();
            if !price.is_empty() {
                spans.push(Span::styled(format!("  {}", price), theme::accent()));
            }
            if r.sell_listings > 0 {
                spans.push(Span::styled(
                    format!("  x{}", r.sell_listings),
                    theme::dim(),
                ));
            }
            if !r.app_name.is_empty() {
                spans.push(Span::styled(format!("  [{}]", r.app_name), theme::dim()));
            }

            lines.push(Line::from(spans));
        }
    }

    Paragraph::new(Text::from(lines))
        .block(theme::panel(format!(
            "Market search ({})",
            hints(&[
                ("Enter".to_string(), "search"),
                (key_of(Ms::Price), "price"),
                ("Esc".to_string(), "close"),
            ])
        )))
        .style(theme::base())
}
