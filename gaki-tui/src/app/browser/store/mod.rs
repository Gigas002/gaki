//! The Store overlay (`Y`): the selected game's store data in sections.
//! Each section loads in the background the first time it is shown, and
//! keeps what it loaded until the overlay closes.

use libgaki::ops::store::{Listing, Prices};
use libgaki::web::discovery::{NewsItem, ReviewPage};

use super::Browser;
use super::job::Load;
use crate::app::steam;

/// The overlay's sections, in Tab order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Price,
    Players,
    News,
    Reviews,
    Similar,
}

impl Section {
    pub const ALL: [Section; 5] = [
        Section::Price,
        Section::Players,
        Section::News,
        Section::Reviews,
        Section::Similar,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Section::Price => "Price",
            Section::Players => "Players",
            Section::News => "News",
            Section::Reviews => "Reviews",
            Section::Similar => "Similar",
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|&s| s == self).unwrap_or(0)
    }

    /// The section `step` places away, wrapping.
    pub fn cycle(self, step: isize) -> Section {
        let len = Self::ALL.len() as isize;
        let next = (self.index() as isize + step).rem_euclid(len);
        Self::ALL[next as usize]
    }
}

/// The Store overlay's state.
pub struct StoreView {
    pub app_id: i32,
    pub name: String,
    pub section: Section,
    /// Top line of the section body.
    pub scroll: u16,
    /// Width the body was last drawn at, so scrolling can clamp to its
    /// wrapped height. Set by the renderer.
    pub body_width: std::cell::Cell<u16>,
    /// Regions the price section compares (country codes). Starts from
    /// `store_regions` in `tui/config.toml`; empty means the account's own.
    pub regions: Vec<String>,
    /// The regions prompt (`r` in Price), when open.
    pub regions_input: Option<String>,
    pub price: Option<Load<Prices>>,
    pub players: Option<Load<i32>>,
    pub news: Option<Load<Vec<NewsItem>>>,
    pub reviews: Option<Load<ReviewPage>>,
    pub similar: Option<Load<Listing>>,
}

impl StoreView {
    pub fn new(app_id: i32, name: String, regions: Vec<String>) -> Self {
        Self {
            app_id,
            name,
            section: Section::Price,
            scroll: 0,
            body_width: std::cell::Cell::new(80),
            regions,
            regions_input: None,
            price: None,
            players: None,
            news: None,
            reviews: None,
            similar: None,
        }
    }

    /// Start loading the shown section if it hasn't been yet.
    fn load_section(&mut self) {
        let app_id = self.app_id;
        match self.section {
            Section::Price if self.price.is_none() => {
                let regions = self.regions.clone();
                self.price = Some(Load::start(move || steam::store_prices(app_id, &regions)));
            }
            Section::Players if self.players.is_none() => {
                self.players = Some(Load::start(move || steam::store_players(app_id)));
            }
            Section::News if self.news.is_none() => {
                self.news = Some(Load::start(move || steam::store_news(app_id)));
            }
            Section::Reviews if self.reviews.is_none() => {
                self.reviews = Some(Load::start(move || steam::store_reviews(app_id)));
            }
            Section::Similar if self.similar.is_none() => {
                self.similar = Some(Load::start(move || steam::store_similar(app_id)));
            }
            _ => {}
        }
    }

    /// Adopt finished loads.
    fn poll(&mut self) {
        if let Some(load) = &mut self.price {
            load.poll();
        }
        if let Some(load) = &mut self.players {
            load.poll();
        }
        if let Some(load) = &mut self.news {
            load.poll();
        }
        if let Some(load) = &mut self.reviews {
            load.poll();
        }
        if let Some(load) = &mut self.similar {
            load.poll();
        }
    }
}

/// Region codes typed as `us, de jp` (commas or spaces), upper-cased.
/// Validation happens when the prices load.
pub(crate) fn split_regions(text: &str) -> Vec<String> {
    text.split([',', ' '])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_uppercase)
        .collect()
}

impl Browser {
    /// Open the Store overlay for a game; its first section starts loading.
    pub fn open_store(&mut self, app_id: i32, name: &str) {
        let regions = crate::config::Config::cached().store_regions;
        let mut view = StoreView::new(app_id, name.to_string(), regions);
        view.load_section();
        self.store = Some(view);
    }

    pub fn close_store(&mut self) {
        self.store = None;
    }

    /// Show the section `step` places away (Tab / Shift-Tab).
    pub fn store_cycle(&mut self, step: isize) {
        if let Some(view) = &mut self.store {
            view.section = view.section.cycle(step);
            view.scroll = 0;
            view.load_section();
        }
    }

    /// Scroll the section body by `delta` lines, clamped to its lines.
    pub fn store_scroll(&mut self, delta: i16) {
        if let Some(view) = &mut self.store {
            let max = crate::ui::store::body_height(view).saturating_sub(1) as u16;
            view.scroll = view.scroll.saturating_add_signed(delta).min(max);
        }
    }

    /// Open the regions prompt (Price only), prefilled with the current list.
    pub fn begin_store_regions(&mut self) {
        if let Some(view) = &mut self.store
            && view.section == Section::Price
        {
            view.regions_input = Some(view.regions.join(","));
        }
    }

    pub fn store_input_open(&self) -> bool {
        self.store
            .as_ref()
            .is_some_and(|v| v.regions_input.is_some())
    }

    pub fn store_input_push(&mut self, c: char) {
        if let Some(text) = self.store.as_mut().and_then(|v| v.regions_input.as_mut()) {
            text.push(c);
        }
    }

    pub fn store_input_pop(&mut self) {
        if let Some(text) = self.store.as_mut().and_then(|v| v.regions_input.as_mut()) {
            text.pop();
        }
    }

    pub fn cancel_store_input(&mut self) {
        if let Some(view) = &mut self.store {
            view.regions_input = None;
        }
    }

    /// Use the typed regions for this session and reload the prices.
    pub fn submit_store_input(&mut self) {
        let Some(view) = &mut self.store else {
            return;
        };
        let Some(text) = view.regions_input.take() else {
            return;
        };
        let regions = split_regions(&text);
        if regions != view.regions || view.price.as_ref().is_some_and(|p| p.error().is_some()) {
            view.regions = regions;
            view.price = None;
            view.scroll = 0;
            view.load_section();
        }
    }

    /// Adopt finished store loads; run on every tick.
    pub fn poll_store(&mut self) {
        if let Some(view) = &mut self.store {
            view.poll();
        }
    }
}

#[cfg(test)]
mod tests;
