//! The Settings overlay (modal popup), with a tab bar over four tabs:
//!
//! * Settings: every `config.toml` key with its value (or its default,
//!   dimmed), the highlighted key's description, and a footer with the
//!   prompt, the confirm, the last save's result or the key hints;
//! * Profiles: the profiles (the default marked) beside the highlighted
//!   one's TOML, file, games and issues;
//! * Check: `config check`'s findings, errors first;
//! * Daemons: the running session daemons, the highlighted one's details,
//!   and the stop confirm.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::browser::settings::{
    CheckTab, CleanTab, DaemonsTab, Edit, ProfilesTab, SPECS, SettingsView, Tab,
};
use crate::keymap::{OverlayAction as O, SettingsAction as S, hints, key_of, pair};
use crate::ui::{selection_marker, theme};

fn dim(text: impl Into<String>) -> Line<'static> {
    Line::from(Span::styled(text.into(), theme::dim()))
}

fn warn(text: impl Into<String>) -> Line<'static> {
    Line::from(Span::styled(text.into(), theme::warning()))
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
                theme::selection()
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
    lines.push(dim(hints(&[
        (pair(O::Down, O::Up), "move"),
        (key_of(S::Edit), "edit"),
        (key_of(S::Unset), "back to the default"),
        ("Esc".to_string(), "close"),
    ])));
    lines
}

fn tab_bar(current: Tab) -> Line<'static> {
    let mut spans = Vec::new();
    for tab in Tab::ALL {
        let label = format!(" {} ", tab.label());
        spans.push(if tab == current {
            Span::styled(label, theme::selection())
        } else {
            Span::styled(label, theme::dim())
        });
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(
        hints(&[(key_of(O::NextTab), "switch")]),
        theme::dim(),
    ));
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
        Tab::Daemons => draw_daemons(frame, body, view),
        Tab::Clean => draw_clean(frame, body, view),
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
            &hints(&[
                (pair(O::Down, O::Up), "move"),
                (pair(O::PageUp, O::PageDown), "scroll"),
                (key_of(S::MakeDefault), "make default"),
                ("Esc".to_string(), "close"),
            ]),
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
            theme::selection()
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
            spans.push(Span::styled(" !", theme::warning()));
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
        vec![dim(hints(&[
            (key_of(S::CheckAgain), "check again"),
            (pair(O::Down, O::Up), "scroll"),
            ("Esc".to_string(), "close"),
        ]))],
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
            theme::good(),
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

fn clean_footer(view: &SettingsView, tab: Option<&CleanTab>) -> Vec<Line<'static>> {
    if let Some(tab) = tab.filter(|t| t.confirm) {
        let total: u64 = tab
            .scan
            .ready()
            .map_or(0, |items| items.iter().map(|i| i.bytes).sum());
        return vec![
            warn(format!(
                "Delete all of this ({})? Caches fill again as gaki needs them.",
                libgaki::core::utils::human_bytes(total)
            )),
            dim("[y] yes  [n] no"),
        ];
    }
    let shaders = if tab.is_some_and(|t| t.all_shaders) {
        "only uninstalled games' shaders"
    } else {
        "every game's shaders"
    };
    status_and_hint(
        view,
        &hints(&[
            (key_of(S::Clean), "delete"),
            (key_of(S::AllShaders), shaders),
            (key_of(S::RescanClean), "scan again"),
            ("Esc".to_string(), "close"),
        ]),
    )
}

