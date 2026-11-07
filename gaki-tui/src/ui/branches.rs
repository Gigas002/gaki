//! The beta-branches overlay (modal popup): the selected game's branches with
//! the currently-active one marked, and the highlighted row for switching.

use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem};

use crate::app::browser::Browser;
use crate::keymap::{BranchesAction as Br, hints, key_of};
use crate::ui::theme;

/// Build the branches overlay list from the browser's open branches state. Each
/// row shows the branch name, a `●` active marker, and an optional description;
/// the highlighted row gets a bright selection bar with a `▶` marker. Renders a
/// single "No branches." row when empty.
pub fn branches(browser: &Browser) -> List<'static> {
    let items: Vec<ListItem<'static>> = if browser.branches.is_empty() {
        crate::ui::empty_list_rows("No branches.")
    } else {
        browser
            .branches
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let selected = i == browser.branch_index;

                let marker = crate::ui::selection_marker(selected);
                let name_style = if selected {
                    theme::selection(theme::ACCENT)
                } else {
                    theme::value()
                };

                let mut spans = vec![
                    Span::styled(marker.to_string(), name_style),
                    Span::styled(entry.name.clone(), name_style),
                ];

                // Active branch marker (Steam green dot).
                if entry.active {
                    spans.push(Span::raw("  ".to_string()));
                    spans.push(Span::styled(
                        "●".to_string(),
                        theme::value().fg(theme::ONLINE),
                    ));
                }

                // Optional branch description, dimmed.
                if !entry.description.is_empty() {
                    spans.push(Span::raw("  ".to_string()));
                    spans.push(Span::styled(entry.description.clone(), theme::dim()));
                }

                ListItem::new(Line::from(spans))
            })
            .collect()
    };

    List::new(items)
        .block(theme::panel(format!(
            "Branches ({})",
            hints(&[(key_of(Br::Switch), "switch")])
        )))
        .style(theme::base())
}
