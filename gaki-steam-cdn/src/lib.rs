// Based on steam-cdn (Apache-2.0), modified by Aurelia and gaki; see README.md.
mod cdn;
mod crypto;
mod error;
mod utils;
pub mod web_api;

pub use cdn::CDNClient;
pub use cdn::CONCURRENCY;
pub use cdn::manifest::file::ProgressFn;
pub use error::Error;
