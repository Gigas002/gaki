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
    SteamPage,
    ProtonDb,
    ProtonIssues,
    Favourite,
    Hide,
}

/// One rendered row of the Actions menu: the action, the category header it
/// sits under, its human label, and the base-keymap key that also triggers it
/// (shown as a hint; empty when the action is reachable only from the menu).
pub struct ActionRow {
    pub action: Action,
    pub category: &'static str,
    pub label: &'static str,
    pub key: &'static str,
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
                key: "Space",
            });
            #[cfg(feature = "install")]
            rows.push(ActionRow {
                action: Action::CancelInstall,
                category: PLAY,
                label: "Cancel install",
                key: "c",
            });
        }
        if installed {
            rows.push(ActionRow {
                action: Action::Play,
                category: PLAY,
                label: "Play",
                key: "",
            });
            rows.push(ActionRow {
                action: Action::PlayAs,
                category: PLAY,
                label: "Play as… (launch target)",
                key: "",
            });
            #[cfg(feature = "install")]
            rows.push(ActionRow {
                action: Action::Update,
                category: PLAY,
                label: "Update",
                key: "U",
            });
            #[cfg(feature = "install")]
            rows.push(ActionRow {
                action: Action::Verify,
                category: PLAY,
                label: "Verify files",
                key: "v",
            });
            #[cfg(feature = "install")]
            rows.push(ActionRow {
                action: Action::Uninstall,
                category: PLAY,
                label: "Uninstall",
                key: "x",
            });
        } else if !installing {
            #[cfg(feature = "install")]
            rows.push(ActionRow {
                action: Action::Install,
                category: PLAY,
                label: "Install",
                key: "d",
            });
        }

        #[cfg(feature = "versioning")]
        const VERS: &str = "Versions";
        #[cfg(feature = "versioning")]
        rows.push(ActionRow {
            action: Action::Versions,
            category: VERS,
            label: "Versions & pinning",
            key: "V",
        });
        #[cfg(feature = "versioning")]
        rows.push(ActionRow {
            action: Action::Branches,
            category: VERS,
            label: "Beta branches",
            key: "b",
        });

        const RT: &str = "Runtimes & config";
        rows.push(ActionRow {
            action: Action::GameSettings,
            category: RT,
            label: "Game settings",
            key: "",
        });
        rows.push(ActionRow {
            action: Action::Proton,
            category: RT,
            label: "Proton runtimes",
            key: "P",
        });
        rows.push(ActionRow {
            action: Action::LaunchOptions,
            category: RT,
            label: "Launch options",
            key: "L",
        });

        const CONTENT: &str = "Content";
        rows.push(ActionRow {
            action: Action::Dlc,
            category: CONTENT,
            label: "DLC",
            key: "D",
        });
        #[cfg(feature = "workshop")]
        rows.push(ActionRow {
            action: Action::Workshop,
            category: CONTENT,
            label: "Workshop",
            key: "W",
        });
        #[cfg(feature = "cloud")]
        rows.push(ActionRow {
            action: Action::Cloud,
            category: CONTENT,
            label: "Cloud saves",
            key: "C",
        });
        rows.push(ActionRow {
            action: Action::Achievements,
            category: CONTENT,
            label: "Achievements",
            key: "a",
        });
        rows.push(ActionRow {
            action: Action::Depots,
            category: CONTENT,
            label: "Depots",
            key: "o",
        });
        #[cfg(feature = "inventory")]
        rows.push(ActionRow {
            action: Action::Inventory,
            category: CONTENT,
            label: "Inventory",
            key: "I",
        });

        #[cfg(feature = "store")]
        rows.push(ActionRow {
            action: Action::Store,
            category: CONTENT,
            label: "Store: price, players, news, reviews, similar",
            key: "Y",
        });

        const LINKS: &str = "Links";
        rows.push(ActionRow {
            action: Action::SteamPage,
            category: LINKS,
            label: "Steam store page",
            key: "E",
        });
        rows.push(ActionRow {
            action: Action::ProtonDb,
            category: LINKS,
            label: "ProtonDB page",
            key: "B",
        });
        rows.push(ActionRow {
            action: Action::ProtonIssues,
            category: LINKS,
            label: "Proton issues on GitHub",
            key: "T",
        });

        const LIB: &str = "Library";
        if installed {
            #[cfg(feature = "install")]
            rows.push(ActionRow {
                action: Action::Move,
                category: LIB,
                label: "Move install",
                key: "M",
            });
            #[cfg(feature = "install")]
            rows.push(ActionRow {
                action: Action::Relink,
                category: LIB,
                label: "Relink install",
                key: "K",
            });
        }
        #[cfg(feature = "install")]
        rows.push(ActionRow {
            action: Action::Import,
            category: LIB,
            label: "Import install",
            key: "N",
        });
        #[cfg(feature = "collections")]
        rows.push(ActionRow {
            action: Action::Collections,
            category: LIB,
            label: "Collections",
            key: "O",
        });
        rows.push(ActionRow {
            action: Action::Favourite,
            category: LIB,
            label: "Toggle favourite",
            key: "f",
        });
        rows.push(ActionRow {
            action: Action::Hide,
            category: LIB,
            label: "Hide game",
            key: "H",
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