fn draw_clean(frame: &mut Frame, area: Rect, view: &SettingsView) {
    use crate::app::browser::job::Load;
    use libgaki::core::utils::human_bytes;
    let tab = view.clean.as_ref();
    let body = with_footer(frame, area, clean_footer(view, tab));
    let lines: Vec<Line<'static>> = match tab.map(|t| &t.scan) {
        None | Some(Load::Loading(_)) => vec![dim("Looking for caches and leftovers…")],
        Some(Load::Failed(e)) => vec![warn(format!("Couldn't look: {e}"))],
        Some(Load::Ready(items)) if items.is_empty() => vec![Line::from(Span::styled(
            "Nothing to clean.".to_string(),
            theme::good(),
        ))],
        Some(Load::Ready(items)) => {
            let mut lines: Vec<Line<'static>> = libgaki::ops::clean::totals(items)
                .into_iter()
                .map(|(kind, (count, bytes))| {
                    Line::from(vec![
                        Span::styled(format!("{:<10}", kind.as_str()), theme::label()),
                        Span::styled(format!("{:>11}", human_bytes(bytes)), theme::value()),
                        Span::styled(format!("  {count} item(s)"), theme::dim()),
                    ])
                })
                .collect();
            let total: u64 = items.iter().map(|i| i.bytes).sum();
            lines.push(Line::from(vec![
                Span::styled(format!("{:<10}", "total"), theme::label()),
                Span::styled(format!("{:>11}", human_bytes(total)), theme::accent()),
            ]));
            lines.push(Line::from(""));
            lines.push(dim(
                "Never touched: config, session, installs, saves, prefixes, partial downloads."
                    .to_string(),
            ));
            lines
        }
    };
    frame.render_widget(
        Paragraph::new(lines)
            .style(theme::base())
            .wrap(Wrap { trim: false }),
        body,
    );
}

fn daemons_footer(view: &SettingsView, tab: Option<&DaemonsTab>) -> Vec<Line<'static>> {
    if let Some(confirm) = tab.and_then(|t| t.confirm.as_ref()) {
        let question = match confirm.pids.as_slice() {
            [pid] if !confirm.all => format!("Stop the daemon with PID {pid}?"),
            pids => format!("Stop all {} daemons?", pids.len()),
        };
        return vec![
            warn(format!(
                "{question} Its clients reconnect to a fresh one when they next need it."
            )),
            dim("[y] yes  [n] no"),
        ];
    }
    status_and_hint(
        view,
        &hints(&[
            (pair(O::Down, O::Up), "move"),
            (key_of(S::StopDaemon), "stop"),
            (key_of(S::StopAllDaemons), "stop all"),
            (key_of(S::RefreshDaemons), "refresh"),
            ("Esc".to_string(), "close"),
        ]),
    )
}

fn draw_daemons(frame: &mut Frame, area: Rect, view: &SettingsView) {
    let tab = view.daemons.as_ref();
    let body = with_footer(frame, area, daemons_footer(view, tab));
    let Some(tab) = tab else {
        frame.render_widget(Paragraph::new(dim("Reading the process list…")), body);
        return;
    };
    if tab.rows.is_empty() {
        frame.render_widget(
            Paragraph::new(dim("No gaki-cli daemon is running.")).style(theme::base()),
            body,
        );
        return;
    }
    let [list, _, detail] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(5),
    ])
    .areas(body);

    let mut lines = vec![Line::from(Span::styled(
        format!("  {:<9}{:<32}{}", "PID", "CONFIG DIR", "SOCKET"),
        theme::label(),
    ))];
    let height = (list.height as usize).saturating_sub(1).max(1);
    let skip = tab.selected.saturating_sub(height - 1);
    for (i, row) in tab.rows.iter().enumerate().skip(skip).take(height) {
        let selected = i == tab.selected;
        let style = if selected {
            theme::selection()
        } else {
            theme::value()
        };
        lines.push(Line::from(vec![
            Span::styled(selection_marker(selected).to_string(), style),
            Span::styled(format!("{:<9}", row.pid), style),
            Span::styled(
                format!("{:<32}", row.config_dir.as_deref().unwrap_or("(default)")),
                theme::value(),
            ),
            Span::styled(
                row.socket
                    .clone()
                    .unwrap_or_else(|| "(default)".to_string()),
                theme::value(),
            ),
        ]));
    }
    frame.render_widget(Paragraph::new(lines).style(theme::base()), list);

    if let Some(row) = tab.selected_row() {
        let detail_lines = vec![
            Line::from(Span::styled("Command".to_string(), theme::label())),
            Line::from(Span::styled(row.command.clone(), theme::value())),
        ];
        frame.render_widget(
            Paragraph::new(detail_lines)
                .style(theme::base())
                .wrap(Wrap { trim: false }),
            detail,
        );
    }
}
