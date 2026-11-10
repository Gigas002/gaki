//! The per-game Actions menu (command palette).

use super::Browser;
#[cfg(feature = "install")]
use super::InstallPhase;

/// A single entry in the per-game Actions menu. Selecting one runs the same
/// code path as its base-keymap accelerator (`app::keys::actions` and
/// `app::keys::browse`, through `app::keys::shared`), so the two never drift apart.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Play,
    PlayAs,
    #[cfg(feature = "install")]
    Install,
    #[cfg(feature = "install")]
    PauseResume,
    #[cfg(feature = "install")]
    CancelInstall,
    #[cfg(feature = "install")]
    Update,
    #[cfg(feature = "install")]
    Verify,
    #[cfg(feature = "install")]
    PrecacheShaders,
    #[cfg(feature = "install")]
    Uninstall,
    #[cfg(feature = "versioning")]
    Versions,
    #[cfg(feature = "versioning")]
    Branches,
    GameSettings,
    Proton,
    LaunchOptions,
    Dlc,
    #[cfg(feature = "workshop")]
    Workshop,
    #[cfg(feature = "cloud")]
    Cloud,
    Achievements,
    Depots,
    #[cfg(feature = "inventory")]
    Inventory,
    #[cfg(feature = "install")]
    Move,
    #[cfg(feature = "install")]
    Relink,
    #[cfg(feature = "install")]
    Import,
    #[cfg(feature = "collections")]
    Collections,
    #[cfg(feature = "store")]
    Store,
    GameInfo,
    SteamPage,
    ProtonDb,
    ProtonIssues,
    SteamDb,
    Favourite,
    Hide,
}

/// One rendered row of the Actions menu: the action, the category header it
/// sits under, and its human label. [`ActionRow::key`] is the library key
/// that also triggers it.
pub struct ActionRow {
    pub action: Action,
    pub category: &'static str,
    pub label: &'static str,
}

impl ActionRow {
    /// The library key that also runs this action, from the live keymap
    /// (shown as a hint); empty when only the menu reaches it.
    pub fn key(&self) -> String {
        self.action
            .shortcut()
            .map(|a| crate::keymap::get().first(a))
            .unwrap_or_default()
    }
}

impl Action {
    /// The library action that runs the same thing, if any.
    pub fn shortcut(self) -> Option<crate::keymap::LibraryAction> {
        use crate::keymap::LibraryAction as A;
        Some(match self {
            Action::Play | Action::PlayAs | Action::GameSettings => return None,
            #[cfg(feature = "install")]
            Action::PrecacheShaders => return None,
            #[cfg(feature = "install")]
            Action::Install => A::Install,
            #[cfg(feature = "install")]
            Action::PauseResume => A::PauseResume,
            #[cfg(feature = "install")]
            Action::CancelInstall => A::CancelInstall,
            #[cfg(feature = "install")]
            Action::Update => A::Update,
            #[cfg(feature = "install")]
            Action::Verify => A::Verify,
            #[cfg(feature = "install")]
            Action::Uninstall => A::Uninstall,
            #[cfg(feature = "versioning")]
            Action::Versions => A::Versions,
            #[cfg(feature = "versioning")]
            Action::Branches => A::Branches,
            Action::Proton => A::Proton,
            Action::LaunchOptions => A::LaunchOptions,
            Action::Dlc => A::Dlc,
            #[cfg(feature = "workshop")]
            Action::Workshop => A::Workshop,
            #[cfg(feature = "cloud")]
            Action::Cloud => A::Cloud,
            Action::Achievements => A::Achievements,
            Action::Depots => A::Depots,
            #[cfg(feature = "inventory")]
            Action::Inventory => A::Inventory,
            #[cfg(feature = "install")]
            Action::Move => A::MoveInstall,
            #[cfg(feature = "install")]
            Action::Relink => A::Relink,
            #[cfg(feature = "install")]
            Action::Import => A::Import,
            #[cfg(feature = "collections")]
            Action::Collections => A::Collections,
            #[cfg(feature = "store")]
            Action::Store => A::Store,
            Action::GameInfo => A::GameInfo,
            Action::SteamPage => A::SteamPage,
            Action::ProtonDb => A::ProtonDb,
            Action::ProtonIssues => A::ProtonIssues,
            Action::SteamDb => A::SteamDb,
            Action::Favourite => A::Favourite,
            Action::Hide => A::Hide,
        })
    }
}

