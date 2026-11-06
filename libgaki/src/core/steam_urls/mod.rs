//! Conventional Steam CDN / storefront URL builders.
//!
//! Shared by the CLI's `--json` responses (`list`, `info`, `dlc`) and the TUI, so a
//! consuming driver (e.g. Heroic) gets ready-to-use artwork and store URLs
//! instead of hard-coding the base paths and reconstructing them from app ids.

const CDN_APPS_BASE: &str = "https://cdn.cloudflare.steamstatic.com/steam/apps";
const STORE_APP_BASE: &str = "https://store.steampowered.com/app";
const PROTONDB_APP_BASE: &str = "https://www.protondb.com/app";
const PROTON_ISSUES: &str = "https://github.com/ValveSoftware/Proton/issues";

/// Store page for an app (or DLC) id.
pub fn store_url(app_id: u32) -> String {
    format!("{STORE_APP_BASE}/{app_id}")
}

/// Wide store header / capsule image.
pub fn header_url(app_id: u32) -> String {
    format!("{CDN_APPS_BASE}/{app_id}/header.jpg")
}

/// Portrait library cover (`library_600x900`).
pub fn capsule_url(app_id: u32) -> String {
    format!("{CDN_APPS_BASE}/{app_id}/library_600x900.jpg")
}

/// Small horizontal capsule, used as a DLC artwork fallback.
pub fn small_capsule_url(app_id: u32) -> String {
    format!("{CDN_APPS_BASE}/{app_id}/capsule_231x87.jpg")
}

/// Wide library hero background.
pub fn hero_url(app_id: u32) -> String {
    format!("{CDN_APPS_BASE}/{app_id}/library_hero.jpg")
}

/// Transparent game logo.
pub fn logo_url(app_id: u32) -> String {
    format!("{CDN_APPS_BASE}/{app_id}/logo.png")
}

/// The app's ProtonDB page: compatibility reports and tweaks.
pub fn protondb_url(app_id: u32) -> String {
    format!("{PROTONDB_APP_BASE}/{app_id}")
}

/// A search of Valve's Proton issue tracker for the app id. Each game's
/// compatibility issue there is titled with its app id.
pub fn proton_issues_url(app_id: u32) -> String {
    format!("{PROTON_ISSUES}?q=is%3Aissue+{app_id}")
}

#[cfg(test)]
mod tests;
