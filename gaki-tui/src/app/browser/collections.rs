//! The collections overlay.

use super::Browser;
use crate::app::steam;

impl Browser {
    /// Fetch the account's collections and open the overlay for `app_id` (the
    /// game that add/remove will act on).
    pub fn open_collections(&mut self, app_id: i32) {
        self.collections = steam::collections_list().unwrap_or_default();
        // The overlay may have changed the favourites or hidden games.
        crate::app::marks::reload();
        self.collections_app_id = app_id;
        self.collections_index = 0;
        self.collections_status.clear();
        self.collections_input = None;
        self.collections_renaming = None;
        self.collections_delete = None;
        self.show_collections = true;
    }

    pub fn close_collections(&mut self) {
        self.show_collections = false;
        self.collections.clear();
        self.collections_index = 0;
        self.collections_input = None;
        self.collections_renaming = None;
        self.collections_delete = None;
    }

    pub fn selected_collection(&self) -> Option<&steam::Collection> {
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
        self.collections = steam::collections_list().unwrap_or_default();
        // The overlay may have changed the favourites or hidden games.
        crate::app::marks::reload();
        if self.collections_index >= self.collections.len() {
            self.collections_index = self.collections.len().saturating_sub(1);
        }
    }

    pub fn add_game_to_selected_collection(&mut self) {
        let app = self.collections_app_id;
        let Some(name) = self.selected_collection().map(|c| c.name.clone()) else {
            return;
        };
        self.collections_status = match steam::collection_add(&name, &[app]) {
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
        self.collections_status = match steam::collection_remove(&name, &[app]) {
            Ok(()) => format!("Removed from \"{name}\"."),
            Err(e) => format!("Failed: {e}"),
        };
        self.refresh_collections();
    }

    /// Ask to delete the highlighted collection (`x`).
    pub fn begin_collection_delete(&mut self) {
        self.collections_delete = self.selected_collection().map(|c| c.name.clone());
    }

    /// Answer the delete confirm: `yes` deletes it (its games stay).
    pub fn answer_collection_delete(&mut self, yes: bool) {
        let Some(name) = self.collections_delete.take() else {
            return;
        };
        if !yes {
            return;
        }
        self.collections_status = match steam::collection_delete(&name) {
            Ok(()) => format!("Deleted \"{name}\"."),
            Err(e) => format!("Failed: {e}"),
        };
        self.refresh_collections();
    }

    pub fn collections_pull(&mut self) {
        self.collections_status = match steam::collections_pull() {
            Ok(()) => "Pulled collections from Steam Cloud.".to_string(),
            Err(e) => format!("Failed: {e}"),
        };
        self.refresh_collections();
    }

    pub fn collections_push(&mut self) {
        self.collections_status = match steam::collection_push() {
            Ok(()) => "Pushed collections to Steam Cloud.".to_string(),
            Err(e) => format!("Failed: {e}"),
        };
    }

    pub fn collections_sync(&mut self) {
        self.collections_status = match steam::collection_sync() {
            Ok(()) => "Synced collections with Steam Cloud.".to_string(),
            Err(e) => format!("Failed: {e}"),
        };
        self.refresh_collections();
    }

    /// Start the inline "new collection" prompt.
    pub fn begin_collection_create(&mut self) {
        self.collections_renaming = None;
        self.collections_input = Some(String::new());
    }

    /// Start renaming the highlighted collection (`e`), the prompt
    /// prefilled with its name.
    pub fn begin_collection_rename(&mut self) {
        let Some(name) = self.selected_collection().map(|c| c.name.clone()) else {
            return;
        };
        self.collections_input = Some(name.clone());
        self.collections_renaming = Some(name);
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

    /// Cancel the create or rename prompt.
    pub fn cancel_collection_input(&mut self) {
        self.collections_input = None;
        self.collections_renaming = None;
    }

    /// Enter in the prompt: create the collection, or rename the one being
    /// renamed. An empty name is refused and keeps the prompt open; an
    /// unchanged one just closes it.
    pub fn commit_collection_input(&mut self) {
        let Some(text) = &self.collections_input else {
            return;
        };
        let name = text.trim().to_string();
        if name.is_empty() {
            self.collections_status = "The name can't be empty.".to_string();
            return;
        }
        self.collections_input = None;
        let renaming = self.collections_renaming.take();
        self.collections_status = match renaming {
            Some(old) if old == name => String::new(),
            Some(old) => match steam::collection_rename(&old, &name) {
                Ok(()) => format!("Renamed \"{old}\" to \"{name}\"."),
                Err(e) => format!("Failed: {e}"),
            },
            None => match steam::collection_create(&name) {
                Ok(()) => format!("Created \"{name}\"."),
                Err(e) => format!("Failed: {e}"),
            },
        };
        self.refresh_collections();
        if let Some(i) = self.collections.iter().position(|c| c.name == name) {
            self.collections_index = i;
        }
    }
}