impl Browser {
    /// The full set of actions applicable to the currently selected game, before
    /// the live filter is applied. Rows are grouped by `category` in display
    /// order; only actions that make sense for the game's state are included
    /// (e.g. `Update`/`Uninstall` only when installed, `Install` only when not).
    pub fn action_candidates(&self) -> Vec<ActionRow> {
        let mut rows: Vec<ActionRow> = Vec::new();
        let Some(game) = self.selected() else {
            return rows;
        };
        let installed = game.installed;
        #[cfg(feature = "install")]
        let installing = self.install_phase(&game) != InstallPhase::Idle;
        #[cfg(not(feature = "install"))]
        let installing = false;

        const PLAY: &str = "Play & Install";
        if installing {
            #[cfg(feature = "install")]
            rows.push(ActionRow {
                action: Action::PauseResume,
                category: PLAY,
                label: "Pause / resume install",
            });
            #[cfg(feature = "install")]
            rows.push(ActionRow {
                action: Action::CancelInstall,
                category: PLAY,
                label: "Cancel install",
            });
        }
        if installed {
            rows.push(ActionRow {
                action: Action::Play,
                category: PLAY,
                label: "Play",
            });
            rows.push(ActionRow {
                action: Action::PlayAs,
                category: PLAY,
                label: "Play as… (launch target)",
            });
            #[cfg(feature = "install")]
            rows.push(ActionRow {
                action: Action::Update,
                category: PLAY,
                label: "Update",
            });
            #[cfg(feature = "install")]
            rows.push(ActionRow {
                action: Action::Verify,
                category: PLAY,
                label: "Verify files",
            });
            #[cfg(feature = "install")]
            rows.push(ActionRow {
                action: Action::PrecacheShaders,
                category: PLAY,
                label: "Pre-cache shaders",
            });
            #[cfg(feature = "install")]
            rows.push(ActionRow {
                action: Action::Uninstall,
                category: PLAY,
                label: "Uninstall",
            });
        } else if !installing {
            #[cfg(feature = "install")]
            rows.push(ActionRow {
                action: Action::Install,
                category: PLAY,
                label: "Install",
            });
        }

        #[cfg(feature = "versioning")]
        const VERS: &str = "Versions";
        #[cfg(feature = "versioning")]
        rows.push(ActionRow {
            action: Action::Versions,
            category: VERS,
            label: "Versions & pinning",
        });
        #[cfg(feature = "versioning")]
        rows.push(ActionRow {
            action: Action::Branches,
            category: VERS,
            label: "Beta branches",
        });

        const RT: &str = "Runtimes & config";
        rows.push(ActionRow {
            action: Action::GameSettings,
            category: RT,
            label: "Game settings",
        });
        rows.push(ActionRow {
            action: Action::Proton,
            category: RT,
            label: "Proton runtimes",
        });
        rows.push(ActionRow {
            action: Action::LaunchOptions,
            category: RT,
            label: "Launch options",
        });

        const CONTENT: &str = "Content";
        rows.push(ActionRow {
            action: Action::Dlc,
            category: CONTENT,
            label: "DLC",
        });
        #[cfg(feature = "workshop")]
        rows.push(ActionRow {
            action: Action::Workshop,
            category: CONTENT,
            label: "Workshop",
        });
        #[cfg(feature = "cloud")]
        rows.push(ActionRow {
            action: Action::Cloud,
            category: CONTENT,
            label: "Cloud saves",
        });
        rows.push(ActionRow {
            action: Action::Achievements,
            category: CONTENT,
            label: "Achievements",
        });
        rows.push(ActionRow {
            action: Action::Depots,
            category: CONTENT,
            label: "Depots",
        });
        #[cfg(feature = "inventory")]
        rows.push(ActionRow {
            action: Action::Inventory,
            category: CONTENT,
            label: "Inventory",
        });

        #[cfg(feature = "store")]
        rows.push(ActionRow {
            action: Action::Store,
            category: CONTENT,
            label: "Store: price, players, news, reviews, similar",
        });
        rows.push(ActionRow {
            action: Action::GameInfo,
            category: CONTENT,
            label: "Game info: details, DRM and files checks, save artwork",
        });

        const LINKS: &str = "Links";
        rows.push(ActionRow {
            action: Action::SteamPage,
            category: LINKS,
            label: "Steam store page",
        });
        rows.push(ActionRow {
            action: Action::ProtonDb,
            category: LINKS,
            label: "ProtonDB page",
        });
        rows.push(ActionRow {
            action: Action::ProtonIssues,
            category: LINKS,
            label: "Proton issues on GitHub",
        });
        rows.push(ActionRow {
            action: Action::SteamDb,
            category: LINKS,
            label: "SteamDB page",
        });

        const LIB: &str = "Library";
        if installed {
            #[cfg(feature = "install")]
            rows.push(ActionRow {
                action: Action::Move,
                category: LIB,
                label: "Move install",
            });
            #[cfg(feature = "install")]
            rows.push(ActionRow {
                action: Action::Relink,
                category: LIB,
                label: "Relink install",
            });
        }
        #[cfg(feature = "install")]
        rows.push(ActionRow {
            action: Action::Import,
            category: LIB,
            label: "Import install",
        });
        #[cfg(feature = "collections")]
        rows.push(ActionRow {
            action: Action::Collections,
            category: LIB,
            label: "Collections",
        });
        rows.push(ActionRow {
            action: Action::Favourite,
            category: LIB,
            label: "Toggle favourite",
        });
        rows.push(ActionRow {
            action: Action::Hide,
            category: LIB,
            label: "Hide game",
        });

        rows
    }

