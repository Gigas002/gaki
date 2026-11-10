//! The Installs overlay (`X`): running installs with their progress, then
//! the backlog, and a footer with the forget confirm or the key hints. Its
//! Duplicates tab lists games installed in more than one library; its
//! Transfer tab exports the library to a file and imports one.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::ListItem;

use libgaki::ops::install::InstallRow;

use crate::app::browser::installs::transfer::{PlanLine, TransferTab, entry_label, plan_lines};
use crate::app::browser::installs::{DuplicatesTab, InstallsTab, InstallsView, RowKind, row_kind};
use crate::app::browser::job::Load;
use crate::app::steam::DuplicateRow;
use crate::keymap::{self, InstallsAction as I, OverlayAction as O, hints, key_of, pair};
use crate::ui::depots::draw_list;
use crate::ui::theme;
use crate::ui::{date_time, empty_list_rows, selection_marker};
use crate::utils::bytes::convert;

/// `Installs · [Queue] Duplicates`, the shown tab bracketed, with a count.
fn title(view: &InstallsView, count: Option<usize>) -> String {
    let mut title = String::new();
    for tab in InstallsTab::ALL {
        if !title.is_empty() {
            title.push_str(" · ");
        }
        if tab == view.tab {
            match count {
                Some(n) => title.push_str(&format!("[{} ({n})]", tab.label())),
                None => title.push_str(&format!("[{}]", tab.label())),
            }
        } else {
            title.push_str(tab.label());
        }
    }
    title
}

