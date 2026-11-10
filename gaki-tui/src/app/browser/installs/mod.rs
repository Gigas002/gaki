//! The Installs overlay (`X`): installs running in this TUI, then the
//! install backlog, which `gaki-cli` shares. Resume one or all (in turn),
//! pause, stop, and forget (optionally deleting the partial files). An
//! install another gaki process runs shows as "running elsewhere" and is
//! left to that process.
//!
//! A second tab (`Tab`) lists games installed in more than one Steam
//! library (`gaki-cli duplicates`): the copy kept, the stale ones with their
//! sizes, and the space deleting them would free. It is read-only; which
//! copy to delete is left to the user.
//!
//! A third tab, Transfer, exports the library to a file and imports one
//! (`gaki-cli library export`/`import`); see [`transfer`].

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use libgaki::ops::install::{InstallRow, RUNNING_ELSEWHERE, Signalled};

use super::Browser;
use crate::app::browser::job::Load;
use crate::app::client::Client;
use crate::app::steam;
use crate::utils::error::TuiError;

pub mod transfer;
pub use transfer::TransferTab;

/// How often the open overlay re-reads the list (local only).
const REFRESH: Duration = Duration::from_secs(1);

/// What a row is, for the keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    /// Running in this process.
    Running,
    /// Running in another gaki process.
    Elsewhere,
    /// A backlog entry (paused, interrupted, aborted, queued).
    Waiting,
}

pub fn row_kind(row: &InstallRow) -> RowKind {
    match row.state {
        "running" => RowKind::Running,
        RUNNING_ELSEWHERE => RowKind::Elsewhere,
        _ => RowKind::Waiting,
    }
}

/// The overlay's tabs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum InstallsTab {
    #[default]
    Queue,
    Duplicates,
    Transfer,
}

impl InstallsTab {
    pub const ALL: [InstallsTab; 3] = [
        InstallsTab::Queue,
        InstallsTab::Duplicates,
        InstallsTab::Transfer,
    ];

    pub fn label(self) -> &'static str {
        match self {
            InstallsTab::Queue => "Installs",
            InstallsTab::Duplicates => "Duplicates",
            InstallsTab::Transfer => "Transfer",
        }
    }

    /// The tab after this one (`forward`) or before it, wrapping.
    pub fn step(self, forward: bool) -> InstallsTab {
        let n = Self::ALL.len();
        let i = Self::ALL.iter().position(|&t| t == self).unwrap_or(0);
        Self::ALL[if forward {
            (i + 1) % n
        } else {
            (i + n - 1) % n
        }]
    }
}

/// The Duplicates tab: the scan runs on a thread the first time it's shown.
pub struct DuplicatesTab {
    pub scan: Load<Vec<steam::DuplicateRow>>,
    pub selected: usize,
}

impl DuplicatesTab {
    /// A tab over `rows`, without scanning (tests, previews).
    pub fn with(rows: Vec<steam::DuplicateRow>) -> Self {
        Self {
            scan: Load::Ready(rows),
            selected: 0,
        }
    }

    fn start() -> Self {
        Self {
            scan: Load::start(steam::duplicates),
            selected: 0,
        }
    }

    pub fn rows(&self) -> &[steam::DuplicateRow] {
        self.scan.ready().map(Vec::as_slice).unwrap_or_default()
    }
}

/// A forget waiting for its y/n.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgetConfirm {
    pub app_id: u32,
    pub name: String,
    pub delete_files: bool,
}

/// The Installs overlay's state.
pub struct InstallsView {
    pub rows: Result<Vec<InstallRow>, String>,
    pub selected: usize,
    pub confirm: Option<ForgetConfirm>,
    /// A one-line message (an action's result).
    pub status: String,
    pub tab: InstallsTab,
    pub duplicates: Option<DuplicatesTab>,
    pub transfer: TransferTab,
    refreshed: Instant,
}

impl InstallsView {
    /// A view over `rows`, without reading anything (tests, previews).
    pub fn with_rows(rows: Vec<InstallRow>) -> Self {
        Self {
            rows: Ok(rows),
            selected: 0,
            confirm: None,
            status: String::new(),
            tab: InstallsTab::Queue,
            duplicates: None,
            transfer: TransferTab::default(),
            refreshed: Instant::now(),
        }
    }

    fn load() -> Self {
        let mut view = Self::with_rows(Vec::new());
        view.refresh();
        view
    }

