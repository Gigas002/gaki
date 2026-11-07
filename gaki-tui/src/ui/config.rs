//! The Settings overlay (modal popup): every `config.toml` key with its value
//! (or its default, dimmed), the highlighted key's description, and a footer
//! with the prompt, the confirm, the last save's result or the key hints.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::browser::settings::{Edit, SPECS, SettingsView};
use crate::ui::{selection_marker, theme};

fn dim(text: impl Into<String>) -> Line<'static> {
    Line::from(Span::styled(text.into(), theme::dim()))
}

fn warn(text: impl Into<String>) -> Line<'static> {
    Line::from(Span::styled(text.into(), theme::value().fg(theme::WARN)))
}

/// How `Enter` edits a key, for the description line.
fn how(edit: Edit) -> &'static str {
    match edit {
        Edit::Toggle { .. } => "Enter switches it",
        Edit::Choice { .. } | Edit::Profile => "Enter picks the next value",
        Edit::Prompt => "Enter edits it",
    }
}

/// The key rows, the highlight kept within `height` rows.
fn rows(view: &SettingsView, height: usize) -> Vec<Line<'static>> {
    let height = height.max(1);
    let skip = view.selected.saturating_sub(height - 1);
    SPECS
        .iter()
        .enumerate()
        .skip(skip)
        .take(height)
        .map(|(i, spec)| {
            let selected = i == view.selected;
            let style = if selected {
                theme::selection(theme::ACCENT)
            } else {
                theme::label()
            };
            let editing = selected.then_some(view.input.as_ref()).flatten();
            let value = match (editing, view.value(i)) {
                (Some(text), _) => Span::styled(format!("{text}_"), theme::accent()),
                (None, Some(value)) => Span::styled(value.to_string(), theme::value()),
                (None, None) => Span::styled(format!("({})", spec.default), theme::dim()),
            };
            Line::from(vec![
                Span::styled(selection_marker(selected).to_string(), style),
                Span::styled(format!("{:<30}", spec.label), style),
                value,
            ])
        })
        .collect()
}

fn footer(view: &SettingsView) -> Vec<Line<'static>> {
    let spec = SPECS[view.selected.min(SPECS.len() - 1)];
    let mut lines = vec![dim(format!(
        "{} · default: {} · {}",
        spec.key,
        spec.default,
        how(spec.edit)
    ))];
    if let Some(pending) = &view.confirm {
        let question = SPECS[pending.index].confirm.unwrap_or("Save this change?");
        lines.push(warn(question));
        lines.push(dim("[y] yes  [n] no"));
        return lines;
    }
    if view.input.is_some() {
        lines.push(dim("Enter save · Esc cancel · empty unsets it"));
        return lines;
    }
    if let Some(e) = &view.error {
        lines.push(warn(format!("Couldn't read config.toml: {e}")));
    } else if !view.status.is_empty() {
        lines.push(Line::from(Span::styled(
            view.status.clone(),
            theme::accent(),
        )));
    }
    lines.push(dim(
        "[j/k] move  [Enter] edit  [x] back to the default  [Esc] close",
    ));
    lines
}

/// Draw the overlay into `area`.
pub fn draw(frame: &mut Frame, area: Rect, view: &SettingsView) {
    let block = theme::panel("Settings · config.toml".to_string());
    let inner = block.inner(area);
    frame.render_widget(Paragraph::new("").block(block).style(theme::base()), area);
    let footer = Paragraph::new(footer(view))
        .style(theme::base())
        .wrap(Wrap { trim: false });
    let height = (footer.line_count(inner.width.max(1)) as u16).min(inner.height / 2);
    let [list, _, bottom] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(height),
    ])
    .areas(inner);
    frame.render_widget(
        Paragraph::new(rows(view, list.height as usize)).style(theme::base()),
        list,
    );
    frame.render_widget(footer, bottom);
}
