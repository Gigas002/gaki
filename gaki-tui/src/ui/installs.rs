//! The Installs overlay (`X`): running installs with their progress, then
//! the backlog, and a footer with the forget confirm or the key hints.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::ListItem;

use libgaki::ops::install::InstallRow;

use crate::app::browser::installs::{InstallsView, RowKind, row_kind};
use crate::ui::depots::draw_list;
use crate::ui::theme;
use crate::ui::{empty_list_rows, selection_marker};
use crate::utils::bytes::convert;

/// Draw the overlay into `area`.
pub fn draw(frame: &mut Frame, area: Rect, view: &InstallsView) {
    let (title, items, selected) = match &view.rows {
        Err(e) => (
            "Installs".to_string(),
            empty_list_rows(format!("Couldn't read the install backlog: {e}")),
            None,
        ),
        Ok(rows) if rows.is_empty() => (
            "Installs (0)".to_string(),
            empty_list_rows("Nothing running, and the backlog is empty."),
            None,
        ),
        Ok(rows) => (
            format!("Installs ({})", rows.len()),
            rows.iter()
                .enumerate()
                .map(|(i, r)| row(r, i == view.selected))
                .collect(),
            Some(view.selected),
        ),
    };
    draw_list(frame, area, title, items, selected, footer(view));
}

/// `downloaded / total` and the percent, as far as they are known.
pub(crate) fn progress(row: &InstallRow) -> String {
    match (row.downloaded_bytes, row.total_bytes) {
        (_, 0) if row.downloaded_bytes == 0 => String::new(),
        (done, 0) => convert(done as f64),
        (done, total) => format!(
            "{} / {}  {}%",
            convert(done as f64),
            convert(total as f64),
            (done.min(total) * 100 / total)
        ),
    }
}

fn row(row: &InstallRow, selected: bool) -> ListItem<'static> {
    let style = if selected {
        theme::selection(theme::ACCENT)
    } else {
        theme::value()
    };
    let name = if row.name.is_empty() {
        format!("app {}", row.app_id)
    } else {
        row.name.clone()
    };
    let state_style = match row_kind(row) {
        RowKind::Running => theme::value().fg(theme::ONLINE),
        RowKind::Elsewhere => theme::accent(),
        RowKind::Waiting => theme::value().fg(theme::WARN),
    };
    let mut spans = vec![
        Span::styled(selection_marker(selected).to_string(), style),
        Span::styled(format!("{name:<28}"), style),
        Span::styled(format!(" {:<18}", row.state), state_style),
        Span::styled(progress(row), theme::dim()),
    ];
    if !row.status.is_empty() {
        spans.push(Span::styled(format!("  {}", row.status), theme::dim()));
    }
    ListItem::new(Line::from(spans))
}

fn dim(text: impl Into<String>) -> Line<'static> {
    Line::from(Span::styled(text.into(), theme::dim()))
}

fn footer(view: &InstallsView) -> Vec<Line<'static>> {
    if let Some(confirm) = &view.confirm {
        let question = if confirm.delete_files {
            format!(
                "Forget {} and delete its partial files? This can't be undone.",
                confirm.name
            )
        } else {
            format!("Forget {}? Its files stay on disk.", confirm.name)
        };
        return vec![
            Line::from(Span::styled(question, theme::value().fg(theme::WARN))),
            dim("[y] yes  [n] no"),
        ];
    }
    let mut lines = Vec::new();
    if !view.status.is_empty() {
        lines.push(Line::from(Span::styled(
            view.status.clone(),
            theme::value().fg(theme::WARN),
        )));
    }
    lines.push(dim(
        "[r/Enter] resume  [R] resume all  [p] pause  [s] stop  [f] forget  [F] forget + delete files  [Esc] close",
    ));
    lines
}
