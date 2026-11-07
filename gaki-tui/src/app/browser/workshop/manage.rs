//! The Workshop overlay's managing side: the status list (installed and
//! subscribed items, with update detection), installing, updating and
//! uninstalling items, and posting a comment. Everything that reaches Steam
//! runs on a thread; `poll_workshop_tasks` adopts the results.

use std::sync::{Arc, Mutex};

use super::super::Browser;
use crate::app::browser::job::{self, Load, Slot};
use crate::app::steam;

/// An install, update or uninstall running in the background. It keeps
/// running when the overlay closes; reopening shows its progress.
pub struct WorkshopJob {
    /// The game the items belong to, to reload its list when done.
    pub app_id: i32,
    /// What the job is doing right now, one line.
    pub progress: Arc<Mutex<String>>,
    /// The message to show once it's done, from the item count.
    done: fn(usize) -> String,
    result: Slot<usize>,
}

impl WorkshopJob {
    pub fn progress_line(&self) -> String {
        self.progress
            .lock()
            .map(|line| line.clone())
            .unwrap_or_default()
    }
}

/// An uninstall waiting for its y/n.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UninstallConfirm {
    pub id: u64,
    pub title: String,
}

/// The comment being written in the comments sub-pane.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommentDraft {
    pub text: String,
    /// Enter was pressed; waiting for y/n.
    pub confirming: bool,
}

/// State for the status list, the item jobs and the comment prompt.
#[derive(Default)]
pub struct WorkshopTasks {
    /// The status list while it loads (adopted into `Browser::workshop`).
    pub list: Option<Load<Vec<steam::WorkshopItem>>>,
    /// Why the list couldn't load.
    pub list_error: Option<String>,
    pub job: Option<WorkshopJob>,
    /// The last finished job's result, or why an action was refused.
    pub message: String,
    pub confirm: Option<UninstallConfirm>,
    pub comment: Option<CommentDraft>,
    /// A comment being posted.
    pub posting: Option<Load<u64>>,
}

impl WorkshopTasks {
    pub fn is_loading(&self) -> bool {
        self.list.as_ref().is_some_and(Load::is_loading)
    }

    /// What the footer shows: the running job's progress, else the last
    /// message.
    pub fn status_line(&self) -> String {
        match &self.job {
            Some(job) => job.progress_line(),
            None => self.message.clone(),
        }
    }
}

impl Browser {
    /// Show `message` in the status list's line and the browse pane's.
    fn workshop_note(&mut self, message: String) {
        self.workshop_status = message.clone();
        self.workshop_tasks.message = message;
    }

    /// Reload the status list for the overlay's game in the background.
    pub fn reload_workshop_status(&mut self) {
        let app_id = self.workshop_app_id;
        self.workshop_tasks.list_error = None;
        self.workshop_tasks.list = Some(Load::start(move || steam::workshop_status(app_id)));
    }

    /// The highlighted status-list row.
    pub fn selected_workshop_item(&self) -> Option<&steam::WorkshopItem> {
        self.workshop.get(self.workshop_selected)
    }

    /// Start installing `ids` (collections expand). Refused while another
    /// job runs.
    fn start_workshop_install(&mut self, ids: Vec<u64>, first: String) {
        if self.workshop_tasks.job.is_some() {
            self.workshop_note("Another Workshop job is still running.".to_string());
            return;
        }
        let progress = Arc::new(Mutex::new(first));
        let line = Arc::clone(&progress);
        let result = job::spawn(move || steam::workshop_install(ids, line));
        self.workshop_tasks.job = Some(WorkshopJob {
            app_id: self.workshop_app_id,
            progress,
            done: |n| match n {
                1 => "Installed.".to_string(),
                n => format!("Installed {n} items."),
            },
            result,
        });
    }

    /// Install or update the highlighted status-list row (`i`).
    pub fn workshop_install_selected(&mut self) {
        let Some((id, title)) = self
            .selected_workshop_item()
            .and_then(|item| Some((item.id?, item.display_title())))
        else {
            return;
        };
        self.start_workshop_install(vec![id], format!("Installing {title}…"));
    }

    /// Update every installed item Steam has newer content for (`U`), one
    /// after another.
    pub fn workshop_update_all(&mut self) {
        let ids: Vec<u64> = self
            .workshop
            .iter()
            .filter(|item| item.update_available)
            .filter_map(|item| item.id)
            .collect();
        if ids.is_empty() {
            self.workshop_note("Everything installed is up to date.".to_string());
            return;
        }
        let count = ids.len();
        self.start_workshop_install(ids, format!("Updating {count} item(s)…"));
    }

    /// Install the highlighted browse result (`F4`); a collection installs
    /// its items.
    pub fn workshop_install_result(&mut self) {
        let Some((id, title)) = self
            .selected_workshop_result()
            .and_then(|item| Some((item.id?, item.display_title())))
        else {
            return;
        };
        self.start_workshop_install(vec![id], format!("Installing {title}…"));
    }

