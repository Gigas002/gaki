//! The Steam Store overlay (`J`): the store-wide views, in sections
//! (Search, Deals, Events, Wishlist, and a friend's wishlist when it is
//! opened from the Friends panel). Rows that are games open the
//! per-game Store overlay (`Enter`), toggle the wishlist (`w`, after a
//! confirm) and open the store page (`E`). Every load runs in the
//! background (see [`super::job`]).

use libgaki::ops::store::{Listing, Wishlist};
use libgaki::steam_client::{DealsScope, StoreAppInfo, StoreEvent};

use super::Browser;
use super::job::{Load, Slot, spawn, take};
use super::links::Link;
use crate::app::steam;

/// The overlay's sections, in Tab order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShopSection {
    Search,
    Deals,
    Events,
    Wishlist,
    /// A friend's wishlist; only there when the overlay was opened for one.
    Friend,
}

impl ShopSection {
    /// The sections every overlay has, in Tab order.
    pub const ALL: [ShopSection; 4] = [
        ShopSection::Search,
        ShopSection::Deals,
        ShopSection::Events,
        ShopSection::Wishlist,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ShopSection::Search => "Search",
            ShopSection::Deals => "Deals",
            ShopSection::Events => "Events",
            ShopSection::Wishlist => "Wishlist",
            ShopSection::Friend => "Friend's wishlist",
        }
    }

    /// The section `step` places away among [`ShopSection::ALL`], wrapping.
    pub fn cycle(self, step: isize) -> ShopSection {
        self.cycle_in(&Self::ALL, step)
    }

    /// The section `step` places away among `sections`, wrapping.
    fn cycle_in(self, sections: &[ShopSection], step: isize) -> ShopSection {
        let len = sections.len() as isize;
        let index = sections.iter().position(|&s| s == self).unwrap_or(0) as isize;
        sections[(index + step).rem_euclid(len) as usize]
    }
}

/// A friend's wishlist, shown in [`ShopSection::Friend`].
pub struct FriendWishlist {
    pub steam_id: u64,
    pub name: String,
    pub load: Option<Load<Wishlist>>,
}

/// The next deal list for `s`.
pub fn next_scope(scope: DealsScope) -> DealsScope {
    match scope {
        DealsScope::DiscountedTopSellers => DealsScope::TopSellers,
        DealsScope::TopSellers => DealsScope::Specials,
        DealsScope::Specials => DealsScope::DiscountedTopSellers,
    }
}

pub fn scope_label(scope: DealsScope) -> &'static str {
    match scope {
        DealsScope::DiscountedTopSellers => "discounted top sellers",
        DealsScope::TopSellers => "top sellers",
        DealsScope::Specials => "specials",
    }
}

/// One game row: what the lists show and what the row keys act on.
#[derive(Debug, Clone, PartialEq)]
pub struct GameRow {
    pub app_id: u32,
    pub name: String,
    pub price: Option<String>,
    pub is_free: bool,
    pub discount_pct: i32,
    pub review: Option<String>,
}

impl GameRow {
    fn from_record(r: &StoreAppInfo) -> Self {
        Self {
            app_id: r.app_id,
            name: r.name.clone(),
            price: r.price.clone(),
            is_free: r.is_free,
            discount_pct: r.discount_pct,
            review: r.review_summary.clone(),
        }
    }

    /// A wishlist item the store has no record for (delisted, say).
    fn bare(app_id: u32) -> Self {
        Self {
            app_id,
            name: format!("App {app_id}"),
            price: None,
            is_free: false,
            discount_pct: 0,
            review: None,
        }
    }
}

/// A wishlist change waiting for its confirm, or running.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WishChange {
    pub app_id: u32,
    pub name: String,
    pub add: bool,
}

