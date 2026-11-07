//! The Settings overlay (`p`), in four tabs (`Tab` switches):
//!
//! * Settings: every `config.toml` key, each with its value or its default.
//!   `Enter` flips a switch, cycles a choice or opens a prompt; `x` unsets a
//!   key. Values go through the same validation as `gaki-cli config set`, so
//!   a bad one is refused with its reason.
//! * Profiles: each profile with its settings, its file, the games using it
//!   and its issues; `d` makes it the default.
//! * Check: `gaki-cli config check`, run when the tab is shown; `r` re-runs.
//! * Daemons: the running `gaki-cli` session daemons; `s` stops the
//!   highlighted one and `S` all of them, each after a y/n.

use libgaki::ops::daemon::GakiProcess;

use super::Browser;
use crate::app::steam;

/// How a key is edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edit {
    /// `true` / `false`, with the value an unset key has.
    Toggle { default: bool },
    /// One of `options`. With a `default` the key always has one of them;
    /// without, unset is one of the steps.
    Choice {
        options: &'static [&'static str],
        default: Option<&'static str>,
    },
    /// One of the profiles, or unset.
    Profile,
    /// Typed text (paths and numbers too; the save validates them).
    Prompt,
}

/// One key: how it's shown and edited.
#[derive(Debug, Clone, Copy)]
pub struct Spec {
    pub key: &'static str,
    pub label: &'static str,
    /// What an unset key means.
    pub default: &'static str,
    pub edit: Edit,
    /// When a change takes effect, if not right away.
    pub note: Option<&'static str>,
    /// Asked before a change is saved.
    pub confirm: Option<&'static str>,
}

/// The proxy is put in the environment once, when a gaki program starts.
const NEXT_START: &str =
    "Saved. It applies when gaki next starts (for gaki-cli's daemon: `gaki-cli daemon stop`).";

/// Every `config.toml` key, in `gaki-cli config show` order.
pub const SPECS: &[Spec] = &[
    Spec {
        key: "steam_library",
        label: "Steam library",
        default: "the detected Steam library",
        edit: Edit::Prompt,
        note: Some("Saved. Reload the library (r) to use it."),
        confirm: None,
    },
    Spec {
        key: "profiles_dir",
        label: "Profiles directory",
        default: "<config dir>/profiles",
        edit: Edit::Prompt,
        note: Some("Saved. Reload the library (r) to use it."),
        confirm: None,
    },
    Spec {
        key: "default_profile",
        label: "Default profile",
        default: "none",
        edit: Edit::Profile,
        note: None,
        confirm: None,
    },
    Spec {
        key: "default_platform",
        label: "Default platform",
        default: "install Windows, run what's installed",
        edit: Edit::Choice {
            options: &["windows", "linux"],
            default: None,
        },
        note: None,
        confirm: None,
    },
    Spec {
        key: "default_proton",
        label: "Default Proton",
        default: "experimental",
        edit: Edit::Prompt,
        note: None,
        confirm: None,
    },
    Spec {
        key: "cloud_sync",
        label: "Cloud sync",
        default: "true",
        edit: Edit::Toggle { default: true },
        note: None,
        confirm: None,
    },
    Spec {
        key: "presence",
        label: "Presence",
        default: "offline",
        edit: Edit::Choice {
            options: &["offline", "online"],
            default: Some("offline"),
        },
        note: Some("Saved, and announced now if you're signed in."),
        confirm: None,
    },
    Spec {
        key: "proxy",
        label: "Proxy",
        default: "direct connection",
        edit: Edit::Prompt,
        note: Some(NEXT_START),
        confirm: None,
    },
    Spec {
        key: "no_proxy",
        label: "Proxy bypass",
        default: "none",
        edit: Edit::Prompt,
        note: Some(NEXT_START),
        confirm: None,
    },
    Spec {
        key: "install_resume",
        label: "Resume installs",
        default: "manual",
        edit: Edit::Choice {
            options: &["manual", "auto"],
            default: Some("manual"),
        },
        note: Some("Saved. gaki-cli's daemon reads it when it starts."),
        confirm: None,
    },
    Spec {
        key: "experimental",
        label: "Experimental features",
        default: "false",
        edit: Edit::Toggle { default: false },
        note: None,
        confirm: None,
    },
    Spec {
        key: "info_cache_ttl",
        label: "Info cache (seconds)",
        default: "21600 (6 hours); 0 turns it off",
        edit: Edit::Prompt,
        note: None,
        confirm: None,
    },
    Spec {
        key: "daemon_idle_timeout",
        label: "Daemon idle timeout (s)",
        default: "1800 (30 minutes); 0 never",
        edit: Edit::Prompt,
        note: Some("Saved. It applies when the session daemon next starts."),
        confirm: None,
    },
    Spec {
        key: "reauth_sensitive",
        label: "Sign in again for money data",
        default: "false",
        edit: Edit::Toggle { default: false },
        note: None,
        confirm: None,
    },
    Spec {
        key: "secret_store",
        label: "Session tokens",
        default: "keyring",
        edit: Edit::Choice {
            options: &["keyring", "memory"],
            default: Some("keyring"),
        },
        note: Some("Saved. It applies from the next sign-in."),
        confirm: Some(
            "Change where the session tokens are kept? With `memory` you sign in \
             again whenever the session ends.",
        ),
    },
];

