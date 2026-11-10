//! The Steam Cloud overlay: lists the selected game's Cloud save files, with
//! keys to sync them (`s` both ways, `d` download-only, `u` upload-only).
//! After a sync leaves diverged saves, they're listed with each side's size
//! and time, the newer side marked, and `c` / `l` keep the cloud or the
//! local copies.

use crate::utils::bytes::convert;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table, Wrap};

use crate::app::browser::Browser;
use crate::app::steam::CloudResolve;
use crate::keymap::{CloudAction as C, hints, key_of};
use crate::ui::{date_time, theme};

/// The lines above the file table: the status, then the conflicts with the
/// resolve confirm or its keys. Wrapped, so long messages stay whole.
fn header_lines(browser: &Browser) -> Vec<Line<'static>> {
    let state = &browser.cloud_state;
    let mut lines = Vec::new();
    if !browser.cloud_status.is_empty() {
        lines.push(Line::from(Span::styled(
            browser.cloud_status.clone(),
            theme::accent(),
        )));
    }
    if state.conflicts.is_empty() {
        return lines;
    }
    lines.push(Line::from(Span::styled(
        format!(
            "Differ between Steam Cloud and here ({}):",
            state.conflicts.len()
        ),
        theme::label(),
    )));
    for c in &state.conflicts {
        lines.push(Line::from(Span::styled(
            format!("  {}", c.filename),
            theme::warning(),
        )));
        let local_newer = c.local_timestamp > c.cloud_timestamp;
        let cloud_newer = c.cloud_timestamp > c.local_timestamp;
        lines.push(side("local", c.local_size, c.local_timestamp, local_newer));
        lines.push(side("cloud", c.cloud_size, c.cloud_timestamp, cloud_newer));
    }
    lines.push(match state.confirm {
        Some(CloudResolve::KeepCloud) => {
            warn("Overwrite the local copies with the cloud ones? [y] yes  [n] no")
        }
        Some(CloudResolve::KeepLocal) => {
            warn("Overwrite the cloud copies with the local ones? [y] yes  [n] no")
        }
        _ if state.sync.is_some() => Line::from(""),
        _ => Line::from(Span::styled(
            hints(&[
                (key_of(C::KeepCloud), "keep the cloud copies"),
                (key_of(C::KeepLocal), "keep the local copies"),
            ]),
            theme::dim(),
        )),
    });
    lines
}

fn warn(text: &str) -> Line<'static> {
    Line::from(Span::styled(text.to_string(), theme::warning()))
}

/// One side of a conflict: `label  size  time`, plus `newer` when it is.
fn side(label: &str, size: u64, time: u64, newer: bool) -> Line<'static> {
    let style = if newer { theme::good() } else { theme::dim() };
    let mark = if newer { "  newer" } else { "" };
    Line::from(Span::styled(
        format!(
            "    {label}  {:>10}  {} UTC{mark}",
            convert(size as f64),
            date_time(time)
        ),
        style,
    ))
}

/// Draw the overlay into `area`: the header lines, then the file table.
pub fn draw(frame: &mut Frame, area: Rect, browser: &Browser) {
    let block = theme::panel(format!(
        "Steam Cloud ({})",
        hints(&[
            (key_of(C::Sync), "sync"),
            (key_of(C::Download), "down"),
            (key_of(C::Upload), "up"),
            ("Esc".to_string(), "close"),
        ])
    ));
    let inner = block.inner(area);
    frame.render_widget(Paragraph::new("").block(block).style(theme::base()), area);

    let header = Paragraph::new(header_lines(browser))
        .style(theme::base())
        .wrap(Wrap { trim: false });
    let height = (header.line_count(inner.width.max(1)) as u16).min(inner.height / 2 + 1);
    let [top, _, bottom] = Layout::vertical([
        Constraint::Length(height),
        Constraint::Length(u16::from(height > 0)),
        Constraint::Min(1),
    ])
    .areas(inner);
    frame.render_widget(header, top);
    frame.render_widget(files(browser), bottom);
}

/// The file table.
fn files(browser: &Browser) -> Table<'static> {
    let mut rows: Vec<Row<'static>> = Vec::new();
    if browser.cloud_files.is_empty() {
        if !browser.cloud_state.is_busy() {
            rows.push(Row::new(vec![Cell::from(Span::styled(
                "No cloud files.",
                theme::dim(),
            ))]));
        }
    } else {
        for file in &browser.cloud_files {
            let modified = if file.timestamp > 0 {
                date_time(file.timestamp as u64)
            } else {
                String::new()
            };
            rows.push(Row::new(vec![
                Cell::from(Span::styled(file.filename.clone(), theme::value())),
                Cell::from(Span::styled(convert(file.size as f64), theme::dim())),
                Cell::from(Span::styled(modified, theme::dim())),
            ]));
        }
    }
    Table::default()
        .rows(rows)
        .header(Row::new(vec![
            Cell::from(Span::styled("NAME", theme::label())),
            Cell::from(Span::styled("SIZE", theme::label())),
            Cell::from(Span::styled("MODIFIED (UTC)", theme::label())),
        ]))
        .style(theme::base())
        .widths([
            Constraint::Percentage(55),
            Constraint::Percentage(15),
            Constraint::Percentage(30),
        ])
}
