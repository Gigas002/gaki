//! The market listings overlay and the Community Market search.

use std::sync::Arc;
use std::thread;

use super::Browser;
use crate::app::steam;

impl Browser {
    /// Fetch the account's market listings (blocking) and open the overlay. A
    /// fetch error simply opens an empty overlay ("No active listings.").
    pub fn open_market(&mut self) {
        self.market = steam::market_listings().unwrap_or_default();
        self.market_scroll = 0;
        self.show_market = true;
    }

    /// Close the market overlay and drop its data.
    pub fn close_market(&mut self) {
        self.show_market = false;
        self.market = Vec::new();
        self.market_scroll = 0;
    }

    /// Scroll the market overlay down by one row (clamped).
    pub fn market_scroll_down(&mut self) {
        let max = self.market.len().saturating_sub(1);
        if self.market_scroll < max {
            self.market_scroll += 1;
        }
    }

    /// Scroll the market overlay up by one row (clamped).
    pub fn market_scroll_up(&mut self) {
        self.market_scroll = self.market_scroll.saturating_sub(1);
    }

    // --- Community Market search overlay ---

    /// Open the Community Market search overlay, clearing any previous query and
    /// results. No backend call happens until the user submits a query.
    pub fn open_market_search(&mut self) {
        self.show_market_search = true;
        self.market_query.clear();
        self.market_results.clear();
        self.market_results_index = 0;
        self.market_results_scroll = 0;
        self.market_search_status.clear();
        // A fresh overlay invalidates any in-flight search; bump the generation so
        // a late worker's tagged result is dropped by `poll_market`.
        self.market_search_gen = self.market_search_gen.wrapping_add(1);
        if let Ok(mut slot) = self.market_search_cell.lock() {
            *slot = (self.market_search_gen, steam::MarketSearchState::Idle);
        }
        if let Ok(mut slot) = self.market_price_cell.lock() {
            *slot = steam::MarketPriceState::Idle;
        }
    }

    /// Close the market search overlay and drop its state.
    pub fn close_market_search(&mut self) {
        self.show_market_search = false;
        self.market_query.clear();
        self.market_results.clear();
        self.market_results_index = 0;
        self.market_results_scroll = 0;
        self.market_search_status.clear();
        // Invalidate any in-flight search so a late worker can't publish into the
        // next time the overlay is opened.
        self.market_search_gen = self.market_search_gen.wrapping_add(1);
    }

    /// Append a typed character to the search query.
    pub fn market_query_push(&mut self, c: char) {
        self.market_query.push(c);
        // A fresh edit invalidates any in-flight search for the prior query; bump
        // the generation so its tagged result is dropped by `poll_market`.
        self.market_search_gen = self.market_search_gen.wrapping_add(1);
    }

    /// Remove the last character from the search query.
    pub fn market_query_pop(&mut self) {
        self.market_query.pop();
        self.market_search_gen = self.market_search_gen.wrapping_add(1);
    }

    /// Kick off a Community Market search for the typed query, off the UI thread.
    /// The worker publishes into the shared cell; [`Self::poll_market`] adopts it. A
    /// no-op when the query is blank.
    pub fn submit_market_search(&mut self) {
        let query = self.market_query.trim().to_string();
        if query.is_empty() {
            return;
        }
        self.market_search_status = "searching...".to_string();
        // Tag the worker with the generation it was launched for; `poll_market`
        // only adopts a result whose tag still matches the current generation, so
        // an out-of-order OLDER search can't clobber a NEWER one's results.
        let search_gen = self.market_search_gen;
        if let Ok(mut slot) = self.market_search_cell.lock() {
            *slot = (search_gen, steam::MarketSearchState::Loading);
        }
        let cell = Arc::clone(&self.market_search_cell);
        thread::spawn(move || steam::market_search_async(query, search_gen, cell));
    }