/// The value after `current` when `Enter` steps a switch or a choice;
/// `None` (outer) for keys edited in a prompt. The inner `None` unsets.
pub fn next_value(
    edit: Edit,
    current: Option<&str>,
    profiles: &[String],
) -> Option<Option<String>> {
    let step = |options: Vec<Option<String>>, current: Option<String>| {
        let at = options.iter().position(|o| *o == current);
        let next = at.map_or(0, |i| (i + 1) % options.len());
        options.into_iter().nth(next).flatten()
    };
    match edit {
        Edit::Toggle { default } => {
            let on = current.and_then(|c| c.parse().ok()).unwrap_or(default);
            Some(Some((!on).to_string()))
        }
        Edit::Choice {
            options,
            default: Some(default),
        } => {
            let all = options.iter().map(|o| Some(o.to_string())).collect();
            Some(step(all, Some(current.unwrap_or(default).to_string())))
        }
        Edit::Choice {
            options,
            default: None,
        } => {
            let all = std::iter::once(None)
                .chain(options.iter().map(|o| Some(o.to_string())))
                .collect();
            Some(step(all, current.map(str::to_string)))
        }
        Edit::Profile => {
            let all = std::iter::once(None)
                .chain(profiles.iter().cloned().map(Some))
                .collect();
            Some(step(all, current.map(str::to_string)))
        }
        Edit::Prompt => None,
    }
}

/// The overlay's tabs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Settings,
    Profiles,
    Check,
    Daemons,
}

impl Tab {
    pub const ALL: [Tab; 4] = [Tab::Settings, Tab::Profiles, Tab::Check, Tab::Daemons];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Settings => "Settings",
            Tab::Profiles => "Profiles",
            Tab::Check => "Check",
            Tab::Daemons => "Daemons",
        }
    }

    fn step(self, forward: bool) -> Tab {
        let i = Tab::ALL.iter().position(|t| *t == self).unwrap_or(0);
        let n = Tab::ALL.len();
        Tab::ALL[if forward {
            (i + 1) % n
        } else {
            (i + n - 1) % n
        }]
    }
}

/// The Profiles tab.
#[derive(Debug, Clone)]
pub struct ProfilesTab {
    pub data: Result<steam::Profiles, String>,
    pub selected: usize,
    /// The detail pane's scroll, in rows.
    pub scroll: u16,
}

impl ProfilesTab {
    pub fn with(data: steam::Profiles) -> Self {
        Self {
            data: Ok(data),
            selected: 0,
            scroll: 0,
        }
    }

    fn load() -> Self {
        Self {
            data: steam::profiles().map_err(|e| e.to_string()),
            selected: 0,
            scroll: 0,
        }
    }

    pub fn selected_row(&self) -> Option<&steam::ProfileRow> {
        self.data.as_ref().ok()?.rows.get(self.selected)
    }
}

/// The Check tab.
#[derive(Debug, Clone)]
pub struct CheckTab {
    pub findings: Result<Vec<steam::CheckFinding>, String>,
    pub scroll: u16,
}

impl CheckTab {
    pub fn with(findings: Vec<steam::CheckFinding>) -> Self {
        Self {
            findings: Ok(findings),
            scroll: 0,
        }
    }

    fn run() -> Self {
        Self {
            findings: steam::config_check().map_err(|e| e.to_string()),
            scroll: 0,
        }
    }
}

/// A daemon stop waiting for its y/n.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopConfirm {
    pub pids: Vec<u32>,
    /// `S`: every listed daemon.
    pub all: bool,
}

/// The Daemons tab.
#[derive(Debug, Clone, Default)]
pub struct DaemonsTab {
    pub rows: Vec<GakiProcess>,
    pub selected: usize,
    pub confirm: Option<StopConfirm>,
}

impl DaemonsTab {
    pub fn with(rows: Vec<GakiProcess>) -> Self {
        Self {
            rows,
            ..Self::default()
        }
    }

    fn load() -> Self {
        Self::with(steam::daemons())
    }

    pub fn selected_row(&self) -> Option<&GakiProcess> {
        self.rows.get(self.selected)
    }

