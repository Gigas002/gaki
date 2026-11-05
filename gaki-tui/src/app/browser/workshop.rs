//! The Workshop overlay: subscribed items, browse and search, ratings, comments.

use std::sync::Arc;
use std::thread;

use super::Browser;
use crate::app::steam;

impl Browser {
    /// Fetch the given game's subscribed Workshop items (blocking) and open the
    /// overlay in its subscribed-list mode. A fetch error simply opens an empty
    /// overlay ("No workshop items.").
    pub fn open_workshop(&mut self, app_id: i32) {
        self.workshop = steam::workshop_list(app_id).unwrap_or_default();
        self.workshop_scroll = 0;
        self.workshop_app_id = app_id;
        self.workshop_browse = false;
        self.workshop_query.clear();
        self.workshop_results = Vec::new();
        self.workshop_index = 0;
        self.workshop_searching = false;
        self.workshop_status.clear();
        self.workshop_rating = false;
        self.close_workshop_comments();
        self.show_workshop = true;
    }

    /// Close the Workshop overlay and drop its data.
    pub fn close_workshop(&mut self) {
        self.show_workshop = false;
        self.workshop = Vec::new();
        self.workshop_scroll = 0;
        self.workshop_browse = false;
        self.workshop_query.clear();
        self.workshop_results = Vec::new();
        self.workshop_index = 0;
        self.workshop_searching = false;
        self.workshop_status.clear();
        self.workshop_rating = false;
        self.close_workshop_comments();
        // Bump the generation so any in-flight worker's late result is dropped.
        self.workshop_gen = self.workshop_gen.wrapping_add(1);
    }

    /// Scroll the Workshop overlay down by one row (clamped).
    pub fn workshop_scroll_down(&mut self) {
        let max = self.workshop.len().saturating_sub(1);
        if self.workshop_scroll < max {
            self.workshop_scroll += 1;
        }
    }

    /// Scroll the Workshop overlay up by one row (clamped).
    pub fn workshop_scroll_up(&mut self) {
        self.workshop_scroll = self.workshop_scroll.saturating_sub(1);
    }

    /// Switch the Workshop overlay into browse/search mode and kick off an
    /// initial (default feed) browse for the current game.
    pub fn workshop_enter_browse(&mut self) {
        if self.workshop_browse {
            return;
        }
        self.workshop_browse = true;
        self.workshop_status.clear();
        self.workshop_index = 0;
        self.start_workshop_search();
    }

    /// Leave browse mode and return to the subscribed-items list.
    pub fn workshop_exit_browse(&mut self) {
        self.workshop_browse = false;
        self.workshop_status.clear();
        // Drop any in-flight search result.
        self.workshop_gen = self.workshop_gen.wrapping_add(1);
        self.workshop_searching = false;
        self.workshop_rating = false;
        self.close_workshop_comments();
    }

    /// Append a character to the browse query (browse mode only).
    pub fn workshop_push_query(&mut self, c: char) {
        self.workshop_query.push(c);
    }

    /// Delete the last character of the browse query (browse mode only).
    pub fn workshop_pop_query(&mut self) {
        self.workshop_query.pop();
    }

    /// Move the highlight down within the browse results (clamped).
    pub fn workshop_next(&mut self) {
        let max = self.workshop_results.len().saturating_sub(1);
        if self.workshop_index < max {
            self.workshop_index += 1;
        }
    }

    /// Move the highlight up within the browse results (clamped).
    pub fn workshop_previous(&mut self) {
        self.workshop_index = self.workshop_index.saturating_sub(1);
    }

    /// The currently highlighted browse result, if any.
    pub fn selected_workshop_result(&self) -> Option<&steam::WorkshopItem> {
        self.workshop_results.get(self.workshop_index)
    }

    /// Launch a `workshop browse` request off the UI thread for the current
    /// query. A monotonic generation tag is captured up front; when the worker
    /// finishes it posts `(generation, result)` into the shared slot, and
    /// `poll_workshop` only applies it if the tag still matches — so a slow
    /// earlier search can never overwrite a newer one.
    pub fn start_workshop_search(&mut self) {
        self.workshop_gen = self.workshop_gen.wrapping_add(1);
        let generation = self.workshop_gen;
        let app_id = self.workshop_app_id;
        let query = self.workshop_query.clone();
        let slot = Arc::clone(&self.workshop_slot);
        self.workshop_searching = true;
        self.workshop_status.clear();
        thread::spawn(move || {
            let result = steam::workshop_browse(app_id, &query).map_err(|e| e.to_string());
            if let Ok(mut guard) = slot.lock() {
                *guard = Some((generation, result));
            }
        });
    }

