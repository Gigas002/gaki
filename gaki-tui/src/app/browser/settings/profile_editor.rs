//! The Profiles tab's editing: a new profile (`a`), the profile editor
//! (`Enter`/`e`: one row per setting, `Enter` changes it, `x` removes it),
//! and deleting one (`x`, after a y/n). Every save goes through
//! `libgaki::ops::config`, which keeps the file's comments and refuses a
//! change that would break the profile.

use libgaki::config::{Profile, Runner, ShaderCache, SteamIntegration};
use libgaki::ops::config::{Change, EnvChange, ProfileChange};

use super::{ProfilesTab, SettingsView};
use crate::app::browser::Browser;
use crate::app::browser::game_config::{cycle, join_words, parse_env, split_words};
use crate::app::steam;

/// The editor's rows, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileField {
    Runner,
    Prefix,
    Proton,
    Steam,
    Wrappers,
    RunnerArgs,
    Debug,
    Env,
    DllOverrides,
    ShaderCache,
}

impl ProfileField {
    pub const ALL: [Self; 10] = [
        Self::Runner,
        Self::Prefix,
        Self::Proton,
        Self::Steam,
        Self::Wrappers,
        Self::RunnerArgs,
        Self::Debug,
        Self::Env,
        Self::DllOverrides,
        Self::ShaderCache,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Runner => "Runner",
            Self::Prefix => "Prefix",
            Self::Proton => "Proton build",
            Self::Steam => "Steam",
            Self::Wrappers => "Wrappers",
            Self::RunnerArgs => "Runner args",
            Self::Debug => "Debug",
            Self::Env => "Variables",
            Self::DllOverrides => "DLL overrides",
            Self::ShaderCache => "Shader cache",
        }
    }

    /// Typed, not cycled.
    pub fn is_text(self) -> bool {
        !matches!(self, Self::Runner | Self::Steam | Self::Debug)
    }

    /// What the prompt asks for.
    pub fn prompt(self) -> &'static str {
        match self {
            Self::Prefix => "Prefix directory (~ works): ",
            Self::Proton => "Proton build (a name or a path): ",
            Self::Wrappers => "Wrappers, outermost first: ",
            Self::RunnerArgs => "Runner arguments: ",
            Self::Env => "Variables (KEY=VALUE): ",
            Self::DllOverrides => "DLL overrides (NAME=MODE, e.g. winhttp=n,b): ",
            _ => "Shader cache (directory or off): ",
        }
    }

    /// The value as the editor shows it; `None` when unset.
    pub fn value(self, profile: &Profile) -> Option<String> {
        let pairs = |map: &std::collections::BTreeMap<String, String>| {
            (!map.is_empty()).then(|| {
                join_words(
                    &map.iter()
                        .map(|(k, v)| format!("{k}={v}"))
                        .collect::<Vec<_>>(),
                )
            })
        };
        let words = |list: &[String]| (!list.is_empty()).then(|| join_words(list));
        match self {
            Self::Runner => Some(profile.runner.to_string()),
            Self::Prefix => profile.prefix.as_ref().map(|p| p.display().to_string()),
            Self::Proton => profile.proton.as_ref().and_then(|p| p.build.clone()),
            Self::Steam => profile.steam.map(|s| s.as_str().to_owned()),
            Self::Wrappers => words(&profile.wrappers),
            Self::RunnerArgs => words(&profile.runner_args),
            Self::Debug => Some(profile.debug.to_string()),
            Self::Env => pairs(&profile.env),
            Self::DllOverrides => pairs(&profile.dll_overrides),
            Self::ShaderCache => profile.shader_cache.as_ref().map(ShaderCache::as_text),
        }
    }

    /// What an unset value means.
    pub fn unset(self) -> &'static str {
        match self {
            Self::Prefix => "each game its own compatdata",
            Self::Proton => "default_proton",
            Self::Steam => "auto",
            Self::ShaderCache => "config.toml's",
            _ => "none",
        }
    }

    /// The change that removes it.
    fn clear(self) -> ProfileChange {
        let mut change = ProfileChange::default();
        match self {
            Self::Runner => {}
            Self::Prefix => change.prefix = Change::Clear,
            Self::Proton => change.proton_build = Change::Clear,
            Self::Steam => change.steam = Change::Clear,
            Self::Wrappers => change.wrappers = Change::Clear,
            Self::RunnerArgs => change.runner_args = Change::Clear,
            Self::Debug => change.debug = Some(false),
            Self::Env => change.env = EnvChange::replace(Default::default()),
            Self::DllOverrides => change.dll_overrides = EnvChange::replace(Default::default()),
            Self::ShaderCache => change.shader_cache = Change::Clear,
        }
        change
    }

    /// The change typed text makes; empty text removes the value. `Err`
    /// when the text doesn't parse.
    fn typed(self, text: &str) -> Result<ProfileChange, String> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(self.clear());
        }
        let mut change = ProfileChange::default();
        match self {
            Self::Prefix => change.prefix = Change::Set(text.to_owned()),
            Self::Proton => change.proton_build = Change::Set(text.to_owned()),
            Self::Wrappers => change.wrappers = Change::Set(split_words(text)?),
            Self::RunnerArgs => change.runner_args = Change::Set(split_words(text)?),
            Self::Env => change.env = EnvChange::replace(parse_env(text)?),
            Self::DllOverrides => change.dll_overrides = EnvChange::replace(parse_env(text)?),
            Self::ShaderCache => change.shader_cache = Change::Set(ShaderCache::parse(text)),
            Self::Runner | Self::Steam | Self::Debug => {}
        }
        Ok(change)
    }

    /// The next value of a cycled row.
    fn step(self, profile: &Profile) -> ProfileChange {
        let mut change = ProfileChange::default();
        match self {
            Self::Runner => {
                change.runner = Some(if profile.runner == Runner::Proton {
                    Runner::Native
                } else {
                    Runner::Proton
                });
            }
            // The default (auto) → auto → host → none → the default.
            Self::Steam => {
                let current = profile.steam.map(SteamIntegration::as_str);
                change.steam = match cycle(current, &["auto", "host", "none"]) {
                    Some("auto") => Change::Set(SteamIntegration::Auto),
                    Some("host") => Change::Set(SteamIntegration::Host),
                    Some(_) => Change::Set(SteamIntegration::None),
                    None => Change::Clear,
                };
            }
            Self::Debug => change.debug = Some(!profile.debug),
            _ => {}
        }
        change
    }
}

