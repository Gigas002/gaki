//! The account, settings (presence, proxy) and wallet overlays.

use super::Browser;
use crate::interface::aurelia;
use crate::utils::error::STError;

impl Browser {
    /// Fetch the logged-in account and open the overlay.
    /// Blocking; returns the backend error if the call fails.
    pub fn open_account(&mut self) -> Result<(), STError> {
        self.account_info = Some(aurelia::account()?);
        self.show_account = true;
        Ok(())
    }

    /// Open the settings overlay even if the launcher's settings cannot be read.
    /// Launcher fetches remain blocking, matching this overlay's existing path;
    /// a failure is reported in the overlay.
    pub fn open_config(&mut self) -> Result<(), STError> {
        self.show_config = true;
        self.config_info = None;
        self.config_proxy = None;
        match aurelia::config_show() {
            Ok(info) => self.config_info = Some(info),
            Err(error) => {
                self.notice = Some(format!("Launcher settings unavailable: {error}"));
                return Err(error);
            }
        }
        // Best-effort: a missing/failed proxy read just leaves the row blank.
        self.config_proxy = aurelia::config_proxy_show().ok();
        Ok(())
    }

    /// Close the config overlay and drop its contents.
    pub fn close_config(&mut self) {
        self.show_config = false;
        self.config_info = None;
        self.config_proxy = None;
        self.config_proxy_input = None;
    }

    /// Re-fetch just the proxy setting (after an edit/clear), keeping the overlay
    /// open.
    fn refresh_proxy(&mut self) {
        self.config_proxy = aurelia::config_proxy_show().ok();
    }

    /// Start editing the proxy URL, prefilling the current value.
    pub fn begin_proxy_edit(&mut self) {
        let current = self
            .config_proxy
            .as_ref()
            .and_then(|p| p.url.clone())
            .unwrap_or_default();
        self.config_proxy_input = Some(current);
    }

    pub fn proxy_input_push(&mut self, c: char) {
        if let Some(s) = self.config_proxy_input.as_mut() {
            s.push(c);
        }
    }

    pub fn proxy_input_pop(&mut self) {
        if let Some(s) = self.config_proxy_input.as_mut() {
            s.pop();
        }
    }

    pub fn cancel_proxy_edit(&mut self) {
        self.config_proxy_input = None;
    }

    /// Commit the typed proxy URL; an empty value
    /// clears the proxy. A backend/validation error surfaces as a status notice.
    pub fn commit_proxy_edit(&mut self) {
        let Some(url) = self.config_proxy_input.take() else {
            return;
        };
        if let Err(e) = aurelia::config_proxy_set(url.trim()) {
            self.notice = Some(format!("Proxy: {e}"));
        }
        self.refresh_proxy();
    }

    /// Clear the configured proxy.
    pub fn clear_proxy(&mut self) {
        if let Err(e) = aurelia::config_proxy_clear() {
            self.notice = Some(format!("Proxy: {e}"));
        }
        self.refresh_proxy();
    }

    /// Fetch the Steam Wallet balance and open the overlay.
    /// Blocking; returns the backend error if the call fails.
    pub fn open_wallet(&mut self) -> Result<(), STError> {
        self.wallet_info = Some(aurelia::wallet()?);
        self.show_wallet = true;
        Ok(())
    }
}
