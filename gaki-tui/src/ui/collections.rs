//! The collections overlay (modal popup): the account's Steam library
//! collections, with the highlighted row for adding/removing the current game,
//! plus create/rename/delete and cloud pull/push/sync. Shows an inline name
//! prompt while creating or renaming, and a y/n before deleting.

use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::browser::Browser;
use crate::keymap::{CollectionsAction as C, hints, key_of};
use crate::ui::theme;
use crate::ui::{paneled_paragraph, selection_marker};

/// Build the collections overlay from the browser's open collections state. In
/// create mode it shows the name prompt; otherwise a selectable list of
/// collections (each with a game count and a dynamic marker) and a status/hint
/// line.
pub fn collections(browser: &Browser) -> Paragraph<'static> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    if let Some(input) = &browser.collections_input {
        let (label, hint) = match &browser.collections_renaming {
            Some(old) => (
                format!("Rename \"{old}\" to: "),
                "Enter to rename · Esc to cancel",
            ),
            None => (
                "New collection name: ".to_string(),
                "Enter to create · Esc to cancel",
            ),
        };
        lines.push(Line::from(vec![
            Span::styled(label, theme::label()),
            Span::styled(input.clone(), theme::accent()),
            Span::styled("_".to_string(), theme::key()),
        ]));
        lines.push(Line::from(""));
        if !browser.collections_status.is_empty() {
            lines.push(Line::from(Span::styled(
                browser.collections_status.clone(),
                theme::value().fg(theme::WARN),
            )));
        }
        lines.push(Line::from(Span::styled(hint.to_string(), theme::dim())));
        return paneled_paragraph(lines, "Collections".to_string());
    }

    if browser.collections.is_empty() {
        lines.push(Line::from(Span::styled(
            match key_of(C::New) {
                key if key.is_empty() => "No collections.".to_string(),
                key => format!("No collections. [{key}] to create one."),
            },
            theme::dim(),
        )));
    } else {
        for (i, c) in browser.collections.iter().enumerate() {
            let selected = i == browser.collections_index;
            let style = if selected {
                theme::selection(theme::ACCENT)
            } else {
                theme::value()
            };
            let count = c
                .count
                .map(|n| n.to_string())
                .unwrap_or_else(|| c.app_ids.len().to_string());
            let kind = if c.dynamic { " · dynamic" } else { "" };
            lines.push(Line::from(vec![
                Span::styled(selection_marker(selected).to_string(), style),
                Span::styled(c.name.clone(), style),
                Span::raw("  ".to_string()),
                Span::styled(format!("{count} games{kind}"), theme::dim()),
            ]));
        }
    }

    lines.push(Line::from(""));
    if let Some(name) = &browser.collections_delete {
        lines.push(Line::from(Span::styled(
            format!("Delete the collection \"{name}\"? Its games stay in your library."),
            theme::value().fg(theme::WARN),
        )));
        lines.push(Line::from(Span::styled(
            "[y] yes  [n] no".to_string(),
            theme::dim(),
        )));
        return paneled_paragraph(lines, "Collections".to_string());
    }
    let status = if !browser.collections_status.is_empty() {
        browser.collections_status.clone()
    } else {
        hints(&[
            (key_of(C::Add), "add game"),
            (key_of(C::Remove), "remove game"),
            (key_of(C::New), "new"),
            (key_of(C::Rename), "rename"),
            (key_of(C::Delete), "delete"),
            (key_of(C::Pull), "pull"),
            (key_of(C::Push), "push"),
            (key_of(C::Sync), "sync"),
            ("Esc".to_string(), "close"),
        ])
    };
    lines.push(Line::from(Span::styled(status, theme::dim())));

    paneled_paragraph(lines, "Collections".to_string())
}
