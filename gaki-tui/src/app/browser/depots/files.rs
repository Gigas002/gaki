//! The depot file view: one depot's manifest entries, opened from the
//! depots overlay with Enter.

use std::path::{Path, PathBuf};

use libgaki::ops::depot::{DepotFiles, EntryKind, ManifestFileEntry, glob_match};

use super::{Load, spawn, take};
use crate::app::browser::Browser;
use crate::app::steam;

/// What the file view's one-line prompt is editing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilesInput {
    /// The filter, applied as it is typed.
    Filter,
    /// A branch to load instead.
    Branch(String),
    /// The directory to save the highlighted file into.
    SaveTo(String),
}

/// The file view's state.
pub struct FilesView {
    pub depot_id: u32,
    pub depot_name: String,
    /// Branch whose current manifest is shown.
    pub branch: String,
    pub listing: Load<DepotFiles>,
    /// Filter text: a glob when it has `*` or `?`, else a substring
    /// (case-insensitive either way).
    pub filter: String,
    /// Highlighted row, an index into [`FilesView::visible`].
    pub selected: usize,
    pub input: Option<FilesInput>,
    /// A one-line message (a refused download, say).
    pub status: String,
}

/// Whether `path` passes `filter` (see [`FilesView::filter`]).
pub(crate) fn passes(filter: &str, path: &str) -> bool {
    if filter.is_empty() {
        true
    } else if filter.contains(['*', '?']) {
        glob_match(filter, path)
    } else {
        path.to_lowercase().contains(&filter.to_lowercase())
    }
}

impl FilesView {
    fn new(depot_id: u32, depot_name: String, listing: Load<DepotFiles>) -> Self {
        Self {
            depot_id,
            depot_name,
            branch: "public".to_string(),
            listing,
            filter: String::new(),
            selected: 0,
            input: None,
            status: String::new(),
        }
    }

    /// The entries that pass the filter, in path order.
    pub fn visible(&self) -> Vec<&ManifestFileEntry> {
        self.listing.ready().map_or_else(Vec::new, |listing| {
            listing
                .files
                .iter()
                .filter(|f| passes(&self.filter, &f.path))
                .collect()
        })
    }

    /// The highlighted entry.
    pub fn selected_entry(&self) -> Option<&ManifestFileEntry> {
        self.visible().get(self.selected).copied()
    }

    fn clamp(&mut self) {
        let max = self.visible().len().saturating_sub(1);
        self.selected = self.selected.min(max);
    }
}

/// Where downloads go when nothing else was picked this session:
/// `XDG_DOWNLOAD_DIR` from `user-dirs.dirs`, else `~/Downloads`.
pub(crate) fn default_save_dir() -> String {
    let home = libgaki::core::config::home_dir().unwrap_or_else(|_| PathBuf::from("/"));
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));
    std::fs::read_to_string(config.join("user-dirs.dirs"))
        .ok()
        .and_then(|text| xdg_download_dir(&text, &home))
        .unwrap_or_else(|| home.join("Downloads"))
        .display()
        .to_string()
}

/// `XDG_DOWNLOAD_DIR` from the text of a `user-dirs.dirs` file, with
/// `$HOME` expanded. Only `$HOME/...` and absolute paths are valid there.
pub(crate) fn xdg_download_dir(text: &str, home: &Path) -> Option<PathBuf> {
    let value = text.lines().find_map(|line| {
        line.trim()
            .strip_prefix("XDG_DOWNLOAD_DIR=")
            .map(|v| v.trim().trim_matches('"').to_string())
    })?;
    if let Some(rest) = value.strip_prefix("$HOME") {
        Some(home.join(rest.trim_start_matches('/')))
    } else if value.starts_with('/') {
        Some(PathBuf::from(value))
    } else {
        None
    }
}

/// A typed directory with `~` expanded.
pub(crate) fn expand_dir(input: &str, home: &Path) -> PathBuf {
    let input = input.trim();
    if input == "~" {
        home.to_path_buf()
    } else if let Some(rest) = input.strip_prefix("~/") {
        home.join(rest)
    } else {
        PathBuf::from(input)
    }
}

impl Browser {
    /// Open the file view for the highlighted depot; its public manifest
    /// loads in the background.
    pub fn open_depot_files(&mut self) {
        let Some(depot) = self.depots.selected_depot() else {
            return;
        };
        let Ok(depot_id) = u32::try_from(depot.id) else {
            return;
        };
        let name = depot.name.clone();
        let listing = load_listing(self.depots.app_id, depot_id, "public");
        self.depots.files = Some(FilesView::new(depot_id, name, listing));
    }

    /// Back from the file view to the depot list.
    pub fn close_depot_files(&mut self) {
        self.depots.files = None;
    }

    /// Move the highlight by `delta` rows (clamped).
    pub fn depot_files_move(&mut self, delta: isize) {
        if let Some(view) = &mut self.depots.files {
            let max = view.visible().len().saturating_sub(1);
            view.selected = view.selected.saturating_add_signed(delta).min(max);
        }
    }