    /// Re-read the list, keeping the highlight on the same PID if it's
    /// still running.
    fn reload(&mut self) {
        let keep = self.selected_row().map(|p| p.pid);
        self.rows = steam::daemons();
        self.selected = keep
            .and_then(|pid| self.rows.iter().position(|p| p.pid == pid))
            .unwrap_or(0)
            .min(self.rows.len().saturating_sub(1));
    }
}

/// A change waiting for its y/n.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    pub index: usize,
    pub value: Option<String>,
}

/// The overlay's state.
#[derive(Debug, Clone, Default)]
pub struct SettingsView {
    /// What each of [`SPECS`] is set to, by index; `None` when unset.
    pub values: Vec<Option<String>>,
    /// Why the values couldn't be read.
    pub error: Option<String>,
    pub selected: usize,
    /// The prompt's text while editing the highlighted key.
    pub input: Option<String>,
    pub confirm: Option<Pending>,
    /// The last save's result.
    pub status: String,
    pub profiles: Vec<String>,
    pub tab: Tab,
    /// Loaded when the Profiles tab is first shown.
    pub profiles_tab: Option<ProfilesTab>,
    /// Run each time the Check tab is shown.
    pub check: Option<CheckTab>,
    /// Read each time the Daemons tab is shown.
    pub daemons: Option<DaemonsTab>,
}

impl SettingsView {
    /// A view over `values` (aligned with [`SPECS`]), without reading
    /// anything (tests, previews).
    pub fn with_values(values: Vec<Option<String>>) -> Self {
        Self {
            values,
            ..Self::default()
        }
    }

    fn load() -> Self {
        let mut view = Self::default();
        view.reload();
        view.profiles = steam::profile_names().unwrap_or_default();
        view
    }

