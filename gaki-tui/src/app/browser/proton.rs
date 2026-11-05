//! The Proton runtimes overlay.

use super::Browser;
use crate::app::steam;
use crate::utils::error::STError;

impl Browser {
    /// Fetch the Proton/Wine runtimes and open the overlay (blocking); the
    /// selection resets to the first row.
    pub fn open_proton(&mut self) -> Result<(), STError> {
        self.protons = steam::proton_list()?;
        self.proton_index = 0;
        self.show_proton = true;
        Ok(())
    }

    /// Close the Proton overlay and drop its contents.
    pub fn close_proton(&mut self) {
        self.show_proton = false;
        self.confirm_proton_uninstall = false;
        self.protons.clear();
        self.proton_index = 0;
    }

    /// Re-fetch the Proton runtimes (e.g. after an install/uninstall) while
    /// keeping the current row highlighted. A fetch error leaves the list as-is.
    pub fn refresh_proton(&mut self) {
        if let Ok(protons) = steam::proton_list() {
            self.protons = protons;
            self.proton_index = self.proton_index.min(self.protons.len().saturating_sub(1));
        }
    }

    /// The current Proton install/uninstall status line, if any work is in
    /// flight or just finished (cloned out of the shared status cell).
    pub fn proton_status_line(&self) -> Option<String> {
        self.proton_status
            .lock()
            .ok()
            .and_then(|s| s.as_ref().map(|st| st.state.clone()))
    }

    /// Drop any leftover Proton status line.
    pub fn clear_proton_status(&self) {
        if let Ok(mut guard) = self.proton_status.lock() {
            *guard = None;
        }
    }

    /// Whether the highlighted runtime can be uninstalled (an installed official
    /// build). Drives whether the uninstall key/confirm is offered.
    pub fn selected_proton_uninstallable(&self) -> bool {
        self.selected_proton()
            .map(|p| p.uninstallable())
            .unwrap_or(false)
    }

    /// Uninstall the highlighted official runtime and refresh the list. A
    /// no-op unless the highlighted runtime is uninstallable.
    pub fn do_proton_uninstall(&mut self) -> Result<(), STError> {
        let name = match self.selected_proton() {
            Some(p) if p.uninstallable() => p.name.clone(),
            _ => return Ok(()),
        };
        steam::proton_uninstall(&name)?;
        self.refresh_proton();
        Ok(())
    }

    /// The highlighted Proton runtime, if any.
    pub fn selected_proton(&self) -> Option<&steam::ProtonJson> {
        self.protons.get(self.proton_index)
    }

    pub fn proton_next(&mut self) {
        if self.protons.is_empty() {
            self.proton_index = 0;
            return;
        }
        self.proton_index = (self.proton_index + 1) % self.protons.len();
    }

    pub fn proton_previous(&mut self) {
        if self.protons.is_empty() {
            self.proton_index = 0;
            return;
        }
        self.proton_index = if self.proton_index == 0 {
            self.protons.len() - 1
        } else {
            self.proton_index - 1
        };
    }
}