/// The Steam Store overlay's state.
pub struct ShopView {
    pub section: ShopSection,
    /// Highlighted row in the shown section.
    pub selected: usize,
    /// The last search run.
    pub query: String,
    /// The search prompt, when open.
    pub search_input: Option<String>,
    pub search: Option<Load<Listing>>,
    pub deals_scope: DealsScope,
    pub deals: Option<Load<Listing>>,
    pub events: Option<Load<Vec<StoreEvent>>>,
    pub wishlist: Option<Load<Wishlist>>,
    /// The friend whose wishlist the Friend section shows.
    pub friend: Option<FriendWishlist>,
    /// A wishlist change asking for y/n.
    pub confirm: Option<WishChange>,
    /// A wishlist change in flight, and its result (the wishlist's size).
    pub wish_op: Option<(WishChange, Slot<u32>)>,
    /// A one-line message (a wishlist change's result, say).
    pub status: String,
}

impl Default for ShopView {
    fn default() -> Self {
        Self {
            section: ShopSection::Search,
            selected: 0,
            query: String::new(),
            search_input: Some(String::new()),
            search: None,
            deals_scope: DealsScope::DiscountedTopSellers,
            deals: None,
            events: None,
            wishlist: None,
            friend: None,
            confirm: None,
            wish_op: None,
            status: String::new(),
        }
    }
}

fn listing_rows(load: Option<&Load<Listing>>) -> Vec<GameRow> {
    load.and_then(Load::ready)
        .map(|l| l.records.iter().map(GameRow::from_record).collect())
        .unwrap_or_default()
}

fn wishlist_rows(load: Option<&Load<Wishlist>>) -> Vec<GameRow> {
    load.and_then(Load::ready)
        .map(|w| {
            w.items
                .iter()
                .map(|(entry, record)| {
                    record
                        .as_ref()
                        .map(GameRow::from_record)
                        .unwrap_or_else(|| GameRow::bare(entry.app_id))
                })
                .collect()
        })
        .unwrap_or_default()
}

impl ShopView {
    /// The game rows of the shown section (none for Events).
    pub fn game_rows(&self) -> Vec<GameRow> {
        match self.section {
            ShopSection::Search => listing_rows(self.search.as_ref()),
            ShopSection::Deals => listing_rows(self.deals.as_ref()),
            ShopSection::Events => Vec::new(),
            ShopSection::Wishlist => wishlist_rows(self.wishlist.as_ref()),
            ShopSection::Friend => {
                wishlist_rows(self.friend.as_ref().and_then(|f| f.load.as_ref()))
            }
        }
    }

    /// The sections shown, in Tab order: the Friend section joins when
    /// there is a friend.
    pub fn sections(&self) -> Vec<ShopSection> {
        let mut sections = ShopSection::ALL.to_vec();
        if self.friend.is_some() {
            sections.push(ShopSection::Friend);
        }
        sections
    }

    /// A section's name in the title bar.
    pub fn section_label(&self, section: ShopSection) -> String {
        match (section, &self.friend) {
            (ShopSection::Friend, Some(f)) => format!("{}'s wishlist", f.name),
            _ => section.label().to_string(),
        }
    }

    /// Rows in the shown section.
    pub fn row_count(&self) -> usize {
        match self.section {
            ShopSection::Events => self
                .events
                .as_ref()
                .and_then(Load::ready)
                .map_or(0, Vec::len),
            _ => self.game_rows().len(),
        }
    }

    pub fn selected_game(&self) -> Option<GameRow> {
        self.game_rows().into_iter().nth(self.selected)
    }

    /// Whether `app_id` is on the loaded wishlist (`None`: not loaded).
    fn on_wishlist(&self, app_id: u32) -> Option<bool> {
        let wishlist = self.wishlist.as_ref()?.ready()?;
        Some(wishlist.items.iter().any(|(e, _)| e.app_id == app_id))
    }

