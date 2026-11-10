//! The depots overlay (modal popup): the selected game's depots with their
//! sizes, and with the `depot` feature the file view of one depot (entries,
//! filter, branch, download prompt). The list scrolls to keep the highlight
//! in view.

use crate::utils::bytes::convert;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};

use crate::app::browser::Browser;
use crate::app::browser::depots::Load;
use crate::keymap::{DepotsAction as D, hints, key_of};
use crate::ui::theme;
use crate::ui::{empty_list_rows, selection_marker};

/// Draw the overlay into `area`.
pub fn draw(frame: &mut Frame, area: Rect, browser: &Browser) {
    #[cfg(feature = "depot")]
    if let Some(view) = &browser.depots.files {
        files::draw(frame, area, view);
        return;
    }
    let (title, items, selected) = depot_rows(browser);
    let mut parts = Vec::new();
    if cfg!(feature = "depot") {
        parts.push((key_of(D::Files), "files"));
    }
    parts.push((key_of(D::SteamDb), "SteamDB"));
    parts.push(("Esc".to_string(), "close"));
    draw_list(
        frame,
        area,
        title,
        items,
        selected,
        vec![dim(hints(&parts))],
    );
}

fn depot_rows(browser: &Browser) -> (String, Vec<ListItem<'static>>, Option<usize>) {
    let view = &browser.depots;
    match &view.list {
        Load::Loading(_) => (
            "Depots".to_string(),
            empty_list_rows("Loading depots…"),
            None,
        ),
        Load::Failed(e) => (
            "Depots".to_string(),
            empty_list_rows(format!("Couldn't load depots: {e}")),
            None,
        ),
        Load::Ready(depots) if depots.is_empty() => (
            "Depots (0)".to_string(),
            empty_list_rows("No depots."),
            None,
        ),
        Load::Ready(depots) => {
            let items = depots
                .iter()
                .enumerate()
                .map(|(i, depot)| {
                    let selected = i == view.selected;
                    let style = if selected {
                        theme::selection()
                    } else {
                        theme::value()
                    };
                    let label = if depot.name.is_empty() {
                        depot.id.to_string()
                    } else {
                        format!("{} ({})", depot.name, depot.id)
                    };
                    ListItem::new(Line::from(vec![
                        Span::styled(selection_marker(selected).to_string(), style),
                        Span::styled(label, style),
                        Span::raw("  ".to_string()),
                        Span::styled(convert(depot.size as f64), theme::dim()),
                    ]))
                })
                .collect();
            (
                format!("Depots ({})", depots.len()),
                items,
                Some(view.selected),
            )
        }
    }
}

fn dim(text: impl Into<String>) -> Line<'static> {
    Line::from(Span::styled(text.into(), theme::dim()))
}

/// A framed list over a footer, scrolled so `selected` stays visible.
pub(crate) fn draw_list(
    frame: &mut Frame,
    area: Rect,
    title: String,
    items: Vec<ListItem<'static>>,
    selected: Option<usize>,
    footer: Vec<Line<'static>>,
) {
    let block = theme::panel(title);
    let inner = block.inner(area);
    frame.render_widget(Paragraph::new("").block(block).style(theme::base()), area);
    let [list_area, footer_area] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(footer.len() as u16 + 1),
    ])
    .areas(inner);
    let mut state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(List::new(items).style(theme::base()), list_area, &mut state);
    let mut footer_lines = vec![Line::from("")];
    footer_lines.extend(footer);
    frame.render_widget(
        Paragraph::new(footer_lines).style(theme::base()),
        footer_area,
    );
}

#[cfg(feature = "depot")]
mod files {
    use super::*;
    use crate::app::browser::depots::files::{FilesInput, FilesView};
    use libgaki::ops::depot::EntryKind;

