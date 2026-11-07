//! The Settings overlay (modal popup), with a tab bar over three tabs:
//!
//! * Settings: every `config.toml` key with its value (or its default,
//!   dimmed), the highlighted key's description, and a footer with the
//!   prompt, the confirm, the last save's result or the key hints;
//! * Profiles: the profiles (the default marked) beside the highlighted
//!   one's TOML, file, games and issues;
//! * Check: `config check`'s findings, errors first.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::browser::settings::{CheckTab, Edit, ProfilesTab, SPECS, SettingsView, Tab};
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

fn tab_bar(current: Tab) -> Line<'static> {
    let mut spans = Vec::new();
    for tab in Tab::ALL {
        let label = format!(" {} ", tab.label());
        spans.push(if tab == current {
            Span::styled(label, theme::selection(theme::ACCENT))
        } else {
            Span::styled(label, theme::dim())
        });
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled("[Tab] switch".to_string(), theme::dim()));
    Line::from(spans)
}

/// A wrapped footer at the bottom of `area`, at most half its height;
/// returns the area left above it.
fn with_footer(frame: &mut Frame, area: Rect, lines: Vec<Line<'static>>) -> Rect {
    let footer = Paragraph::new(lines)
        .style(theme::base())
        .wrap(Wrap { trim: false });
    let height = (footer.line_count(area.width.max(1)) as u16).min(area.height / 2);
    let [body, _, bottom] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(height),
    ])
    .areas(area);
    frame.render_widget(footer, bottom);
    body
}

/// Draw the overlay into `area`.
pub fn draw(frame: &mut Frame, area: Rect, view: &SettingsView) {
    let block = theme::panel("Settings".to_string());
    let inner = block.inner(area);
    frame.render_widget(Paragraph::new("").block(block).style(theme::base()), area);
    let [bar, _, body] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
    ])
    .areas(inner);
    frame.render_widget(Paragraph::new(tab_bar(view.tab)).style(theme::base()), bar);
    match view.tab {
        Tab::Settings => {
            let list = with_footer(frame, body, footer(view));
            frame.render_widget(
                Paragraph::new(rows(view, list.height as usize)).style(theme::base()),
                list,
            );
        }
        Tab::Profiles => draw_profiles(frame, body, view),
        Tab::Check => draw_check(frame, body, view.check.as_ref()),
    }
}

fn status_and_hint(view: &SettingsView, hint: &str) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if !view.status.is_empty() {
        lines.push(Line::from(Span::styled(
            view.status.clone(),
            theme::accent(),
        )));
    }
    lines.push(dim(hint.to_string()));
    lines
}

fn draw_profiles(frame: &mut Frame, area: Rect, view: &SettingsView) {
    let body = with_footer(
        frame,
        area,
        status_and_hint(
            view,
            "[j/k] move  [PgUp/PgDn] scroll  [d] make default  [Esc] close",
        ),
    );
    let Some(tab) = &view.profiles_tab else {
        frame.render_widget(Paragraph::new(dim("Loading…")), body);
        return;
    };
    let data = match &tab.data {
        Err(e) => {
            frame.render_widget(
                Paragraph::new(warn(format!("Couldn't read the profiles: {e}")))
                    .wrap(Wrap { trim: false }),
                body,
            );
            return;
        }
        Ok(data) => data,
    };
    let [list, detail] =
        Layout::horizontal([Constraint::Percentage(30), Constraint::Percentage(70)]).areas(body);

    let mut names: Vec<Line<'static>> = Vec::new();
    if data.rows.is_empty() {
        names.push(dim(format!("No profiles in {}.", data.dir)));
    }
    let height = (list.height as usize).max(1);
    let skip = tab.selected.saturating_sub(height - 1);
    for (i, row) in data.rows.iter().enumerate().skip(skip).take(height) {
        let selected = i == tab.selected;
        let style = if selected {
            theme::selection(theme::ACCENT)
        } else {
            theme::value()
        };
        let mut spans = vec![
            Span::styled(selection_marker(selected).to_string(), style),
            Span::styled(row.name.clone(), style),
        ];
        if row.is_default {
            spans.push(Span::styled("  default", theme::accent()));
        }
        if !row.issues.is_empty() {
            spans.push(Span::styled(" !", theme::value().fg(theme::WARN)));
        }
        names.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(names).style(theme::base()), list);

    let lines = profile_detail(tab, &data.errors);
    let paragraph = Paragraph::new(lines)
        .style(theme::base())
        .wrap(Wrap { trim: false });
    let max = paragraph.line_count(detail.width.max(1)).saturating_sub(1);
    let scroll = tab.scroll.min(max as u16);
    frame.render_widget(paragraph.scroll((scroll, 0)), detail);
}

fn profile_detail(tab: &ProfilesTab, errors: &[String]) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(row) = tab.selected_row() {
        lines.push(dim(row.path.clone()));
        lines.push(Line::from(""));
        for issue in &row.issues {
            lines.push(warn(issue.clone()));
        }
        if !row.issues.is_empty() {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(
            "Used by".to_string(),
            theme::label(),
        )));
        if row.games.is_empty() {
            lines.push(dim("  no game names it"));
        }
        for game in &row.games {
            lines.push(Line::from(Span::styled(
                format!("  {game}"),
                theme::value(),
            )));
        }
        lines.push(Line::from(""));
        for line in row.toml.lines() {
            lines.push(Line::from(Span::styled(line.to_string(), theme::value())));
        }
    }
    if !errors.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Profiles that didn't load".to_string(),
            theme::label(),
        )));
        for e in errors {
            lines.push(warn(e.clone()));
        }
    }
    lines
}

fn draw_check(frame: &mut Frame, area: Rect, check: Option<&CheckTab>) {
    let body = with_footer(
        frame,
        area,
        vec![dim("[r] check again  [j/k] scroll  [Esc] close")],
    );
    let lines: Vec<Line<'static>> = match check {
        None => vec![dim("Checking…")],
        Some(CheckTab {
            findings: Err(e), ..
        }) => vec![warn(format!("Couldn't check: {e}"))],
        Some(CheckTab {
            findings: Ok(findings),
            ..
        }) if findings.is_empty() => vec![Line::from(Span::styled(
            "No problems found.".to_string(),
            theme::value().fg(theme::ONLINE),
        ))],
        Some(CheckTab {
            findings: Ok(findings),
            ..
        }) => findings
            .iter()
            .map(|f| {
                if f.error {
                    Line::from(Span::styled(f.text.clone(), theme::item_failed()))
                } else {
                    warn(f.text.clone())
                }
            })
            .collect(),
    };
    let scroll = check.map_or(0, |c| c.scroll);
    let paragraph = Paragraph::new(lines)
        .style(theme::base())
        .wrap(Wrap { trim: false });
    let max = paragraph.line_count(body.width.max(1)).saturating_sub(1);
    frame.render_widget(paragraph.scroll((scroll.min(max as u16), 0)), body);
}
