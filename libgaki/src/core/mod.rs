pub mod acf;
pub mod config;
pub mod error;
pub mod models;
#[cfg(feature = "authentication")]
pub mod net;
pub mod steam_urls;
pub mod store;
pub mod utils;

/// Steam API language for every store and community request. gaki shows
/// Steam's English metadata only.
pub const STEAM_LANGUAGE: &str = "english";