    pub(super) fn draw(frame: &mut Frame, area: Rect, view: &FilesView) {
        let name = if view.depot_name.is_empty() {
            view.depot_id.to_string()
        } else {
            format!("{} ({})", view.depot_name, view.depot_id)
        };
        let visible = view.visible();
        let (title, items, selected) = match &view.listing {
            Load::Loading(_) => (
                format!("{name} · {}", view.branch),
                empty_list_rows("Loading the manifest…"),
                None,
            ),
            Load::Failed(e) => (
                format!("{name} · {}", view.branch),
                empty_list_rows(format!("Couldn't load the manifest: {e}")),
                None,
            ),
            Load::Ready(listing) => {
                let title = format!(
                    "{name} · {} · manifest {} · {} of {}",
                    view.branch,
                    listing.manifest_id,
                    visible.len(),
                    listing.files.len()
                );
                if visible.is_empty() {
                    let message = if listing.files.is_empty() {
                        "The manifest is empty."
                    } else {
                        "Nothing matches the filter."
                    };
                    (title, empty_list_rows(message), None)
                } else {
                    let items = visible
                        .iter()
                        .enumerate()
                        .map(|(i, entry)| {
                            let selected = i == view.selected;
                            let style = if selected {
                                theme::selection()
                            } else {
                                theme::value()
                            };
                            let (size, suffix) = match entry.kind {
                                EntryKind::Directory => ("<dir>".to_string(), "/".to_string()),
                                EntryKind::Symlink => (
                                    "<link>".to_string(),
                                    entry
                                        .link_target
                                        .as_ref()
                                        .map(|t| format!(" -> {t}"))
                                        .unwrap_or_default(),
                                ),
                                EntryKind::File => (
                                    convert(entry.size as f64),
                                    if entry.executable { "*" } else { "" }.to_string(),
                                ),
                            };
                            ListItem::new(Line::from(vec![
                                Span::styled(selection_marker(selected).to_string(), style),
                                Span::styled(format!("{size:>10}  "), theme::dim()),
                                Span::styled(format!("{}{suffix}", entry.path), style),
                            ]))
                        })
                        .collect();
                    (title, items, Some(view.selected))
                }
            }
        };
        draw_list(frame, area, title, items, selected, footer(view));
    }

    /// The prompt being typed, or the status and the key hints.
    fn footer(view: &FilesView) -> Vec<Line<'static>> {
        let prompt = |label: &str, value: &str, hint: &str| {
            vec![
                Line::from(vec![
                    Span::styled(label.to_string(), theme::label()),
                    Span::styled(value.to_string(), theme::accent()),
                    Span::styled("_".to_string(), theme::key()),
                ]),
                dim(hint),
            ]
        };
        match &view.input {
            Some(FilesInput::Filter) => prompt(
                "Filter: ",
                &view.filter,
                "text matches anywhere, * and ? are wildcards · Enter keep · Esc clear",
            ),
            Some(FilesInput::Branch(text)) => prompt(
                "Branch: ",
                text,
                "Enter load its current manifest · Esc cancel",
            ),
            Some(FilesInput::SaveTo(text)) => prompt(
                "Save to directory: ",
                text,
                "Enter download (never overwrites) · Esc cancel",
            ),
            None => {
                let mut lines = Vec::new();
                if !view.filter.is_empty() {
                    lines.push(dim(format!("Filter: {}", view.filter)));
                }
                if !view.status.is_empty() {
                    lines.push(Line::from(Span::styled(
                        view.status.clone(),
                        theme::warning(),
                    )));
                }
                {
                    use crate::keymap::{DepotFilesAction as F, OverlayAction as O, pair};
                    lines.push(dim(hints(&[
                        (key_of(F::Download), "download"),
                        (key_of(F::Filter), "filter"),
                        (key_of(F::Branch), "branch"),
                        (pair(O::PageUp, O::PageDown), "page"),
                        ("Esc".to_string(), "back"),
                    ])));
                }
                lines
            }
        }
    }
}