/// Draw the overlay into `area`.
pub fn draw(frame: &mut Frame, area: Rect, view: &InstallsView) {
    match view.tab {
        InstallsTab::Duplicates => return draw_duplicates(frame, area, view),
        InstallsTab::Transfer => return draw_transfer(frame, area, view),
        InstallsTab::Queue => {}
    }
    let (title, items, selected) = match &view.rows {
        Err(e) => (
            title(view, None),
            empty_list_rows(format!("Couldn't read the install backlog: {e}")),
            None,
        ),
        Ok(rows) if rows.is_empty() => (
            title(view, Some(0)),
            empty_list_rows("Nothing running, and the backlog is empty."),
            None,
        ),
        Ok(rows) => (
            title(view, Some(rows.len())),
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
    lines.push(dim(hints(&[
        (keymap::get().label(I::Resume).replace(" / ", "/"), "resume"),
        (key_of(I::ResumeAll), "resume all"),
        (key_of(I::Pause), "pause"),
        (key_of(I::Stop), "stop"),
        (key_of(I::Forget), "forget"),
        (key_of(I::ForgetAndDelete), "forget + delete files"),
        (key_of(O::NextTab), "next tab"),
        ("Esc".to_string(), "close"),
    ])));
    lines
}

fn duplicate_row(row: &DuplicateRow, selected: bool) -> ListItem<'static> {
    let style = if selected {
        theme::selection(theme::ACCENT)
    } else {
        theme::value()
    };
    let mut lines = vec![Line::from(vec![
        Span::styled(selection_marker(selected).to_string(), style),
        Span::styled(row.name.clone(), style),
        Span::styled(format!("  {}", row.app_id), theme::dim()),
        Span::styled(
            format!("  {} reclaimable", convert(row.reclaimable() as f64)),
            theme::accent(),
        ),
    ])];
    lines.push(Line::from(vec![
        Span::styled("    keep   ".to_string(), theme::value().fg(theme::ONLINE)),
        Span::styled(row.keep.display().to_string(), theme::value()),
        Span::styled(
            format!("  updated {}", date_time(row.keep_updated)),
            theme::dim(),
        ),
    ]));
    for stale in &row.stale {
        lines.push(Line::from(vec![
            Span::styled("    stale  ".to_string(), theme::value().fg(theme::WARN)),
            Span::styled(stale.path.display().to_string(), theme::value()),
            Span::styled(
                format!(
                    "  {}  updated {}",
                    convert(stale.size as f64),
                    date_time(stale.last_updated)
                ),
                theme::dim(),
            ),
        ]));
    }
    ListItem::new(lines)
}

fn draw_duplicates(frame: &mut Frame, area: Rect, view: &InstallsView) {
    let tab: Option<&DuplicatesTab> = view.duplicates.as_ref();
    let hint = dim(hints(&[
        (pair(O::Down, O::Up), "move"),
        (key_of(I::Rescan), "scan again"),
        (key_of(O::NextTab), "next tab"),
        ("Esc".to_string(), "close"),
    ]));
    let (count, items, selected, mut footer) = match tab.map(|t| &t.scan) {
        None | Some(Load::Loading(_)) => (
            None,
            empty_list_rows("Scanning the Steam libraries…"),
            None,
            Vec::new(),
        ),
        Some(Load::Failed(e)) => (
            None,
            empty_list_rows(format!("Couldn't scan the Steam libraries: {e}")),
            None,
            Vec::new(),
        ),
        Some(Load::Ready(rows)) if rows.is_empty() => (
            Some(0),
            empty_list_rows("No game is installed in more than one Steam library."),
            None,
            Vec::new(),
        ),
        Some(Load::Ready(rows)) => {
            let selected = tab.map_or(0, |t| t.selected);
            let total: u64 = rows.iter().map(DuplicateRow::reclaimable).sum();
            (
                Some(rows.len()),
                rows.iter()
                    .enumerate()
                    .map(|(i, r)| duplicate_row(r, i == selected))
                    .collect(),
                Some(selected),
                vec![
                    Line::from(Span::styled(
                        format!(
                            "{} reclaimable. Two copies leave two manifests disagreeing, \
                             so the game can report an update on every check.",
                            convert(total as f64)
                        ),
                        theme::value().fg(theme::WARN),
                    )),
                    dim("gaki never deletes a copy: remove the stale folder yourself."),
                ],
            )
        }
    };
    footer.push(hint);
    draw_list(frame, area, title(view, count), items, selected, footer);
}

fn plan_row(line: PlanLine<'_>, selected: bool) -> ListItem<'static> {
    let style = if selected {
        theme::selection(theme::ACCENT)
    } else {
        theme::value()
    };
    let (what, what_style, entry) = match line {
        PlanLine::Install(e) => ("install", theme::value().fg(theme::ONLINE), e),
        PlanLine::Installed(e) => ("skip: installed", theme::dim(), e),
        PlanLine::NotOwned(e) => ("skip: not owned", theme::value().fg(theme::WARN), e),
    };
    let mut spans = vec![
        Span::styled(selection_marker(selected).to_string(), style),
        Span::styled(format!("{:<17}", what), what_style),
        Span::styled(entry_label(entry), style),
        Span::styled(format!("  {}", entry.app_id), theme::dim()),
    ];
    if let PlanLine::Install(e) = line
        && let Some(branch) = &e.branch
    {
        spans.push(Span::styled(
            format!("  was on '{branch}'; installs take the default branch"),
            theme::dim(),
        ));
    }
    ListItem::new(Line::from(spans))
}

fn transfer_footer(tab: &TransferTab) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let caret = if tab.editing { "▏" } else { "" };
    lines.push(Line::from(vec![
        Span::styled("File: ".to_string(), theme::label()),
        Span::styled(format!("{}{caret}", tab.path), theme::value()),
    ]));
    let scope = if tab.all {
        "every game in the library"
    } else {
        "installed games only"
    };
    lines.push(Line::from(vec![
        Span::styled("Export: ".to_string(), theme::label()),
        Span::styled(scope.to_string(), theme::value()),
    ]));
    if !tab.status.is_empty() {
        lines.push(Line::from(Span::styled(
            tab.status.clone(),
            theme::value().fg(theme::WARN),
        )));
    }
    if tab.editing {
        lines.push(dim("Type the path  [Enter] done"));
    } else if let Some(plan) = tab.ready_plan().filter(|_| tab.confirm_open()) {
        let n = plan.install.len();
        let games = if n == 1 { "game" } else { "games" };
        lines.push(Line::from(Span::styled(
            format!("Install {n} {games} into the default library, one after another?"),
            theme::value().fg(theme::WARN),
        )));
        lines.push(dim("[y] yes  [n] no"));
    } else {
        lines.push(dim(hints(&[
            (key_of(I::EditPath), "edit path"),
            (key_of(I::ExportAll), "installed / all"),
            (key_of(I::Export), "export"),
            (key_of(I::Import), "import"),
            (key_of(O::NextTab), "next tab"),
            ("Esc".to_string(), "close"),
        ])));
    }
    lines
}

fn draw_transfer(frame: &mut Frame, area: Rect, view: &InstallsView) {
    let tab = &view.transfer;
    let (count, items, selected) = match &tab.plan {
        None => (
            None,
            empty_list_rows(
                "Export writes your library to a file; import reads one on another machine \
                 and installs the games that are missing.",
            ),
            None,
        ),
        Some(Load::Loading(_)) => (
            None,
            empty_list_rows("Reading the file and your library…"),
            None,
        ),
        Some(Load::Failed(e)) => (None, empty_list_rows(format!("Import failed: {e}")), None),
        Some(Load::Ready(plan)) => {
            let lines = plan_lines(plan);
            if lines.is_empty() {
                (Some(0), empty_list_rows("The file lists no games."), None)
            } else {
                (
                    Some(lines.len()),
                    lines
                        .iter()
                        .enumerate()
                        .map(|(i, &l)| plan_row(l, i == tab.selected))
                        .collect(),
                    Some(tab.selected),
                )
            }
        }
    };
    draw_list(
        frame,
        area,
        title(view, count),
        items,
        selected,
        transfer_footer(tab),
    );
}
