//! The library list: tab filters, the fuzzy query, sorting, the selection,
//! and the per-game badges and tab counts.

use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use ratatui::style::Style;

use super::Browser;
use crate::app::game::Game;
use crate::config::Config;
use crate::ui::theme;

/// The quick-filter tabs along the top of the library.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    All,
    Installed,
    Updates,
    Favourites,
}

impl Filter {
    /// Tab order, left to right.
    pub const TABS: [Filter; 4] = [
        Filter::All,
        Filter::Installed,
        Filter::Updates,
        Filter::Favourites,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Filter::All => "All",
            Filter::Installed => "Installed",
            Filter::Updates => "Updates",
            Filter::Favourites => "Favourites",
        }
    }

    pub fn index(self) -> usize {
        Filter::TABS.iter().position(|f| *f == self).unwrap_or(0)
    }
}

/// Which top-level view the tab bar is on. The four library filters all live in
/// the `Library` view; selecting the `Friends` tab (one slot to the right of
/// the filters) swaps the main list for the Friends & Chat panel and grants it
/// keyboard focus. The tab bar treats these as a single ring: the filters
/// occupy slots `0..TABS.len()` and Friends occupies the slot after them.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View {
    Library,
    #[cfg(feature = "social")]
    Friends,
}

/// Sort order for the visible list.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    Name,
    Installed,
}

impl Sort {
    pub fn label(self) -> &'static str {
        match self {
            Sort::Name => "name",
            Sort::Installed => "installed",
        }
    }

    pub fn next(self) -> Sort {
        match self {
            Sort::Name => Sort::Installed,
            Sort::Installed => Sort::Name,
        }
    }
}

/// Live tallies shown in the status bar (computed over the browsable universe,
/// i.e. after hidden/allowed-type filtering but independent of the active tab).
pub struct Counts {
    pub total: usize,
    pub visible: usize,
    pub installed: usize,
    pub updates: usize,
    pub downloading: usize,
    /// A background update check is running.
    pub checking_updates: bool,
}

/// What a game's status badge should look like.
pub struct Badge {
    pub glyph: &'static str,
    pub style: Style,
    /// Short trailing note (e.g. a download percentage or "update").
    pub note: Option<String>,
}

/// Classify a game into a status badge (glyph + colour + optional note).
pub fn badge(game: &Game) -> Badge {
    let status = game.get_status();
    let state = status.as_ref().map(|s| s.state.as_str()).unwrap_or("");

    if state.starts_with("paused") {
        // A paused download: freeze the last-seen percent next to a pause glyph.
        let note = state
            .split_whitespace()
            .find(|t| t.ends_with('%'))
            .map(|p| p.to_string());
        Badge {
            glyph: "⏸",
            style: Style::default().fg(theme::WARN),
            note,
        }
    } else if state.contains("downloading")
        || state.contains("processing")
        || state.contains("verifying")
    {
        // Pull a percentage out of e.g. "downloading 42.0%" if present.
        let note = state
            .split_whitespace()
            .find(|t| t.ends_with('%'))
            .map(|p| p.to_string());
        Badge {
            glyph: "⬇",
            style: Style::default().fg(theme::ACCENT_BRIGHT),
            note,
        }
    } else if state.contains("Failed") {
        Badge {
            glyph: "✖",
            style: theme::item_failed(),
            note: Some("failed".to_string()),
        }
    } else if state.contains("update") || game.update_available {
        Badge {
            glyph: "▲",
            style: Style::default().fg(theme::WARN),
            note: Some("update".to_string()),
        }
    } else if game.installed
        || state == "installed"
        || state.contains("Installed")
        || state.contains("ran")
    {
        Badge {
            glyph: "●",
            style: Style::default().fg(theme::ONLINE),
            note: None,
        }
    } else {
        Badge {
            glyph: "○",
            style: theme::dim(),
            note: None,
        }
    }
}

impl Browser {
    /// Toggle the expanded/collapsed state of the description panel.
    pub fn toggle_description(&mut self) {
        self.expand_description = !self.expand_description;
    }