    /// Drain a finished browse worker's result into the overlay state. Called
    /// once per event-loop iteration; cheap and non-blocking when idle. Stale
    /// results (generation mismatch) are discarded.
    pub fn poll_workshop(&mut self) {
        let posted = match self.workshop_slot.lock() {
            Ok(mut guard) => guard.take(),
            Err(_) => None,
        };
        if let Some((generation, result)) = posted {
            if generation != self.workshop_gen {
                return; // a newer search superseded this one
            }
            self.workshop_searching = false;
            match result {
                Ok(items) => {
                    self.workshop_results = items;
                    self.workshop_index = 0;
                    if self.workshop_results.is_empty() {
                        self.workshop_status = "No matches.".to_string();
                    } else {
                        self.workshop_status.clear();
                    }
                }
                Err(err) => {
                    self.workshop_results = Vec::new();
                    self.workshop_index = 0;
                    self.workshop_status = format!("Browse failed: {err}");
                }
            }
        }

        // Drain a finished subscribe/unsubscribe worker.
        let action = match self.workshop_action_slot.lock() {
            Ok(mut guard) => guard.take(),
            Err(_) => None,
        };
        if let Some((item_id, want_subscribed, result)) = action {
            self.workshop_acting = false;
            match result {
                Ok(()) => {
                    // Flip the flag on whichever result row carries this id (the
                    // highlight may have moved while the request was in flight).
                    for item in self.workshop_results.iter_mut() {
                        if item.id == Some(item_id) {
                            item.subscribed = want_subscribed;
                        }
                    }
                    self.workshop_status = if want_subscribed {
                        "Subscribed.".to_string()
                    } else {
                        "Unsubscribed.".to_string()
                    };
                    // The refreshed subscribed list (if any) arrives separately
                    // via `workshop_refresh_slot`, drained below — no blocking
                    // network call here on the render thread.
                }
                Err(err) => {
                    self.workshop_status = if want_subscribed {
                        format!("Subscribe failed: {err}")
                    } else {
                        format!("Unsubscribe failed: {err}")
                    };
                }
            }
        }

        // Drain a re-fetched subscribed list posted by a successful action
        // worker. Non-blocking try-lock + take; install only if the generation
        // still matches so a refresh that arrives after the overlay closed or
        // moved on is discarded.
        let refresh = match self.workshop_refresh_slot.lock() {
            Ok(mut guard) => guard.take(),
            Err(_) => None,
        };
        if let Some((generation, items)) = refresh
            && generation == self.workshop_gen
        {
            self.workshop = items;
            let max = self.workshop.len().saturating_sub(1);
            if self.workshop_scroll > max {
                self.workshop_scroll = max;
            }
        }

        // Drain a finished rate worker. The generation is checked so a late
        // result for a since-closed/superseded overlay is dropped silently.
        let rated = match self.workshop_rate_slot.lock() {
            Ok(mut guard) => guard.take(),
            Err(_) => None,
        };
        if let Some((generation, result)) = rated {
            // The in-flight rate is definitively finished, so always clear the
            // re-press guard — otherwise bumping the generation while a rate is
            // in flight (a new search, or closing the comments sub-pane) would
            // strand `workshop_rating = true` and wedge rating forever. Only the
            // user-visible status line is gated on the generation still matching.
            self.workshop_rating = false;
            if generation == self.workshop_gen {
                self.workshop_status = match result {
                    Ok(()) => "Rated.".to_string(),
                    Err(err) => format!("Rate failed: {err}"),
                };
            }
        }

        // Drain a finished comments fetch. Apply only when the generation still
        // matches (the sub-pane was not closed / superseded since the fetch
        // started) and the sub-pane is still open.
        let comments = match self.workshop_comments_slot.lock() {
            Ok(mut guard) => guard.take(),
            Err(_) => None,
        };
        if let Some((generation, result)) = comments
            && generation == self.workshop_gen
            && self.workshop_comments_open
        {
            self.workshop_comments_loading = false;
            match result {
                Ok(items) => {
                    self.workshop_comments = items;
                    self.workshop_comments_scroll = 0;
                    self.workshop_comments_status = if self.workshop_comments.is_empty() {
                        "No comments.".to_string()
                    } else {
                        String::new()
                    };
                }
                Err(err) => {
                    self.workshop_comments = Vec::new();
                    self.workshop_comments_scroll = 0;
                    self.workshop_comments_status = format!("Comments failed: {err}");
                }
            }
        }
    }