/// The open editor.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileEditor {
    /// The profile's name.
    pub name: String,
    /// The highlighted row.
    pub field: usize,
    /// The prompt's text while typing.
    pub input: Option<String>,
}

impl ProfileEditor {
    pub fn field(&self) -> ProfileField {
        ProfileField::ALL[self.field.min(ProfileField::ALL.len() - 1)]
    }
}

impl ProfilesTab {
    /// The edited profile's row.
    pub fn edited(&self) -> Option<&steam::ProfileRow> {
        let name = &self.editor.as_ref()?.name;
        self.data
            .as_ref()
            .ok()?
            .rows
            .iter()
            .find(|r| &r.name == name)
    }
}

impl SettingsView {
    /// Read the profiles again, keeping the highlight on `name` (or where
    /// it was) and the editor open.
    fn reload_profiles(&mut self, name: Option<&str>) {
        let Some(old) = self.profiles_tab.take() else {
            return;
        };
        let mut tab = ProfilesTab::load();
        let rows = tab.data.as_ref().map(|d| d.rows.as_slice()).unwrap_or(&[]);
        tab.selected = name
            .and_then(|name| rows.iter().position(|r| r.name == name))
            .unwrap_or(old.selected)
            .min(rows.len().saturating_sub(1));
        tab.editor = old.editor;
        tab.creating = None;
        self.profiles_tab = Some(tab);
        self.profiles = steam::profile_names().unwrap_or_default();
    }

    /// Save `change` to the edited profile.
    fn save_profile(&mut self, change: &ProfileChange, field: ProfileField) {
        let Some(name) = self
            .profiles_tab
            .as_ref()
            .and_then(|t| t.editor.as_ref())
            .map(|e| e.name.clone())
        else {
            return;
        };
        self.status = match steam::profile_update(&name, change) {
            Ok(()) => format!("{}: {} saved.", name, field.label()),
            Err(e) => format!("Not saved: {e}"),
        };
        self.reload_profiles(Some(&name));
    }
}

impl Browser {
    fn profiles_tab_mut(&mut self) -> Option<&mut ProfilesTab> {
        self.settings.profiles_tab.as_mut()
    }

    pub fn profile_editor_open(&self) -> bool {
        self.settings
            .profiles_tab
            .as_ref()
            .is_some_and(|t| t.editor.is_some())
    }

    pub fn profile_prompt_open(&self) -> bool {
        self.settings.profiles_tab.as_ref().is_some_and(|t| {
            t.creating.is_some() || t.editor.as_ref().is_some_and(|e| e.input.is_some())
        })
    }

    pub fn profile_delete_confirm_open(&self) -> bool {
        self.settings
            .profiles_tab
            .as_ref()
            .is_some_and(|t| t.confirm_delete.is_some())
    }

    /// `Enter`/`e`: edit the highlighted profile, when gaki runs it.
    pub fn edit_profile(&mut self) {
        let view = &mut self.settings;
        let Some(tab) = &mut view.profiles_tab else {
            return;
        };
        let Some(row) = tab.selected_row() else {
            return;
        };
        if !row.editable {
            let tool = if row.profile.runner == Runner::Slr {
                "sruman"
            } else {
                "lootrs"
            };
            view.status = format!(
                "{} runs `{}`, a {tool} runner: edit it with {tool} or by hand.",
                row.name, row.profile.runner
            );
            return;
        }
        tab.editor = Some(ProfileEditor {
            name: row.name.clone(),
            ..ProfileEditor::default()
        });
        view.status.clear();
    }