    fn reload(&mut self) {
        match steam::settings_values() {
            Ok(values) => {
                self.values = SPECS
                    .iter()
                    .map(|spec| {
                        values
                            .iter()
                            .find(|(key, _)| *key == spec.key)
                            .and_then(|(_, value)| value.clone())
                    })
                    .collect();
                self.error = None;
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    pub fn value(&self, index: usize) -> Option<&str> {
        self.values.get(index).and_then(|v| v.as_deref())
    }

    /// Save `value` for key `index`, or ask first when the key wants that.
    fn apply(&mut self, index: usize, value: Option<String>) {
        let spec = SPECS[index];
        if value.as_deref() == self.value(index) {
            return;
        }
        if spec.confirm.is_some() {
            self.confirm = Some(Pending { index, value });
            return;
        }
        self.save(index, value);
    }

    fn save(&mut self, index: usize, value: Option<String>) {
        let spec = SPECS[index];
        self.status = match steam::settings_set(spec.key, value.as_deref()) {
            Ok(()) => spec.note.unwrap_or("Saved.").to_string(),
            Err(e) => e.to_string(),
        };
        self.reload();
    }
}

impl Browser {
    /// Open Settings. It reads only local files.
    pub fn open_config(&mut self) {
        self.settings = SettingsView::load();
        self.show_config = true;
    }

    pub fn close_config(&mut self) {
        self.show_config = false;
        self.settings = SettingsView::default();
    }

    pub fn settings_move(&mut self, delta: isize) {
        let last = SPECS.len() - 1;
        let view = &mut self.settings;
        view.selected = view.selected.saturating_add_signed(delta).min(last);
    }

    /// `Enter`: flip, cycle, or open the prompt prefilled with the value.
    pub fn settings_activate(&mut self) {
        let view = &mut self.settings;
        let index = view.selected;
        let spec = SPECS[index];
        match next_value(spec.edit, view.value(index), &view.profiles) {
            Some(value) => view.apply(index, value),
            None => view.input = Some(view.value(index).unwrap_or_default().to_string()),
        }
    }

    /// Show the next (`Tab`) or previous (`Shift-Tab`) tab. Profiles load
    /// the first time; the check runs each time it's shown.
    pub fn settings_tab(&mut self, forward: bool) {
        let view = &mut self.settings;
        view.tab = view.tab.step(forward);
        match view.tab {
            Tab::Profiles if view.profiles_tab.is_none() => {
                view.profiles_tab = Some(ProfilesTab::load());
            }
            Tab::Check => view.check = Some(CheckTab::run()),
            Tab::Daemons => view.daemons = Some(DaemonsTab::load()),
            _ => {}
        }
    }

    /// `j`/`k` in the Profiles tab.
    pub fn profiles_move(&mut self, delta: isize) {
        if let Some(tab) = &mut self.settings.profiles_tab {
            let len = tab.data.as_ref().map_or(0, |d| d.rows.len());
            tab.selected = tab
                .selected
                .saturating_add_signed(delta)
                .min(len.saturating_sub(1));
            tab.scroll = 0;
        }
    }

    /// Scroll the Profiles detail pane or the Check list by `delta` rows.
    pub fn settings_scroll(&mut self, delta: i16) {
        let view = &mut self.settings;
        let scroll = match view.tab {
            Tab::Profiles => view.profiles_tab.as_mut().map(|t| &mut t.scroll),
            Tab::Check => view.check.as_mut().map(|t| &mut t.scroll),
            Tab::Settings | Tab::Daemons => None,
        };
        if let Some(scroll) = scroll {
            *scroll = scroll.saturating_add_signed(delta);
        }
    }

    /// `r` in the Check tab: check again.
    pub fn rerun_check(&mut self) {
        self.settings.check = Some(CheckTab::run());
    }

    /// `j`/`k` in the Daemons tab.
    pub fn daemons_move(&mut self, delta: isize) {
        if let Some(tab) = &mut self.settings.daemons {
            tab.selected = tab
                .selected
                .saturating_add_signed(delta)
                .min(tab.rows.len().saturating_sub(1));
        }
    }

    /// `r` in the Daemons tab: read the list again.
    pub fn reload_daemons(&mut self) {
        if let Some(tab) = &mut self.settings.daemons {
            tab.reload();
        }
    }

    /// Ask to stop the highlighted daemon (`s`) or every one listed (`S`).
    pub fn begin_daemon_stop(&mut self, all: bool) {
        let view = &mut self.settings;
        let Some(tab) = &mut view.daemons else {
            return;
        };
        let pids: Vec<u32> = if all {
            tab.rows.iter().map(|p| p.pid).collect()
        } else {
            tab.selected_row().map(|p| p.pid).into_iter().collect()
        };
        if pids.is_empty() {
            view.status = "No daemon is running.".to_string();
            return;
        }
        tab.confirm = Some(StopConfirm { pids, all });
    }

    pub fn daemon_confirm_open(&self) -> bool {
        self.settings
            .daemons
            .as_ref()
            .is_some_and(|t| t.confirm.is_some())
    }

    /// Answer the stop confirm: `yes` stops them and re-reads the list.
    pub fn answer_daemon_stop(&mut self, yes: bool) {
        let view = &mut self.settings;
        let Some(tab) = &mut view.daemons else {
            return;
        };
        let Some(confirm) = tab.confirm.take() else {
            return;
        };
        if !yes {
            return;
        }
        let stopped = steam::stop_daemons(&confirm.pids);
        view.status = match (stopped, confirm.pids.len()) {
            (0, _) => "No daemon was stopped; it may have exited already.".to_string(),
            (1, 1) => format!("Stopped the daemon (PID {}).", confirm.pids[0]),
            (n, total) if n == total => format!("Stopped {n} daemons."),
            (n, total) => format!("Stopped {n} of {total} daemons."),
        };
        tab.reload();
    }

    /// `d` in the Profiles tab: make the highlighted profile the default.
    pub fn make_profile_default(&mut self) {
        let view = &mut self.settings;
        let Some(row) = view
            .profiles_tab
            .as_ref()
            .and_then(ProfilesTab::selected_row)
        else {
            return;
        };
        if row.is_default {
            view.status = format!("{} is already the default.", row.name);
            return;
        }
        let name = row.name.clone();
        view.status = match steam::settings_set("default_profile", Some(&name)) {
            Ok(()) => format!("{name} is now the default profile."),
            Err(e) => e.to_string(),
        };
        view.reload();
        let selected = view.profiles_tab.as_ref().map_or(0, |t| t.selected);
        let mut tab = ProfilesTab::load();
        tab.selected = selected;
        view.profiles_tab = Some(tab);
    }

    /// `x`: unset the highlighted key, back to its default.
    pub fn settings_unset(&mut self) {
        let index = self.settings.selected;
        self.settings.apply(index, None);
    }

    pub fn settings_input_push(&mut self, c: char) {
        if let Some(text) = &mut self.settings.input {
            text.push(c);
        }
    }

    pub fn settings_input_pop(&mut self) {
        if let Some(text) = &mut self.settings.input {
            text.pop();
        }
    }

    pub fn cancel_settings_input(&mut self) {
        self.settings.input = None;
    }

    /// `Enter` in the prompt: save the text; empty unsets the key.
    pub fn commit_settings_input(&mut self) {
        let Some(text) = self.settings.input.take() else {
            return;
        };
        let text = text.trim();
        let value = (!text.is_empty()).then(|| text.to_string());
        let index = self.settings.selected;
        self.settings.apply(index, value);
    }

    /// Answer the confirm: `yes` saves the change.
    pub fn answer_settings_confirm(&mut self, yes: bool) {
        let Some(pending) = self.settings.confirm.take() else {
            return;
        };
        if yes {
            self.settings.save(pending.index, pending.value);
        }
    }
}

#[cfg(test)]
mod tests;