    /// Toggle the subscription of the highlighted browse result off the UI
    /// thread: subscribe if it is not currently subscribed, otherwise
    /// unsubscribe. The worker posts `(item_id, want_subscribed, result)` into
    /// `workshop_action_slot`, which `poll_workshop` drains to flip the local
    /// flag and refresh the subscribed list. A request already in flight is a
    /// no-op so a held key cannot stack calls.
    pub fn workshop_toggle_subscribe_selected(&mut self) {
        if self.workshop_acting {
            return;
        }
        let Some(item) = self.selected_workshop_result() else {
            return;
        };
        let Some(id) = item.id else {
            return;
        };
        let want_subscribed = !item.subscribed;
        let slot = Arc::clone(&self.workshop_action_slot);
        let refresh_slot = Arc::clone(&self.workshop_refresh_slot);
        let app_id = self.workshop_app_id;
        let generation = self.workshop_gen;
        self.workshop_acting = true;
        self.workshop_status = if want_subscribed {
            "Subscribing…".to_string()
        } else {
            "Unsubscribing…".to_string()
        };
        thread::spawn(move || {
            let result = if want_subscribed {
                steam::workshop_subscribe(id)
            } else {
                steam::workshop_unsubscribe(id)
            }
            .map_err(|e| e.to_string());
            // On success, re-fetch the subscribed list *here on the worker
            // thread* (another blocking shell-out) and hand it back tagged with
            // the generation captured up front, so the UI thread never blocks on
            // `workshop list` and a stale refresh is dropped on arrival.
            if result.is_ok()
                && let Ok(items) = steam::workshop_list(app_id)
                && let Ok(mut guard) = refresh_slot.lock()
            {
                *guard = Some((generation, items));
            }
            if let Ok(mut guard) = slot.lock() {
                *guard = Some((id, want_subscribed, result));
            }
        });
    }

    /// Rate the highlighted browse result thumbs-up (`up = true`) or thumbs-down
    /// off the UI thread. The worker posts `(generation, result)` into
    /// `workshop_rate_slot`, which `poll_workshop` drains to update the status
    /// line. A request already in flight is a no-op so a held key cannot stack
    /// calls; a missing selection / id is a no-op.
    pub fn workshop_rate_selected(&mut self, up: bool) {
        if self.workshop_rating {
            return;
        }
        let Some(item) = self.selected_workshop_result() else {
            return;
        };
        let Some(id) = item.id else {
            return;
        };
        let slot = Arc::clone(&self.workshop_rate_slot);
        let generation = self.workshop_gen;
        self.workshop_rating = true;
        self.workshop_status = if up {
            "Rating up…".to_string()
        } else {
            "Rating down…".to_string()
        };
        thread::spawn(move || {
            let result = steam::workshop_rate(id, up).map_err(|e| e.to_string());
            if let Ok(mut guard) = slot.lock() {
                *guard = Some((generation, result));
            }
        });
    }

    /// Open the comments sub-pane for the highlighted browse result and kick off
    /// an off-thread fetch of its comments. The worker posts `(generation,
    /// result)` into `workshop_comments_slot`, drained by `poll_workshop`. A
    /// missing selection / id is a no-op.
    pub fn workshop_open_comments(&mut self) {
        let Some(item) = self.selected_workshop_result() else {
            return;
        };
        let Some(id) = item.id else {
            return;
        };
        self.workshop_comments_open = true;
        self.workshop_comments_loading = true;
        self.workshop_comments = Vec::new();
        self.workshop_comments_scroll = 0;
        self.workshop_comments_status.clear();
        self.workshop_comments_id = id;
        // Reuse the overlay generation so a fetch for a since-closed/superseded
        // sub-pane is dropped on arrival (the gen is bumped on close/exit).
        let generation = self.workshop_gen;
        let slot = Arc::clone(&self.workshop_comments_slot);
        thread::spawn(move || {
            let result = steam::workshop_comments(id, 30).map_err(|e| e.to_string());
            if let Ok(mut guard) = slot.lock() {
                *guard = Some((generation, result));
            }
        });
    }

    /// Close the comments sub-pane and drop its contents. Bumps the overlay
    /// generation so any in-flight comments fetch is discarded on arrival.
    pub fn close_workshop_comments(&mut self) {
        if self.workshop_comments_open {
            self.workshop_gen = self.workshop_gen.wrapping_add(1);
        }
        self.workshop_comments_open = false;
        self.workshop_comments_loading = false;
        self.workshop_comments = Vec::new();
        self.workshop_comments_scroll = 0;
        self.workshop_comments_status.clear();
        self.workshop_comments_id = 0;
    }

    /// The id of the item the comments sub-pane is showing.
    pub fn workshop_comments_id(&self) -> u64 {
        self.workshop_comments_id
    }

    /// Scroll the comments sub-pane down by one row (clamped).
    pub fn workshop_comments_scroll_down(&mut self) {
        let max = self.workshop_comments.len().saturating_sub(1);
        if self.workshop_comments_scroll < max {
            self.workshop_comments_scroll += 1;
        }
    }

    /// Scroll the comments sub-pane up by one row (clamped).
    pub fn workshop_comments_scroll_up(&mut self) {
        self.workshop_comments_scroll = self.workshop_comments_scroll.saturating_sub(1);
    }
}