    /// Start editing the filter (`/`).
    pub fn begin_depot_filter(&mut self) {
        if let Some(view) = &mut self.depots.files {
            view.input = Some(FilesInput::Filter);
            view.status.clear();
        }
    }

    /// Start typing a branch to load (`b`), prefilled with the current one.
    pub fn begin_depot_branch(&mut self) {
        if let Some(view) = &mut self.depots.files {
            view.input = Some(FilesInput::Branch(view.branch.clone()));
            view.status.clear();
        }
    }

    /// Start a download of the highlighted file (`d`): ask where, prefilled
    /// with this session's last directory or the Downloads folder. Only
    /// regular files download, one at a time.
    pub fn begin_depot_download(&mut self) {
        let busy = self.depots.download.is_some();
        let last = self.depots.save_dir.clone();
        let Some(view) = &mut self.depots.files else {
            return;
        };
        let Some(entry) = view.selected_entry() else {
            return;
        };
        view.status = if busy {
            "A download is already running.".to_string()
        } else if entry.kind != EntryKind::File {
            "Only files can be downloaded.".to_string()
        } else {
            view.input = Some(FilesInput::SaveTo(last.unwrap_or_else(default_save_dir)));
            String::new()
        };
    }

    /// Type a character into the open prompt.
    pub fn depot_input_push(&mut self, c: char) {
        let Some(view) = &mut self.depots.files else {
            return;
        };
        match &mut view.input {
            Some(FilesInput::Filter) => {
                view.filter.push(c);
                view.selected = 0;
            }
            Some(FilesInput::Branch(text) | FilesInput::SaveTo(text)) => text.push(c),
            None => {}
        }
    }

    /// Delete the last character of the open prompt.
    pub fn depot_input_pop(&mut self) {
        let Some(view) = &mut self.depots.files else {
            return;
        };
        match &mut view.input {
            Some(FilesInput::Filter) => {
                view.filter.pop();
                view.selected = 0;
            }
            Some(FilesInput::Branch(text) | FilesInput::SaveTo(text)) => {
                text.pop();
            }
            None => {}
        }
    }

    /// Esc in a prompt: leave it. Esc in the filter also clears it.
    pub fn cancel_depot_input(&mut self) {
        if let Some(view) = &mut self.depots.files {
            if view.input == Some(FilesInput::Filter) {
                view.filter.clear();
            }
            view.input = None;
            view.clamp();
        }
    }

    /// Enter in a prompt: keep the filter, load the branch, or start the
    /// download.
    pub fn submit_depot_input(&mut self) {
        let app_id = self.depots.app_id;
        let Some(view) = &mut self.depots.files else {
            return;
        };
        match view.input.take() {
            Some(FilesInput::Filter) | None => view.clamp(),
            Some(FilesInput::Branch(branch)) => {
                let branch = branch.trim();
                if !branch.is_empty() && branch != view.branch {
                    view.branch = branch.to_string();
                    view.selected = 0;
                    view.listing = load_listing(app_id, view.depot_id, &view.branch);
                }
            }
            Some(FilesInput::SaveTo(dir)) => {
                let Some(entry) = view.selected_entry() else {
                    return;
                };
                let Some(manifest_id) = view.listing.ready().map(|l| l.manifest_id) else {
                    return;
                };
                let home = libgaki::core::config::home_dir().unwrap_or_else(|_| PathBuf::from("/"));
                let out_dir = expand_dir(&dir, &home);
                if out_dir.as_os_str().is_empty() {
                    view.status = "Type a directory to save into.".to_string();
                    return;
                }
                let path = entry.path.clone();
                let name = path.rsplit('/').next().unwrap_or(&path).to_string();
                let depot_id = view.depot_id;
                view.status = format!("Downloading {name}…");
                let slot = spawn(move || {
                    steam::download_depot_file(app_id, depot_id, manifest_id, &path, &out_dir)
                });
                self.depots.save_dir = Some(dir.trim().to_string());
                self.depots.download = Some((name, slot));
            }
        }
    }

    /// Whether the file view's prompt is open (keys type into it).
    pub fn depot_input_open(&self) -> bool {
        self.depots
            .files
            .as_ref()
            .is_some_and(|v| v.input.is_some())
    }

    /// Adopt a finished manifest fetch or download.
    pub(super) fn poll_depot_files(&mut self) {
        if let Some(view) = &mut self.depots.files
            && view.listing.poll()
        {
            view.clamp();
        }
        let Some((name, slot)) = &self.depots.download else {
            return;
        };
        let Some(result) = take(slot) else {
            return;
        };
        let message = match result {
            Ok(path) => format!("Saved {}", path.display()),
            Err(e) => format!("Couldn't download {name}: {e}"),
        };
        self.depots.download = None;
        if let Some(view) = &mut self.depots.files {
            view.status = message.clone();
        }
        self.notice = Some(message);
    }
}

fn load_listing(app_id: i32, depot_id: u32, branch: &str) -> Load<DepotFiles> {
    let branch = branch.to_string();
    Load::start(move || steam::depot_files(app_id, depot_id, &branch))
}