    /// The Actions menu rows after applying the live `actions_filter` (a
    /// case-insensitive substring match on the label). This is what the menu
    /// widget renders and what the selection indexes into.
    pub fn filtered_actions(&self) -> Vec<ActionRow> {
        let needle = self.actions_filter.to_lowercase();
        self.action_candidates()
            .into_iter()
            .filter(|r| needle.is_empty() || r.label.to_lowercase().contains(&needle))
            .collect()
    }

    /// Open the Actions menu for the selected game.
    pub fn open_actions(&mut self) {
        if let Some(game) = self.selected() {
            self.actions_app_id = game.id;
            self.actions_index = 0;
            self.actions_filter.clear();
            self.show_actions = true;
        }
    }

    /// Close the Actions menu.
    pub fn close_actions(&mut self) {
        self.show_actions = false;
        self.actions_filter.clear();
        self.actions_index = 0;
    }

    pub fn actions_next(&mut self) {
        let n = self.filtered_actions().len();
        if n == 0 {
            self.actions_index = 0;
            return;
        }
        self.actions_index = (self.actions_index + 1) % n;
    }

    pub fn actions_previous(&mut self) {
        let n = self.filtered_actions().len();
        if n == 0 {
            self.actions_index = 0;
            return;
        }
        self.actions_index = if self.actions_index == 0 {
            n - 1
        } else {
            self.actions_index - 1
        };
    }

    /// The action currently highlighted in the (filtered) menu.
    pub fn selected_action(&self) -> Option<Action> {
        self.filtered_actions()
            .get(self.actions_index)
            .map(|r| r.action)
    }

    /// Append to the Actions menu filter (typing narrows the list).
    pub fn actions_filter_push(&mut self, c: char) {
        self.actions_filter.push(c);
        self.actions_index = 0;
    }

    /// Backspace the Actions menu filter.
    pub fn actions_filter_pop(&mut self) {
        self.actions_filter.pop();
        self.actions_index = 0;
    }
}