    /// Replace the library contents, keeping the current filter/query/sort and a
    /// sensible selection.
    pub fn set_items(&mut self, items: Vec<Game>) {
        self.items = items;
        self.clamp_selection();
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Whether a game passes the hidden/allowed-type rules and the active tab.
    fn passes(game: &Game, filter: Filter, config: &Config) -> bool {
        if config.hidden_games.contains(&game.id) {
            return false;
        }
        if !config.allowed_games.contains(&game.game_type) {
            return false;
        }
        match filter {
            Filter::All => true,
            Filter::Installed => game.installed,
            Filter::Updates => game.update_available,
            Filter::Favourites => config.favorite_games.contains(&game.id),
        }
    }

    /// The currently visible games: active tab + fuzzy query, sorted.
    pub fn visible(&self) -> Vec<&Game> {
        let config = Config::cached();
        let matcher = SkimMatcherV2::default();
        let mut games: Vec<&Game> = self
            .items
            .iter()
            .filter(|g| Browser::passes(g, self.filter, &config))
            .filter(|g| {
                self.query.is_empty() || matcher.fuzzy_match(g.raw_name(), &self.query).is_some()
            })
            .collect();

        match self.sort {
            Sort::Name => {
                games.sort_by(|a, b| {
                    a.raw_name()
                        .to_lowercase()
                        .cmp(&b.raw_name().to_lowercase())
                });
            }
            Sort::Installed => {
                games.sort_by(|a, b| {
                    b.installed.cmp(&a.installed).then_with(|| {
                        a.raw_name()
                            .to_lowercase()
                            .cmp(&b.raw_name().to_lowercase())
                    })
                });
            }
        }
        games
    }

    /// Whether an overlay or prompt covers the library (the detail pane is
    /// hidden, so its art and details needn't load).
    pub fn overlay_open(&self) -> bool {
        self.show_help
            || self.show_dlc
            || self.show_account
            || self.show_config
            || self.show_achievements
            || self.show_cloud
            || self.show_branches
            || self.show_depots
            || self.show_chat
            || self.show_inventory
            || self.show_launch
            || self.show_market
            || self.show_market_search
            || self.show_move
            || self.show_relink
            || self.show_import
            || self.show_proton
            || self.show_running
            || self.show_wallet
            || self.show_workshop
            || self.show_friend_add
            || self.confirm_friend_remove
            || self.confirm_uninstall
            || self.show_install_picker
            || self.confirm_install
            || self.show_actions
            || self.show_versions
            || self.show_game_config
            || self.show_launch_targets
            || self.show_collections
            || self.store_open()
    }

    /// Whether a Store overlay (`Y` or `J`) is open (never, without
    /// `store`).
    pub fn store_open(&self) -> bool {
        #[cfg(feature = "store")]
        {
            self.store.is_some() || self.shop.is_some()
        }
        #[cfg(not(feature = "store"))]
        {
            false
        }
    }

    pub fn selected(&self) -> Option<Game> {
        let visible = self.visible();
        self.state
            .selected()
            .and_then(|i| visible.get(i))
            .map(|g| (*g).clone())
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.state.selected()
    }

    pub fn visible_len(&self) -> usize {
        self.visible().len()
    }

    /// Live tallies for the status bar, over the hidden/allowed-filtered universe.
    pub fn counts(&self) -> Counts {
        let config = Config::cached();
        let universe: Vec<&Game> = self
            .items
            .iter()
            .filter(|g| Browser::passes(g, Filter::All, &config))
            .collect();
        let installed = universe.iter().filter(|g| g.installed).count();
        let updates = universe.iter().filter(|g| g.update_available).count();
        let downloading = universe
            .iter()
            .filter(|g| {
                g.get_status()
                    .map(|s| {
                        let st = s.state;
                        st.contains("downloading") || st.contains("processing")
                    })
                    .unwrap_or(false)
            })
            .count();
        Counts {
            total: universe.len(),
            visible: self.visible_len(),
            installed,
            updates,
            downloading,
            #[cfg(feature = "install")]
            checking_updates: self.checking_updates(),
            #[cfg(not(feature = "install"))]
            checking_updates: false,
        }
    }

    // --- Selection movement ---

    pub(super) fn reset_selection(&mut self) {
        let len = self.visible_len();
        self.state.select(if len == 0 { None } else { Some(0) });
    }

    /// Keep the selection within the (possibly shrunk) visible range.
    pub(super) fn clamp_selection(&mut self) {
        let len = self.visible_len();
        match self.state.selected() {
            _ if len == 0 => self.state.select(None),
            Some(i) if i >= len => self.state.select(Some(len - 1)),
            None => self.state.select(Some(0)),
            _ => {}
        }
    }

    pub fn next(&mut self) {
        let len = self.visible_len();
        if len == 0 {
            self.state.select(None);
            return;
        }
        let i = match self.state.selected() {
            Some(i) if i + 1 < len => i + 1,
            Some(_) => 0,
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn previous(&mut self) {
        let len = self.visible_len();
        if len == 0 {
            self.state.select(None);
            return;
        }
        let i = match self.state.selected() {
            Some(0) | None => len - 1,
            Some(i) => i - 1,
        };
        self.state.select(Some(i));
    }

    pub fn page_down(&mut self, page: usize) {
        let len = self.visible_len();
        if len == 0 {
            return;
        }
        let i = self.state.selected().unwrap_or(0);
        self.state.select(Some((i + page).min(len - 1)));
    }

    pub fn page_up(&mut self, page: usize) {
        let len = self.visible_len();
        if len == 0 {
            return;
        }
        let i = self.state.selected().unwrap_or(0);
        self.state.select(Some(i.saturating_sub(page)));
    }

    pub fn home(&mut self) {
        if self.visible_len() > 0 {
            self.state.select(Some(0));
        }
    }

    pub fn end(&mut self) {
        let len = self.visible_len();
        if len > 0 {
            self.state.select(Some(len - 1));
        }
    }

    /// Select a visible row by index (e.g. from a mouse click), if in range.
    pub fn select_index(&mut self, i: usize) {
        if i < self.visible_len() {
            self.state.select(Some(i));
        }
    }

    /// Re-clamp the selection after the visible set may have changed out of band
    /// (e.g. a game was hidden, or favourites toggled while on the Favourites tab).
    pub fn refresh(&mut self) {
        self.clamp_selection();
    }

    // --- Filter / sort / query mutations (all re-clamp the selection) ---

    pub fn set_filter(&mut self, filter: Filter) {
        // Selecting a library filter always returns to the Library view (so the
        // "All → Friends" transition reverses cleanly when a filter is picked).
        self.view = View::Library;
        self.filter = filter;
        self.reset_selection();
    }

    /// The active tab's index along the combined tab ring: `0..TABS.len()` for
    /// the library filters, and `TABS.len()` for the Friends tab. The tab bar
    /// renders from this so the highlight tracks the view.
    pub fn tab_index(&self) -> usize {
        match self.view {
            View::Library => self.filter.index(),
            #[cfg(feature = "social")]
            View::Friends => Filter::TABS.len(),
        }
    }

    /// Cycle through the combined tab ring (the four library filters followed by
    /// the Friends tab). Stepping right off Favourites lands on Friends (which
    /// loads the roster and focuses the panel); stepping right off Friends wraps
    /// back to All. The library filters re-clamp the selection as before.
    pub fn cycle_filter(&mut self, forward: bool) {
        // Slots: 0..TABS.len() are filters, TABS.len() is the Friends tab.
        // Without `social` there is no Friends tab.
        let slots = Filter::TABS.len() + usize::from(cfg!(feature = "social"));
        let idx = self.tab_index();
        let next = if forward {
            (idx + 1) % slots
        } else {
            (idx + slots - 1) % slots
        };
        #[cfg(feature = "social")]
        if next == Filter::TABS.len() {
            self.enter_friends();
            return;
        }
        self.set_filter(Filter::TABS[next]);
    }

    pub fn cycle_sort(&mut self) {
        self.sort = self.sort.next();
        self.clamp_selection();
    }

    pub fn push_query(&mut self, c: char) {
        self.query.push(c);
        self.reset_selection();
    }

    pub fn pop_query(&mut self) {
        self.query.pop();
        self.reset_selection();
    }

    pub fn clear_query(&mut self) {
        self.query.clear();
        self.reset_selection();
    }
}