    /// Look up the highlighted result's market price, off the UI thread. The
    /// worker publishes into the shared price cell; [`Self::poll_market`] adopts it. A
    /// no-op when no result is highlighted.
    pub fn lookup_market_price(&mut self) {
        let Some(result) = self.selected_market_result() else {
            return;
        };
        let app_id = result.app_id;
        let name = result.market_hash_name.clone();
        if name.is_empty() {
            self.market_search_status = "no item name to price".to_string();
            return;
        }
        self.market_search_status = format!("pricing {}...", result.display_name());
        if let Ok(mut slot) = self.market_price_cell.lock() {
            *slot = steam::MarketPriceState::Loading;
        }
        let cell = Arc::clone(&self.market_price_cell);
        thread::spawn(move || steam::market_price_async(app_id, name, cell));
    }

    /// Adopt any completed search/price result published by the worker threads,
    /// updating the results list and status line. Called once per frame from the
    /// event loop; cheap and non-blocking (no disk/CLI access on the UI thread).
    pub fn poll_market(&mut self) {
        if !self.show_market_search {
            return;
        }

        // Adopt a finished search, but only if it was launched for the current
        // query generation. A result tagged with a stale generation (the query
        // was edited or the overlay toggled after the worker spawned) is dropped,
        // closing the out-of-order clobber where a slow OLDER search could
        // overwrite a NEWER one's results.
        let search = self.market_search_cell.lock().ok().and_then(|mut slot| {
            let search_gen = slot.0;
            let done = !matches!(
                &slot.1,
                steam::MarketSearchState::Idle | steam::MarketSearchState::Loading
            );
            if done && search_gen == self.market_search_gen {
                let (_, state) = std::mem::replace(
                    &mut *slot,
                    (self.market_search_gen, steam::MarketSearchState::Idle),
                );
                Some(state)
            } else if done {
                // Stale result: discard it (reset to Idle) without adopting.
                slot.1 = steam::MarketSearchState::Idle;
                None
            } else {
                None
            }
        });
        match search {
            Some(steam::MarketSearchState::Ready(results)) => {
                let count = results.len();
                self.market_results = results;
                self.market_results_index = 0;
                self.market_results_scroll = 0;
                self.market_search_status = if count == 0 {
                    "no results".to_string()
                } else {
                    format!("{} results", count)
                };
            }
            Some(steam::MarketSearchState::Failed(err)) => {
                self.market_results.clear();
                self.market_results_index = 0;
                self.market_results_scroll = 0;
                self.market_search_status = format!("Failed: {}", err);
            }
            _ => {}
        }

        // Adopt a finished price lookup (does not disturb the results list).
        let price = self.market_price_cell.lock().ok().map(|mut slot| {
            let done = !matches!(
                &*slot,
                steam::MarketPriceState::Idle | steam::MarketPriceState::Loading
            );
            if done {
                std::mem::replace(&mut *slot, steam::MarketPriceState::Idle)
            } else {
                steam::MarketPriceState::Loading
            }
        });
        match price {
            Some(steam::MarketPriceState::Ready(p)) => {
                self.market_search_status = match p.summary() {
                    Some(s) => format!("{} — {}", p.market_hash_name, s),
                    None => format!("{} — no price data", p.market_hash_name),
                };
            }
            Some(steam::MarketPriceState::Failed(err)) => {
                self.market_search_status = format!("Price failed: {}", err);
            }
            _ => {}
        }
    }

    /// The highlighted search result, if any.
    pub fn selected_market_result(&self) -> Option<&steam::MarketSearchResultJson> {
        self.market_results.get(self.market_results_index)
    }

    /// Move the search-results highlight down by one row (clamped). The widget
    /// derives its scroll window from the index so the highlight stays visible.
    pub fn market_result_next(&mut self) {
        if self.market_results.is_empty() {
            self.market_results_index = 0;
            return;
        }
        if self.market_results_index + 1 < self.market_results.len() {
            self.market_results_index += 1;
        }
    }

    /// Move the search-results highlight up by one row (clamped).
    pub fn market_result_previous(&mut self) {
        self.market_results_index = self.market_results_index.saturating_sub(1);
    }
}
