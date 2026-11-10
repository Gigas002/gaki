//! The Installs overlay's Transfer tab: `library export` and `library
//! import` from `gaki-cli`. Export writes the installed games (or every game)
//! to a file; import reads one, shows what it would install and skip, and on
//! `y` queues the missing games in the backlog and starts them one after
//! another, like Resume all.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use libgaki::library::transfer::{ImportPlan, LibraryEntry};

use super::InstallsTab;
use crate::app::browser::Browser;
use crate::app::browser::job::Load;
use crate::app::client::Client;
use crate::app::steam;
use crate::utils::error::TuiError;
use crate::utils::xdg::expand_dir;

/// The file the tab starts with.
pub const DEFAULT_PATH: &str = "~/gaki-library.toml";

/// The Transfer tab's state.
pub struct TransferTab {
    /// The library file, as typed (`~` allowed).
    pub path: String,
    /// The path is being typed.
    pub editing: bool,
    /// Export every game, not only the installed ones.
    pub all: bool,
    /// A running export: how many games it wrote.
    pub export: Option<Load<usize>>,
    /// A read import file, sorted against the library; waits for `y` once
    /// it is ready and has games to install.
    pub plan: Option<Load<ImportPlan>>,
    /// The highlighted line of the plan.
    pub selected: usize,
    /// A one-line message (an action's result).
    pub status: String,
}

impl Default for TransferTab {
    fn default() -> Self {
        Self {
            path: DEFAULT_PATH.to_string(),
            editing: false,
            all: false,
            export: None,
            plan: None,
            selected: 0,
            status: String::new(),
        }
    }
}