    /// Start loading the shown section if it hasn't been yet. Search loads
    /// only once there is a query.
    fn load_section(&mut self) {
        match self.section {
            ShopSection::Search => {
                if self.search.is_none() && !self.query.is_empty() {
                    let term = self.query.clone();
                    self.search = Some(Load::start(move || steam::store_search(&term)));
                }
            }
            ShopSection::Deals if self.deals.is_none() => {
                let scope = self.deals_scope;
                self.deals = Some(Load::start(move || steam::store_deals(scope)));
            }
            ShopSection::Events if self.events.is_none() => {
                self.events = Some(Load::start(steam::store_events));
            }
            ShopSection::Wishlist if self.wishlist.is_none() => {
                self.wishlist = Some(Load::start(steam::store_wishlist));
            }
            ShopSection::Friend => {
                if let Some(friend) = &mut self.friend
                    && friend.load.is_none()
                {
                    let id = friend.steam_id;
                    friend.load = Some(Load::start(move || steam::store_wishlist_of(id)));
                }
                // Your own wishlist says whether `w` adds or removes.
                if self.wishlist.is_none() {
                    self.wishlist = Some(Load::start(steam::store_wishlist));
                }
            }
            _ => {}
        }
    }

    fn clamp(&mut self) {
        self.selected = self.selected.min(self.row_count().saturating_sub(1));
    }

    fn poll(&mut self) -> Option<String> {
        let mut changed = false;
        for load in [&mut self.search, &mut self.deals].into_iter().flatten() {
            changed |= load.poll();
        }
        if let Some(load) = &mut self.events {
            changed |= load.poll();
        }
        if let Some(load) = &mut self.wishlist {
            changed |= load.poll();
        }
        if let Some(load) = self.friend.as_mut().and_then(|f| f.load.as_mut()) {
            changed |= load.poll();
        }
        if changed {
            self.clamp();
        }

        let (change, slot) = self.wish_op.as_ref()?;
        let result = take(slot)?;
        let message = match result {
            Ok(count) => format!(
                "{} {} ({count} on your wishlist).",
                if change.add { "Added" } else { "Removed" },
                change.name
            ),
            Err(e) => format!("Couldn't change the wishlist: {e}"),
        };
        self.wish_op = None;
        // The loaded wishlist is stale now; reload it when it is shown.
        self.wishlist = None;
        self.load_section();
        self.clamp();
        self.status = message.clone();
        Some(message)
    }
}

impl Browser {
    /// Open the Steam Store overlay on its Search section, ready to type.
    pub fn open_shop(&mut self) {
        self.shop = Some(ShopView::default());
    }

    pub fn close_shop(&mut self) {
        self.shop = None;
    }

    /// Open the Steam Store overlay on `name`'s wishlist (a friend's:
    /// Steam serves no one else's).
    pub fn open_shop_wishlist_of(&mut self, steam_id: u64, name: String) {
        let mut view = ShopView {
            section: ShopSection::Friend,
            search_input: None,
            friend: Some(FriendWishlist {
                steam_id,
                name,
                load: None,
            }),
            ..ShopView::default()
        };
        view.load_section();
        self.shop = Some(view);
    }

    /// `w` in the Friends panel: the highlighted friend's wishlist.
    #[cfg(feature = "social")]
    pub fn open_friend_wishlist(&mut self) {
        let Some(friend) = self.selected_friend() else {
            return;
        };
        let (id, name) = (friend.steam_id, friend.display_name());
        self.open_shop_wishlist_of(id, name);
    }

    /// Show the section `step` places away.
    pub fn shop_cycle(&mut self, step: isize) {
        if let Some(view) = &mut self.shop {
            view.section = view.section.cycle_in(&view.sections(), step);
            view.selected = 0;
            view.status.clear();
            view.load_section();
        }
    }

    /// Move the highlight by `delta` rows (clamped).
    pub fn shop_move(&mut self, delta: isize) {
        if let Some(view) = &mut self.shop {
            let max = view.row_count().saturating_sub(1);
            view.selected = view.selected.saturating_add_signed(delta).min(max);
        }
    }

    /// Search prompt (`/`), prefilled with the last query; Search only.
    pub fn begin_shop_search(&mut self) {
        if let Some(view) = &mut self.shop
            && view.section == ShopSection::Search
        {
            view.search_input = Some(view.query.clone());
        }
    }

    pub fn shop_input_open(&self) -> bool {
        self.shop.as_ref().is_some_and(|v| v.search_input.is_some())
    }

    pub fn shop_input_push(&mut self, c: char) {
        if let Some(text) = self.shop.as_mut().and_then(|v| v.search_input.as_mut()) {
            text.push(c);
        }
    }

