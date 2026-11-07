//! The account and wallet overlays.

use super::Browser;
use crate::app::steam;
use crate::utils::error::TuiError;

impl Browser {
    /// Fetch the logged-in account and open the overlay.
    /// Blocking; returns the backend error if the call fails.
    pub fn open_account(&mut self) -> Result<(), TuiError> {
        self.account_info = Some(steam::account()?);
        self.show_account = true;
        Ok(())
    }

    /// Fetch the Steam Wallet balance and open the overlay.
    /// Blocking; returns the backend error if the call fails.
    #[cfg(feature = "inventory")]
    pub fn open_wallet(&mut self) -> Result<(), TuiError> {
        self.wallet_info = Some(steam::wallet()?);
        self.show_wallet = true;
        Ok(())
    }
}
