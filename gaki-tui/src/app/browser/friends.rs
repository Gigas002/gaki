//! The Friends panel and friend management (search, add, remove).

use std::thread;

use super::{Browser, View};
use crate::app::steam;

impl Browser {
    /// Whether the Friends tab is the active view (so the Friends & Chat panel
    /// holds keyboard focus). Derived from the view — the Friends tab *is* the
    /// focus, so there is no separate focus flag to drift out of sync.
    pub fn friends_focused(&self) -> bool {
        self.view == View::Friends
    }

    /// Enter the Friends view (select the Friends tab). The tab is shown
    /// immediately; the roster is lazy-loaded off the UI thread the first time
    /// (the panel shows "Loading friends…" until [`Browser::poll_friends_ops`]
    /// adopts the result), so tabbing in never blocks/lags. Idempotent:
    /// re-entering keeps the already-loaded list.
    pub fn enter_friends(&mut self) {
        self.view = View::Friends;
        if !self.friends_loaded && !self.friends_loading {
            self.friends_index = 0;
            self.friends_loading = true;
            // Reuse the existing off-thread refresh path: flag it and the poll
            // loop spawns the worker and adopts the roster when it lands.
            if let Ok(mut flag) = self.friends_refresh_pending.lock() {
                *flag = true;
            }
        }
    }

    /// Toggle the Friends view on/off. Entering loads the roster (see
    /// [`Browser::enter_friends`]); leaving returns to the Library view (the
    /// active filter is preserved) while keeping the loaded list.
    pub fn toggle_friends_focus(&mut self) {
        if self.view == View::Friends {
            self.view = View::Library;
        } else {
            self.enter_friends();
        }
    }

    /// Leave the Friends view (Esc), returning to the Library list while keeping
    /// the loaded friends list.
    pub fn unfocus_friends(&mut self) {
        self.view = View::Library;
    }

    /// The highlighted friend, if any.
    pub fn selected_friend(&self) -> Option<&steam::FriendJson> {
        self.friends.get(self.friends_index)
    }

    /// Move the friends highlight down by one row (clamped). The widget scrolls
    /// the whole list to keep the highlight visible.
    pub fn friends_scroll_down(&mut self) {
        if self.friends.is_empty() {
            self.friends_index = 0;
            return;
        }
        if self.friends_index + 1 < self.friends.len() {
            self.friends_index += 1;
        }
    }

    /// Move the friends highlight up by one row (clamped).
    pub fn friends_scroll_up(&mut self) {
        self.friends_index = self.friends_index.saturating_sub(1);
    }

    // --- Friend management (search / add / remove) ---

    /// Open the add-friend (search/add) overlay, clearing any previous query,
    /// resolved preview, and status.
    pub fn open_friend_add(&mut self) {
        self.show_friend_add = true;
        self.friend_add_query.clear();
        self.friend_search_result = None;
        self.friend_search_gen = self.friend_search_gen.wrapping_add(1);
        if let Ok(mut status) = self.friend_add_status.lock() {
            status.clear();
        }
        if let Ok(mut pending) = self.friend_search_pending.lock() {
            *pending = None;
        }
    }

    /// Close the add-friend overlay and drop its contents.
    pub fn close_friend_add(&mut self) {
        self.show_friend_add = false;
        self.friend_add_query.clear();
        self.friend_search_result = None;
        self.friend_search_gen = self.friend_search_gen.wrapping_add(1);
        if let Ok(mut status) = self.friend_add_status.lock() {
            status.clear();
        }
        if let Ok(mut pending) = self.friend_search_pending.lock() {
            *pending = None;
        }
    }

    /// Append a character to the add-friend query.
    pub fn friend_add_push(&mut self, c: char) {
        self.friend_add_query.push(c);
        // A fresh edit invalidates any previously resolved preview. Bump the
        // generation and drop any in-flight/published search result so a late
        // worker can't resurrect a stale preview (and add target) for the
        // now-edited query — its tagged result will no longer match.
        self.friend_search_result = None;
        self.friend_search_gen = self.friend_search_gen.wrapping_add(1);
        if let Ok(mut pending) = self.friend_search_pending.lock() {
            *pending = None;
        }
    }

    /// Delete the last character from the add-friend query.
    pub fn friend_add_pop(&mut self) {
        self.friend_add_query.pop();
        self.friend_search_result = None;
        self.friend_search_gen = self.friend_search_gen.wrapping_add(1);
        if let Ok(mut pending) = self.friend_search_pending.lock() {
            *pending = None;
        }
    }

    /// Resolve the typed reference to a concrete account via `friends search`,
    /// off the UI thread. The worker publishes the result into shared cells that
    /// [`Browser::poll_friends_ops`] adopts; the status line reflects progress.
    pub fn friend_search(&mut self) {
        let query = self.friend_add_query.trim().to_string();
        if query.is_empty() {
            return;
        }
        if let Ok(mut status) = self.friend_add_status.lock() {
            *status = "searching...".to_string();
        }
        let status = self.friend_add_status.clone();
        let pending = self.friend_search_pending.clone();
        // Tag the worker with the generation it was launched for; the poll only
        // adopts a result whose tag still matches the current generation.
        let search_gen = self.friend_search_gen;
        thread::spawn(move || match steam::friends_search(&query) {
            Ok(found) => {
                if let Ok(mut status) = status.lock() {
                    *status = format!("Found {}", found.display_name());
                }
                if let Ok(mut pending) = pending.lock() {
                    *pending = Some((search_gen, found));
                }
            }
            Err(err) => {
                if let Ok(mut status) = status.lock() {
                    *status = format!("Not found: {}", err);
                }
            }
        });
    }