    pub fn shop_input_pop(&mut self) {
        if let Some(text) = self.shop.as_mut().and_then(|v| v.search_input.as_mut()) {
            text.pop();
        }
    }

    pub fn cancel_shop_input(&mut self) {
        if let Some(view) = &mut self.shop {
            view.search_input = None;
        }
    }

    /// Run the typed search (an empty one just closes the prompt).
    pub fn submit_shop_input(&mut self) {
        let Some(view) = &mut self.shop else {
            return;
        };
        let Some(text) = view.search_input.take() else {
            return;
        };
        let term = text.trim();
        if term.is_empty() {
            return;
        }
        view.query = term.to_string();
        view.search = None;
        view.selected = 0;
        view.load_section();
    }

    /// Next deal list (`s`, Deals only).
    pub fn shop_next_deals(&mut self) {
        if let Some(view) = &mut self.shop
            && view.section == ShopSection::Deals
        {
            view.deals_scope = next_scope(view.deals_scope);
            view.deals = None;
            view.selected = 0;
            view.load_section();
        }
    }

    /// Reload the shown section (`r`).
    pub fn shop_reload(&mut self) {
        if let Some(view) = &mut self.shop {
            match view.section {
                ShopSection::Search => view.search = None,
                ShopSection::Deals => view.deals = None,
                ShopSection::Events => view.events = None,
                ShopSection::Wishlist => view.wishlist = None,
                ShopSection::Friend => {
                    if let Some(friend) = &mut view.friend {
                        friend.load = None;
                    }
                }
            }
            view.status.clear();
            view.load_section();
        }
    }

    /// `Enter`: the highlighted game's per-game Store overlay, on top.
    pub fn shop_open_game(&mut self) {
        let Some(row) = self.shop.as_ref().and_then(ShopView::selected_game) else {
            return;
        };
        self.open_store(row.app_id as i32, &row.name);
    }

    /// `E`: the highlighted game's store page in the browser.
    pub fn shop_open_page(&mut self) {
        let Some(row) = self.shop.as_ref().and_then(ShopView::selected_game) else {
            return;
        };
        self.open_link(Link::Steam, row.app_id as i32);
    }

    /// `w`: ask to add the highlighted game to the wishlist, or to remove
    /// it when it is already on it (always, in the Wishlist section).
    pub fn begin_shop_wish(&mut self) {
        let Some(view) = &mut self.shop else {
            return;
        };
        if view.wish_op.is_some() {
            view.status = "A wishlist change is already running.".to_string();
            return;
        }
        let Some(row) = view.selected_game() else {
            return;
        };
        let on_list =
            view.section == ShopSection::Wishlist || view.on_wishlist(row.app_id) == Some(true);
        view.confirm = Some(WishChange {
            app_id: row.app_id,
            name: row.name,
            add: !on_list,
        });
    }

    pub fn shop_confirm_open(&self) -> bool {
        self.shop.as_ref().is_some_and(|v| v.confirm.is_some())
    }

    /// Answer the wishlist confirm: `yes` starts the change.
    pub fn answer_shop_confirm(&mut self, yes: bool) {
        let Some(view) = &mut self.shop else {
            return;
        };
        let Some(change) = view.confirm.take() else {
            return;
        };
        if !yes {
            return;
        }
        let (app_id, add) = (change.app_id, change.add);
        view.status = format!(
            "{} {}…",
            if add { "Adding" } else { "Removing" },
            change.name
        );
        let slot = spawn(move || steam::store_wishlist_set(app_id, add));
        view.wish_op = Some((change, slot));
    }

    /// Adopt finished loads and wishlist changes; run on every tick.
    pub fn poll_shop(&mut self) {
        let message = self.shop.as_mut().and_then(ShopView::poll);
        if let Some(message) = message {
            self.notice = Some(message);
        }
    }

    /// Whether the Steam Store overlay is open.
    pub fn shop_open(&self) -> bool {
        self.shop.is_some()
    }
}

#[cfg(test)]
mod tests;