    /// Re-read the list, keeping the highlight on the same app if it's
    /// still there.
    pub fn refresh(&mut self) {
        let keep = self.selected_row().map(|r| r.app_id);
        self.set_rows(steam::install_rows().map_err(|e| e.to_string()));
        self.refreshed = Instant::now();
        if let Some(app_id) = keep
            && let Ok(rows) = &self.rows
            && let Some(i) = rows.iter().position(|r| r.app_id == app_id)
        {
            self.selected = i;
        }
    }

    pub(crate) fn set_rows(&mut self, rows: Result<Vec<InstallRow>, String>) {
        self.rows = rows;
        let len = self.rows.as_ref().map_or(0, Vec::len);
        self.selected = self.selected.min(len.saturating_sub(1));
    }

    pub fn selected_row(&self) -> Option<&InstallRow> {
        self.rows.as_ref().ok()?.get(self.selected)
    }

    fn row_name(row: &InstallRow) -> String {
        if row.name.is_empty() {
            format!("app {}", row.app_id)
        } else {
            row.name.clone()
        }
    }
}

const ELSEWHERE_HINT: &str = "Another gaki process is running it; stop it there.";

impl Browser {
    pub fn open_installs(&mut self) {
        self.installs = Some(InstallsView::load());
    }

    pub fn close_installs(&mut self) {
        self.installs = None;
    }

    /// Switch to the next tab (`forward`) or the previous one. The
    /// duplicates scan runs the first time Duplicates is shown; `r` there
    /// rescans.
    pub fn installs_tab(&mut self, forward: bool) {
        if let Some(view) = &mut self.installs {
            view.tab = view.tab.step(forward);
            if view.tab == InstallsTab::Duplicates && view.duplicates.is_none() {
                view.duplicates = Some(DuplicatesTab::start());
            }
        }
    }

    /// `r` in the Duplicates tab: scan again.
    pub fn rescan_duplicates(&mut self) {
        if let Some(view) = &mut self.installs
            && !view
                .duplicates
                .as_ref()
                .is_some_and(|d| d.scan.is_loading())
        {
            view.duplicates = Some(DuplicatesTab::start());
        }
    }

    pub fn installs_move(&mut self, delta: isize) {
        if self
            .installs
            .as_ref()
            .is_some_and(|v| v.tab == InstallsTab::Transfer)
        {
            self.transfer_move(delta);
            return;
        }
        if let Some(view) = &mut self.installs
            && view.tab == InstallsTab::Duplicates
        {
            if let Some(tab) = &mut view.duplicates {
                let len = tab.rows().len();
                tab.selected = tab
                    .selected
                    .saturating_add_signed(delta)
                    .min(len.saturating_sub(1));
            }
            return;
        }
        if let Some(view) = &mut self.installs {
            let len = view.rows.as_ref().map_or(0, Vec::len);
            view.selected = view
                .selected
                .saturating_add_signed(delta)
                .min(len.saturating_sub(1));
        }
    }

    /// Resume the highlighted entry (`Enter`/`r`), or every waiting one in
    /// turn (`R`), each into the library and platform it was started with.
    pub fn installs_resume(&mut self, client: &Client, all: bool) -> Result<(), TuiError> {
        let Some(view) = &self.installs else {
            return Ok(());
        };
        let targets: Vec<(u32, String, Option<String>)> = match &view.rows {
            Ok(rows) if all => rows
                .iter()
                .filter(|r| row_kind(r) == RowKind::Waiting)
                .map(|r| {
                    (
                        r.app_id,
                        InstallsView::row_name(r),
                        r.entry.as_ref().map(|e| e.library.clone()),
                    )
                })
                .collect(),
            _ => {
                let Some(row) = view.selected_row() else {
                    return Ok(());
                };
                let message = match row_kind(row) {
                    RowKind::Running => Some("It is already running."),
                    RowKind::Elsewhere => Some(ELSEWHERE_HINT),
                    RowKind::Waiting => None,
                };
                if let Some(message) = message {
                    self.set_installs_status(message.to_string());
                    return Ok(());
                }
                vec![(
                    row.app_id,
                    InstallsView::row_name(row),
                    row.entry.as_ref().map(|e| e.library.clone()),
                )]
            }
        };
        if targets.is_empty() {
            self.set_installs_status("Nothing to resume.".to_string());
            return Ok(());
        }
        let mut jobs = Vec::with_capacity(targets.len());
        for (app_id, _, library) in &targets {
            let id = *app_id as i32;
            let status = self
                .items
                .iter()
                .find(|g| g.id == id)
                .map(|g| g.status_counter())
                .unwrap_or_else(|| Arc::new(Mutex::new(None)));
            let library = library.clone().filter(|l| !l.is_empty());
            let control = self.begin_install(id, library);
            jobs.push(steam::ResumeJob {
                app_id: id,
                status,
                control,
            });
        }
        client.resume(jobs)?;
        let message = match targets.as_slice() {
            [(_, name, _)] => format!("Resuming {name}…"),
            many => format!("Resuming {} installs, one after another…", many.len()),
        };
        self.set_installs_status(message);
        Ok(())
    }