    /// Send a friend request for the resolved account (or, if no search has run,
    /// the raw typed query) via `friends add`, off the UI thread. On success the
    /// roster is flagged for refresh and the overlay reports the outcome.
    pub fn friend_add_confirm(&mut self) {
        // Prefer the resolved SteamID; fall back to the raw query so `add` still
        // works if the user skipped the search step.
        let target = match &self.friend_search_result {
            Some(found) => found.steam_id.to_string(),
            None => self.friend_add_query.trim().to_string(),
        };
        if target.is_empty() {
            return;
        }
        if let Ok(mut status) = self.friend_add_status.lock() {
            *status = "sending request...".to_string();
        }
        let status = self.friend_add_status.clone();
        let refresh = self.friends_refresh_pending.clone();
        thread::spawn(move || match steam::friends_add(&target) {
            Ok(()) => {
                if let Ok(mut status) = status.lock() {
                    *status = "Request sent.".to_string();
                }
                if let Ok(mut refresh) = refresh.lock() {
                    *refresh = true;
                }
            }
            Err(err) => {
                if let Ok(mut status) = status.lock() {
                    *status = format!("Failed: {}", err);
                }
            }
        });
    }

    /// Remove the highlighted friend (or cancel a pending request) via
    /// `friends remove`, off the UI thread. On success the roster is flagged for
    /// refresh. A missing selection is a no-op.
    pub fn friend_remove_confirm(&mut self) {
        let Some(friend) = self.selected_friend() else {
            return;
        };
        let steamid = friend.steam_id;
        let refresh = self.friends_refresh_pending.clone();
        thread::spawn(move || {
            if steam::friends_remove(steamid).is_ok()
                && let Ok(mut refresh) = refresh.lock()
            {
                *refresh = true;
            }
        });
    }

    /// Apply any async friend-management results: adopt a resolved search into
    /// the preview, and re-fetch the roster (off the UI thread) when an add or
    /// remove has succeeded. Called once per event-loop iteration.
    pub fn poll_friends_ops(&mut self) {
        // Adopt a resolved search result published by the worker thread, but only
        // if it was launched for the current query generation. A result tagged
        // with a stale generation (the query was edited or the overlay toggled
        // after the worker spawned) is dropped, closing the TOCTOU where a late
        // worker could briefly revive a preview for the now-edited query.
        let adopted = self
            .friend_search_pending
            .lock()
            .ok()
            .and_then(|mut pending| pending.take());
        if let Some((search_gen, found)) = adopted
            && search_gen == self.friend_search_gen
        {
            self.friend_search_result = Some(found);
        }

        // Re-fetch the roster after a successful add/remove. The fetch itself is
        // a quick CLI call; run it on a worker and adopt the list next poll via
        // the same `friends` field (kept simple: a one-shot blocking re-fetch on
        // a detached thread that swaps the vec in through a channel-free flag).
        let needs_refresh = self
            .friends_refresh_pending
            .lock()
            .map(|mut flag| std::mem::replace(&mut *flag, false))
            .unwrap_or(false);
        if needs_refresh {
            // Fetch off the UI thread, then publish into the search-pending-style
            // roster cell adopted on a later poll.
            let roster = self.friends_roster_pending.clone();
            thread::spawn(move || {
                let list = steam::friends().unwrap_or_default();
                if let Ok(mut slot) = roster.lock() {
                    *slot = Some(list);
                }
            });
        }

        // Adopt a freshly fetched roster, if one is ready, keeping the highlight
        // in range.
        if let Ok(mut slot) = self.friends_roster_pending.lock()
            && let Some(list) = slot.take()
        {
            self.friends = list;
            self.friends_loaded = true;
            self.friends_loading = false;
            if self.friends_index >= self.friends.len() {
                self.friends_index = self.friends.len().saturating_sub(1);
            }
        }
    }

    /// Open a dedicated chat **in a new terminal window** with the highlighted
    /// friend: spawns this same binary with `--chat <steamid> --name <name>`, which
    /// launches straight into the full-screen chat loop. A missing selection or
    /// spawn failure is a no-op (the panel stays as-is).
    pub fn open_chat_terminal(&self) {
        let Some(friend) = self.selected_friend() else {
            return;
        };
        let steamid = friend.steam_id;
        let name = friend.display_name();
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let exe = exe.to_string_lossy().into_owned();
        let id = steamid.to_string();

        // Best-effort: launch the chat binary directly, detached.
        let _ = std::process::Command::new(&exe)
            // `--name=` keeps a name that starts with `-` from reading as a flag.
            .args(["--chat", &id, &format!("--name={name}")])
            .spawn();
    }
}
