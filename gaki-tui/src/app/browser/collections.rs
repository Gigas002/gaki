//! The collections overlay.

use super::Browser;
use crate::interface::aurelia;

impl Browser {
    /// Fetch the account's collections and open the overlay for `app_id` (the
    /// game that add/remove will act on).
    pub fn open_collections(&mut self, app_id: i32) {
        self.collections = aurelia::collections_list().unwrap_or_default();
        self.collections_app_id = app_id;
        self.collections_index = 0;
        self.collections_status.clear();
        self.collections_input = None;
        self.show_collections = true;
    }

    pub fn close_collections(&mut self) {
        self.show_collections = false;
        self.collections.clear();
        self.collections_index = 0;
        self.collections_input = None;
    }

    pub fn selected_collection(&self) -> Option<&aurelia::CollectionJson> {
        self.collections.get(self.collections_index)
    }

    pub fn collections_next(&mut self) {
        if self.collections.is_empty() {
            self.collections_index = 0;
            return;
        }
        self.collections_index = (self.collections_index + 1) % self.collections.len();
    }

    pub fn collections_previous(&mut self) {
        if self.collections.is_empty() {
            self.collections_index = 0;
            return;
        }
        self.collections_index = if self.collections_index == 0 {
            self.collections.len() - 1
        } else {
            self.collections_index - 1
        };
    }

    fn refresh_collections(&mut self) {
        self.collections = aurelia::collections_list().unwrap_or_default();
        if self.collections_index >= self.collections.len() {
            self.collections_index = self.collections.len().saturating_sub(1);
        }
    }

    pub fn add_game_to_selected_collection(&mut self) {
        let app = self.collections_app_id;
        let Some(name) = self.selected_collection().map(|c| c.name.clone()) else {
            return;
        };
        self.collections_status = match aurelia::collection_add(&name, &[app]) {
            Ok(()) => format!("Added to \"{name}\"."),
            Err(e) => format!("Failed: {e}"),
        };
        self.refresh_collections();
    }

    pub fn remove_game_from_selected_collection(&mut self) {
        let app = self.collections_app_id;
        let Some(name) = self.selected_collection().map(|c| c.name.clone()) else {
            return;
        };
        self.collections_status = match aurelia::collection_remove(&name, &[app]) {
            Ok(()) => format!("Removed from \"{name}\"."),
            Err(e) => format!("Failed: {e}"),
        };
        self.refresh_collections();
    }

    pub fn delete_selected_collection(&mut self) {
        let Some(name) = self.selected_collection().map(|c| c.name.clone()) else {
            return;
        };
        self.collections_status = match aurelia::collection_delete(&name) {
            Ok(()) => format!("Deleted \"{name}\"."),
            Err(e) => format!("Failed: {e}"),
        };
        self.refresh_collections();
    }

    pub fn collections_pull(&mut self) {
        self.collections_status = match aurelia::collections_pull() {
            Ok(()) => "Pulled collections from Steam Cloud.".to_string(),
            Err(e) => format!("Failed: {e}"),
        };
        self.refresh_collections();
    }

    pub fn collections_push(&mut self) {
        self.collections_status = match aurelia::collection_push() {
            Ok(()) => "Pushed collections to Steam Cloud.".to_string(),
            Err(e) => format!("Failed: {e}"),
        };
    }

    pub fn collections_sync(&mut self) {
        self.collections_status = match aurelia::collection_sync() {
            Ok(()) => "Synced collections with Steam Cloud.".to_string(),
            Err(e) => format!("Failed: {e}"),
        };
        self.refresh_collections();
    }

    /// Start the inline "new collection" prompt.
    pub fn begin_collection_create(&mut self) {
        self.collections_input = Some(String::new());
    }

    pub fn collections_input_push(&mut self, c: char) {
        if let Some(s) = self.collections_input.as_mut() {
            s.push(c);
        }
    }

    pub fn collections_input_pop(&mut self) {
        if let Some(s) = self.collections_input.as_mut() {
            s.pop();
        }
    }

    pub fn cancel_collection_create(&mut self) {
        self.collections_input = None;
    }

    pub fn commit_collection_create(&mut self) {
        let Some(name) = self.collections_input.take() else {
            return;
        };
        let name = name.trim().to_string();
        if name.is_empty() {
            return;
        }
        self.collections_status = match aurelia::collection_create(&name) {
            Ok(()) => format!("Created \"{name}\"."),
            Err(e) => format!("Failed: {e}"),
        };
        self.refresh_collections();
    }
}