    /// Pause (`p`) or stop (`s`) the highlighted install. One this TUI
    /// tracks goes through its control (so the library badge follows); a
    /// backlog entry is relabelled; another process's is refused.
    pub fn installs_signal(&mut self, pause: bool) {
        let Some(row) = self.installs.as_ref().and_then(InstallsView::selected_row) else {
            return;
        };
        let (app_id, name, kind) = (row.app_id, InstallsView::row_name(row), row_kind(row));
        let id = app_id as i32;
        if kind == RowKind::Running && self.install_controls.contains_key(&id) {
            if pause {
                self.pause_install(id);
            } else {
                self.stop_install(id);
            }
            self.set_installs_status(format!(
                "{} {name}…",
                if pause { "Pausing" } else { "Stopping" }
            ));
            return;
        }
        let message = match steam::install_signal(app_id, pause) {
            Ok(Signalled::Running) => {
                format!("{} {name}…", if pause { "Pausing" } else { "Stopping" })
            }
            Ok(Signalled::Elsewhere) => ELSEWHERE_HINT.to_string(),
            Ok(Signalled::Relabelled(state)) => {
                format!("{name} is not downloading; marked {}.", state.as_str())
            }
            Ok(Signalled::NotFound) => format!("{name} is no longer in the list."),
            Err(e) => e.to_string(),
        };
        self.set_installs_status(message);
        self.refresh_installs();
    }

    /// Ask to forget the highlighted entry (`f`), or to forget it and delete
    /// its partial files (`F`). Running installs are refused.
    pub fn begin_installs_forget(&mut self, delete_files: bool) {
        let Some(view) = &mut self.installs else {
            return;
        };
        let Some(row) = view.selected_row() else {
            return;
        };
        match row_kind(row) {
            RowKind::Running => view.status = "Stop it first.".to_string(),
            RowKind::Elsewhere => view.status = ELSEWHERE_HINT.to_string(),
            RowKind::Waiting => {
                view.confirm = Some(ForgetConfirm {
                    app_id: row.app_id,
                    name: InstallsView::row_name(row),
                    delete_files,
                });
            }
        }
    }

    pub fn installs_confirm_open(&self) -> bool {
        self.installs.as_ref().is_some_and(|v| v.confirm.is_some())
    }

    /// Answer the forget confirm.
    pub fn answer_installs_confirm(&mut self, yes: bool) {
        let Some(confirm) = self.installs.as_mut().and_then(|v| v.confirm.take()) else {
            return;
        };
        if !yes {
            return;
        }
        let message = match steam::install_forget(confirm.app_id, confirm.delete_files) {
            Ok(Some(dir)) => format!("Forgot {} and deleted {}.", confirm.name, dir.display()),
            Ok(None) => format!("Forgot {}.", confirm.name),
            Err(e) => e.to_string(),
        };
        self.set_installs_status(message);
        self.refresh_installs();
    }

    fn set_installs_status(&mut self, message: String) {
        if let Some(view) = &mut self.installs {
            view.status = message;
        }
    }

    fn refresh_installs(&mut self) {
        if let Some(view) = &mut self.installs {
            view.refresh();
        }
    }

    /// Re-read the list once a second while the overlay is open, for live
    /// progress and for installs other processes start or finish.
    pub fn poll_installs(&mut self) {
        if let Some(view) = &mut self.installs {
            if let Some(tab) = &mut view.duplicates {
                tab.scan.poll();
            }
            view.transfer.poll();
            if view.refreshed.elapsed() >= REFRESH {
                view.refresh();
            }
        }
    }
}

#[cfg(test)]
mod tests;