/// One line of an import plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanLine<'a> {
    Install(&'a LibraryEntry),
    Installed(&'a LibraryEntry),
    NotOwned(&'a LibraryEntry),
}

/// A plan's lines: what gets installed, then what is skipped.
pub fn plan_lines(plan: &ImportPlan) -> Vec<PlanLine<'_>> {
    plan.install
        .iter()
        .map(PlanLine::Install)
        .chain(plan.installed.iter().map(PlanLine::Installed))
        .chain(plan.not_owned.iter().map(PlanLine::NotOwned))
        .collect()
}

/// An entry's name for messages.
pub fn entry_label(entry: &LibraryEntry) -> String {
    if entry.name.is_empty() {
        format!("app {}", entry.app_id)
    } else {
        entry.name.clone()
    }
}

impl TransferTab {
    /// The typed path with `~` expanded.
    fn file(&self) -> PathBuf {
        let home = libgaki::core::config::home_dir().unwrap_or_else(|_| PathBuf::from("/"));
        expand_dir(&self.path, &home)
    }

    /// An export or an import read is running.
    pub fn busy(&self) -> bool {
        self.export.as_ref().is_some_and(Load::is_loading)
            || self.plan.as_ref().is_some_and(Load::is_loading)
    }

    /// The plan, once it is read.
    pub fn ready_plan(&self) -> Option<&ImportPlan> {
        self.plan.as_ref().and_then(Load::ready)
    }

    /// A read plan with games to install, waiting for `y`/`n`.
    pub fn confirm_open(&self) -> bool {
        self.ready_plan().is_some_and(|p| !p.install.is_empty())
    }

    /// Adopt finished jobs. Called once per tick.
    pub fn poll(&mut self) {
        if let Some(export) = &mut self.export
            && export.poll()
        {
            match export.ready() {
                Some(&n) => {
                    let games = if n == 1 { "game" } else { "games" };
                    self.status = format!("Wrote {n} {games} to {}.", self.file().display());
                }
                None => {
                    if let Load::Failed(e) = export {
                        self.status = format!("Export failed: {e}");
                    }
                }
            }
            self.export = None;
        }
        if let Some(plan) = &mut self.plan
            && plan.poll()
        {
            self.selected = 0;
            match plan {
                Load::Ready(p) if p.install.is_empty() => {
                    self.status = "Nothing to install: every game in the file is installed \
                                   or not in this account's library."
                        .to_string();
                }
                Load::Ready(_) => self.status.clear(),
                Load::Failed(e) => {
                    self.status = format!("Import failed: {e}");
                    self.plan = None;
                }
                Load::Loading(_) => {}
            }
        }
    }
}

impl Browser {
    fn transfer_mut(&mut self) -> Option<&mut TransferTab> {
        self.installs
            .as_mut()
            .filter(|v| v.tab == InstallsTab::Transfer)
            .map(|v| &mut v.transfer)
    }

    /// The Transfer tab is typing its path.
    pub fn transfer_editing(&self) -> bool {
        self.installs
            .as_ref()
            .is_some_and(|v| v.tab == InstallsTab::Transfer && v.transfer.editing)
    }

    /// The Transfer tab waits for `y`/`n` on an import.
    pub fn transfer_confirm_open(&self) -> bool {
        self.installs
            .as_ref()
            .is_some_and(|v| v.tab == InstallsTab::Transfer && v.transfer.confirm_open())
    }

    /// `e`: type the file's path.
    pub fn transfer_edit_path(&mut self) {
        if let Some(tab) = self.transfer_mut()
            && !tab.busy()
        {
            tab.editing = true;
        }
    }

    pub fn transfer_path_push(&mut self, c: char) {
        if let Some(tab) = self.transfer_mut() {
            tab.path.push(c);
        }
    }

    pub fn transfer_path_pop(&mut self) {
        if let Some(tab) = self.transfer_mut() {
            tab.path.pop();
        }
    }

    /// `Enter` or `Esc` while typing: done.
    pub fn transfer_path_done(&mut self) {
        if let Some(tab) = self.transfer_mut() {
            tab.editing = false;
            if tab.path.trim().is_empty() {
                tab.path = DEFAULT_PATH.to_string();
            }
        }
    }

    /// `a`: export every game, or only the installed ones.
    pub fn transfer_toggle_all(&mut self) {
        if let Some(tab) = self.transfer_mut() {
            tab.all = !tab.all;
        }
    }

    /// `x`: write the library file in the background.
    pub fn transfer_export(&mut self) {
        self.transfer_start_export(steam::export_library);
    }

    pub(crate) fn transfer_start_export(
        &mut self,
        job: impl FnOnce(PathBuf, bool) -> Result<usize, TuiError> + Send + 'static,
    ) {
        let Some(tab) = self.transfer_mut() else {
            return;
        };
        if tab.busy() {
            return;
        }
        let (file, all) = (tab.file(), tab.all);
        tab.plan = None;
        tab.status = format!("Writing {}…", file.display());
        tab.export = Some(Load::start(move || job(file, all)));
    }

    /// `i`: read the library file and show what importing it would do.
    pub fn transfer_import(&mut self) {
        self.transfer_start_import(steam::plan_library_import);
    }

    pub(crate) fn transfer_start_import(
        &mut self,
        job: impl FnOnce(PathBuf) -> Result<ImportPlan, TuiError> + Send + 'static,
    ) {
        let Some(tab) = self.transfer_mut() else {
            return;
        };
        if tab.busy() {
            return;
        }
        let file = tab.file();
        tab.status = format!("Reading {}…", file.display());
        tab.plan = Some(Load::start(move || job(file)));
    }

    /// `j`/`k` in the plan.
    pub fn transfer_move(&mut self, delta: isize) {
        if let Some(tab) = self.transfer_mut() {
            let len = tab.ready_plan().map_or(0, |p| plan_lines(p).len());
            tab.selected = tab
                .selected
                .saturating_add_signed(delta)
                .min(len.saturating_sub(1));
        }
    }

    /// `y`/`n` on a read plan. Yes queues the games to install in the
    /// backlog and starts them one after another; the Installs tab shows
    /// them.
    pub fn answer_transfer_confirm(&mut self, client: &Client, yes: bool) -> Result<(), TuiError> {
        let Some(tab) = self.transfer_mut() else {
            return Ok(());
        };
        let Some(plan) = tab.plan.take().and_then(|p| match p {
            Load::Ready(plan) => Some(plan),
            _ => None,
        }) else {
            return Ok(());
        };
        tab.selected = 0;
        if !yes {
            tab.status = "Import cancelled.".to_string();
            return Ok(());
        }
        let branched: Vec<String> = plan
            .on_a_branch()
            .map(|e| {
                format!(
                    "{} ({})",
                    entry_label(e),
                    e.branch.as_deref().unwrap_or_default()
                )
            })
            .collect();
        let names: Vec<(u32, String)> = plan
            .install
            .iter()
            .map(|e| (e.app_id, entry_label(e)))
            .collect();
        let queued = match steam::queue_library_import(plan.install) {
            Ok(ids) => ids,
            Err(e) => {
                if let Some(tab) = self.transfer_mut() {
                    tab.status = format!("Couldn't queue the import: {e}");
                }
                return Ok(());
            }
        };

        let mut jobs = Vec::with_capacity(queued.len());
        for app_id in &queued {
            let id = *app_id as i32;
            let status = self
                .items
                .iter()
                .find(|g| g.id == id)
                .map(|g| g.status_counter())
                .unwrap_or_else(|| Arc::new(Mutex::new(None)));
            let control = self.begin_install(id, None);
            jobs.push(steam::ResumeJob {
                app_id: id,
                status,
                control,
            });
        }
        client.resume(jobs)?;

        let mut message = match names.as_slice() {
            [(_, name)] => format!("Installing {name}…"),
            many => format!("Installing {} games, one after another…", many.len()),
        };
        if !branched.is_empty() {
            message.push_str(&format!(
                " Installs take the default branch; switch afterwards: {}.",
                branched.join(", ")
            ));
        }
        if let Some(view) = &mut self.installs {
            view.tab = InstallsTab::Queue;
            view.refresh();
            view.status = message;
        }
        Ok(())
    }
}