    /// Ask to uninstall the highlighted row (`u`).
    pub fn begin_workshop_uninstall(&mut self) {
        let Some(item) = self.selected_workshop_item() else {
            return;
        };
        let Some(id) = item.id else {
            return;
        };
        if !item.installed {
            self.workshop_note("It isn't installed.".to_string());
            return;
        }
        let title = item.display_title();
        self.workshop_tasks.confirm = Some(UninstallConfirm { id, title });
    }

    pub fn workshop_confirm_open(&self) -> bool {
        self.workshop_tasks.confirm.is_some()
    }

    /// Answer the uninstall confirm.
    pub fn answer_workshop_uninstall(&mut self, yes: bool) {
        let Some(confirm) = self.workshop_tasks.confirm.take() else {
            return;
        };
        if !yes {
            return;
        }
        if self.workshop_tasks.job.is_some() {
            self.workshop_note("Another Workshop job is still running.".to_string());
            return;
        }
        let id = confirm.id;
        self.workshop_tasks.job = Some(WorkshopJob {
            app_id: self.workshop_app_id,
            progress: Arc::new(Mutex::new(format!("Uninstalling {}…", confirm.title))),
            done: |_| "Uninstalled.".to_string(),
            result: job::spawn(move || steam::workshop_uninstall(vec![id])),
        });
    }

    /// Open the comment prompt in the comments sub-pane (`c`).
    pub fn begin_workshop_comment(&mut self) {
        if self.workshop_tasks.posting.is_some() {
            return;
        }
        self.workshop_tasks.comment = Some(CommentDraft::default());
    }

    pub fn workshop_comment_open(&self) -> bool {
        self.workshop_tasks.comment.is_some()
    }

    pub fn workshop_comment_push(&mut self, c: char) {
        if let Some(draft) = &mut self.workshop_tasks.comment
            && !draft.confirming
        {
            draft.text.push(c);
        }
    }

    pub fn workshop_comment_pop(&mut self) {
        if let Some(draft) = &mut self.workshop_tasks.comment
            && !draft.confirming
        {
            draft.text.pop();
        }
    }

    pub fn cancel_workshop_comment(&mut self) {
        self.workshop_tasks.comment = None;
    }

    /// Enter in the prompt: ask to confirm a non-empty comment.
    pub fn submit_workshop_comment(&mut self) {
        if let Some(draft) = &mut self.workshop_tasks.comment
            && !draft.text.trim().is_empty()
        {
            draft.confirming = true;
        }
    }

    /// Answer the post confirm: `yes` posts it in the background.
    pub fn answer_workshop_comment(&mut self, yes: bool) {
        let Some(draft) = self.workshop_tasks.comment.take() else {
            return;
        };
        if !yes {
            // Back to editing, so a stray key doesn't lose the text.
            self.workshop_tasks.comment = Some(CommentDraft {
                confirming: false,
                ..draft
            });
            return;
        }
        let id = self.workshop_comments_id();
        let text = draft.text.trim().to_string();
        self.workshop_comments_status = "Posting…".to_string();
        self.workshop_tasks.posting =
            Some(Load::start(move || steam::workshop_post_comment(id, text)));
    }

    /// Adopt finished loads and jobs. Called once per event-loop iteration.
    pub fn poll_workshop_tasks(&mut self) {
        if let Some(list) = &mut self.workshop_tasks.list
            && list.poll()
        {
            let outcome = self.workshop_tasks.list.take();
            match outcome {
                Some(Load::Ready(items)) => {
                    let keep = self.workshop.get(self.workshop_selected).and_then(|i| i.id);
                    self.workshop = items;
                    self.workshop_selected = keep
                        .and_then(|id| self.workshop.iter().position(|i| i.id == Some(id)))
                        .unwrap_or(0);
                }
                Some(Load::Failed(e)) => self.workshop_tasks.list_error = Some(e),
                _ => {}
            }
        }

        let finished = self
            .workshop_tasks
            .job
            .as_ref()
            .and_then(|job| job::take(&job.result));
        if let Some(result) = finished
            && let Some(job) = self.workshop_tasks.job.take()
        {
            self.workshop_note(match result {
                Ok(n) => (job.done)(n),
                Err(e) => format!("Failed: {e}"),
            });
            if self.show_workshop && job.app_id == self.workshop_app_id {
                self.reload_workshop_status();
            }
        }

        if let Some(posting) = &mut self.workshop_tasks.posting
            && posting.poll()
        {
            let outcome = self.workshop_tasks.posting.take();
            match outcome {
                Some(Load::Failed(e)) => {
                    self.workshop_comments_status = format!("Posting failed: {e}");
                }
                _ if self.workshop_comments_open => {
                    // The reload shows the new comment.
                    let id = self.workshop_comments_id();
                    self.fetch_workshop_comments(id);
                }
                _ => {}
            }
        }
    }
}
