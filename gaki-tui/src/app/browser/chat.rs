//! The chat view, in the overlay or the stand-alone `--chat` window.

use super::Browser;
use crate::app::steam;

impl Browser {
    /// Open the chat view for the highlighted friend. Sets the partner, clears
    /// the input, and fetches recent history (a fetch error opens an empty
    /// conversation). No-op when no friend is selected.
    pub fn open_chat(&mut self) {
        let Some(friend) = self.selected_friend() else {
            return;
        };
        let steamid = friend.steam_id;
        let partner = friend.display_name();
        self.chat_steamid = steamid;
        self.chat_partner = partner;
        self.chat_input.clear();
        self.chat_scroll = 0;
        self.chat_messages = steam::chat_history(self.chat_steamid, 30).unwrap_or_default();
        self.show_chat = true;
    }

    /// Close the chat view and drop its state.
    pub fn close_chat(&mut self) {
        self.show_chat = false;
        self.chat_messages = Vec::new();
        self.chat_steamid = 0;
        self.chat_partner = String::new();
        self.chat_input.clear();
        self.chat_scroll = 0;
    }

    /// Re-fetch the open conversation's history (e.g. after sending a message).
    pub fn refresh_chat(&mut self) {
        self.chat_messages = steam::chat_history(self.chat_steamid, 30).unwrap_or_default();
        // The chat panel is bottom-anchored and renders the Vec top→bottom, so it
        // must be oldest→newest. The CLI returns history newest-first, so reverse
        // it; then a *stable* sort by send time enforces true chronological order
        // when timestamps are present, and is a harmless no-op (keeping the
        // reversed oldest→newest order) when they are not.
        self.chat_messages.reverse();
        self.chat_messages.sort_by_key(|m| m.timestamp);
    }

    /// Send the composed message to the chat partner (blocking), then clear the
    /// input and re-fetch the history. No-op when the input is empty.
    pub fn chat_send(&mut self) {
        if self.chat_input.is_empty() {
            return;
        }
        let _ = steam::chat_send(self.chat_steamid, &self.chat_input);
        self.chat_input.clear();
        self.refresh_chat();
    }

    /// Append a typed character to the composed message.
    pub fn chat_push(&mut self, c: char) {
        self.chat_input.push(c);
    }

    /// Remove the last character from the composed message.
    pub fn chat_pop(&mut self) {
        self.chat_input.pop();
    }
}