    pub fn close_profile_editor(&mut self) {
        if let Some(tab) = self.profiles_tab_mut() {
            tab.editor = None;
        }
    }

    /// `j`/`k` in the editor.
    pub fn profile_field_move(&mut self, delta: isize) {
        if let Some(editor) = self.profiles_tab_mut().and_then(|t| t.editor.as_mut()) {
            editor.field = editor
                .field
                .saturating_add_signed(delta)
                .min(ProfileField::ALL.len() - 1);
        }
    }

    /// `Enter` on a row: cycle it, or open the prompt with its value.
    pub fn profile_field_activate(&mut self) {
        let Some(tab) = &mut self.settings.profiles_tab else {
            return;
        };
        let Some(profile) = tab.edited().map(|r| r.profile.clone()) else {
            return;
        };
        let Some(editor) = &mut tab.editor else {
            return;
        };
        let field = editor.field();
        if field.is_text() {
            editor.input = Some(field.value(&profile).unwrap_or_default());
            self.settings.status.clear();
            return;
        }
        let change = field.step(&profile);
        self.settings.save_profile(&change, field);
    }

    /// `x` on a row: remove the setting.
    pub fn profile_field_unset(&mut self) {
        let Some(field) = self
            .settings
            .profiles_tab
            .as_ref()
            .and_then(|t| t.editor.as_ref())
            .map(ProfileEditor::field)
        else {
            return;
        };
        if field == ProfileField::Runner {
            self.settings.status = "A profile always has a runner.".into();
            return;
        }
        self.settings.save_profile(&field.clear(), field);
    }

    /// `a`: ask for the new profile's name.
    pub fn begin_new_profile(&mut self) {
        if let Some(tab) = self.profiles_tab_mut() {
            tab.creating = Some(String::new());
        }
    }

    /// `x`: ask before deleting the highlighted profile.
    pub fn begin_delete_profile(&mut self) {
        let view = &mut self.settings;
        let Some(tab) = &mut view.profiles_tab else {
            return;
        };
        let Some(row) = tab.selected_row() else {
            return;
        };
        let mut users = row.games.clone();
        if row.is_default {
            users.insert(0, "default_profile".into());
        }
        if !users.is_empty() {
            view.status = format!("{} is in use by {}.", row.name, users.join(", "));
            return;
        }
        tab.confirm_delete = Some(row.name.clone());
    }

    /// Answer the delete confirm.
    pub fn answer_delete_profile(&mut self, yes: bool) {
        let view = &mut self.settings;
        let Some(name) = view
            .profiles_tab
            .as_mut()
            .and_then(|t| t.confirm_delete.take())
        else {
            return;
        };
        if !yes {
            return;
        }
        view.status = match steam::profile_delete(&name) {
            Ok(()) => format!("Deleted {name}; its prefix stays."),
            Err(e) => format!("Not deleted: {e}"),
        };
        view.reload_profiles(None);
    }

    pub fn profile_edit_input_push(&mut self, c: char) {
        if let Some(text) = self.profile_edit_input() {
            text.push(c);
        }
    }

    pub fn profile_edit_input_pop(&mut self) {
        if let Some(text) = self.profile_edit_input() {
            text.pop();
        }
    }

    fn profile_edit_input(&mut self) -> Option<&mut String> {
        let tab = self.profiles_tab_mut()?;
        if let Some(name) = &mut tab.creating {
            return Some(name);
        }
        tab.editor.as_mut()?.input.as_mut()
    }

    pub fn cancel_profile_edit_input(&mut self) {
        if let Some(tab) = self.profiles_tab_mut()
            && tab.creating.take().is_none()
            && let Some(editor) = &mut tab.editor
        {
            editor.input = None;
        }
    }

    /// `Enter` in a prompt: create the named profile and open its editor,
    /// or save the typed value. Text that doesn't parse keeps the prompt.
    pub fn commit_profile_edit_input(&mut self) {
        let view = &mut self.settings;
        let Some(tab) = &mut view.profiles_tab else {
            return;
        };
        if let Some(name) = tab.creating.take() {
            let name = name.trim().to_owned();
            if name.is_empty() {
                return;
            }
            match steam::profile_create(&name) {
                Ok(()) => {
                    view.reload_profiles(Some(&name));
                    self.edit_profile();
                    self.settings.status = format!("Created {name}.");
                }
                Err(e) => {
                    view.status = format!("Not created: {e}");
                    tab.creating = Some(name);
                }
            }
            return;
        }
        let Some(editor) = &mut tab.editor else {
            return;
        };
        let Some(text) = editor.input.clone() else {
            return;
        };
        let field = editor.field();
        match field.typed(&text) {
            Err(e) => view.status = format!("Invalid: {e}"),
            Ok(change) => {
                editor.input = None;
                view.save_profile(&change, field);
            }
        }
    }
}
