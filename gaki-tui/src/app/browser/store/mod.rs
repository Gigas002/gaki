//! The Store overlay (`Y`): the selected game's store data in sections.
//! Each section loads in the background the first time it is shown, and
//! keeps what it loaded until the overlay closes. Reviews also shows your
//! own review, and `w` there opens the composer to write or change it.

use libgaki::ops::store::{Listing, OwnReview, Prices, ReviewDraft, ReviewWritten};
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
    /// Your review, loaded with the Reviews section.
    pub mine: Option<Load<Option<OwnReview>>>,
    /// The review composer, when open.
    pub composer: Option<ReviewComposer>,
    /// A review being posted.
    pub posting: Option<Load<ReviewWritten>>,
    /// The last review action's result.
    pub review_status: String,
    pub similar: Option<Load<Listing>>,
}

/// The review composer: the draft, and whether it waits for the y/n.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewComposer {
    pub draft: ReviewDraft,
    /// It replaces your review.
    pub replaces: bool,
    pub confirming: bool,
}

/// A composer toggle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewToggle {
    Verdict,
    Visibility,
    Free,
    Comments,
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
            mine: None,
            composer: None,
            posting: None,
            review_status: String::new(),
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
            Section::Reviews => {
                if self.reviews.is_none() {
                    self.reviews = Some(Load::start(move || steam::store_reviews(app_id)));
                }
                if self.mine.is_none() {
                    self.mine = Some(Load::start(move || steam::store_my_review(app_id)));
                }
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
        if let Some(load) = &mut self.mine {
            load.poll();
        }
        if let Some(load) = &mut self.posting
            && load.poll()
        {
            match self.posting.take() {
                Some(Load::Ready(written)) => {
                    self.review_status = match written {
                        ReviewWritten::Created => "Posted your review.",
                        ReviewWritten::Updated => "Updated your review.",
                    }
                    .into();
                    self.composer = None;
                    self.scroll = 0;
                    // Show it as Steam has it now.
                    self.mine = None;
                    self.load_section();
                }
                Some(Load::Failed(e)) => {
                    self.review_status = format!("Not posted: {e}");
                    // Back to the draft, so nothing typed is lost.
                    if let Some(composer) = &mut self.composer {
                        composer.confirming = false;
                    }
                }
                _ => {}
            }
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

    /// `w` in Reviews: open the composer on your review, or on a new one
    /// (recommended, public).
    pub fn begin_review(&mut self) {
        let Some(view) = &mut self.store else {
            return;
        };
        if view.section != Section::Reviews || view.posting.is_some() {
            return;
        }
        let (draft, replaces) = match &view.mine {
            Some(Load::Ready(Some(review))) => (review.draft(), true),
            Some(Load::Ready(None) | Load::Failed(_)) => (
                ReviewDraft {
                    text: String::new(),
                    voted_up: true,
                    is_public: true,
                    language: None,
                    received_compensation: false,
                    comments_disabled: false,
                },
                false,
            ),
            _ => {
                view.review_status = "Still loading your review…".into();
                return;
            }
        };
        view.review_status.clear();
        view.scroll = 0;
        view.composer = Some(ReviewComposer {
            draft,
            replaces,
            confirming: false,
        });
    }

    pub fn review_composer_open(&self) -> bool {
        self.store.as_ref().is_some_and(|v| v.composer.is_some())
    }

    pub fn review_confirming(&self) -> bool {
        self.store
            .as_ref()
            .and_then(|v| v.composer.as_ref())
            .is_some_and(|c| c.confirming)
    }

    fn review_draft(&mut self) -> Option<&mut ReviewDraft> {
        let view = self.store.as_mut()?;
        if view.posting.is_some() {
            return None;
        }
        let composer = view.composer.as_mut()?;
        (!composer.confirming).then_some(&mut composer.draft)
    }

    /// Typed text; `\n` is a new line.
    pub fn review_push(&mut self, c: char) {
        if let Some(draft) = self.review_draft() {
            draft.text.push(c);
        }
    }

    pub fn review_pop(&mut self) {
        if let Some(draft) = self.review_draft() {
            draft.text.pop();
        }
    }

    pub fn review_toggle(&mut self, toggle: ReviewToggle) {
        if let Some(draft) = self.review_draft() {
            let flag = match toggle {
                ReviewToggle::Verdict => &mut draft.voted_up,
                ReviewToggle::Visibility => &mut draft.is_public,
                ReviewToggle::Free => &mut draft.received_compensation,
                ReviewToggle::Comments => &mut draft.comments_disabled,
            };
            *flag = !*flag;
        }
    }

    /// Esc: close the composer (the draft is dropped), or leave the y/n.
    pub fn cancel_review(&mut self) {
        if let Some(view) = &mut self.store {
            if view.posting.is_some() {
                return;
            }
            match &mut view.composer {
                Some(composer) if composer.confirming => composer.confirming = false,
                _ => {
                    view.composer = None;
                    view.scroll = 0;
                }
            }
        }
    }

    /// `ctrl+s`: ask before posting a draft Steam would take.
    pub fn submit_review(&mut self) {
        let Some(view) = &mut self.store else {
            return;
        };
        let Some(composer) = &mut view.composer else {
            return;
        };
        if view.posting.is_some() {
            return;
        }
        match composer.draft.check() {
            Some(problem) => view.review_status = format!("Can't post: {problem}."),
            None => {
                view.review_status.clear();
                composer.confirming = true;
            }
        }
    }

    /// Answer the post confirm: `yes` posts in the background.
    pub fn answer_review(&mut self, yes: bool) {
        let Some(view) = &mut self.store else {
            return;
        };
        let Some(composer) = &mut view.composer else {
            return;
        };
        if !composer.confirming {
            return;
        }
        if !yes {
            composer.confirming = false;
            return;
        }
        let app_id = view.app_id;
        let draft = composer.draft.clone();
        view.review_status = "Posting…".into();
        view.posting = Some(Load::start(move || {
            steam::store_write_review(app_id, draft)
        }));
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
