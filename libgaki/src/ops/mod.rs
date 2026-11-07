//! Application operations: the use cases behind every gaki command and view.
//!
//! Each submodule takes typed arguments and returns typed results. Nothing in
//! here prints, prompts or parses CLI flags, so `gaki-cli` and `gaki-tui` can
//! share the same behaviour and differ only in presentation.

#[cfg(feature = "authentication")]
pub mod auth;
#[cfg(feature = "cloud")]
pub mod cloud;
#[cfg(feature = "collections")]
pub mod collections;
pub mod config;
pub mod daemon;
#[cfg(feature = "depot")]
pub mod depot;
#[cfg(feature = "library")]
pub mod info;
#[cfg(feature = "install")]
pub mod install;
#[cfg(feature = "library")]
pub mod launch;
#[cfg(feature = "library")]
pub mod library;
#[cfg(feature = "inventory")]
pub mod market;
pub mod progress;
pub mod proton;
#[cfg(feature = "authentication")]
pub mod session;
#[cfg(feature = "social")]
pub mod social;
pub mod steam;
#[cfg(feature = "store")]
pub mod store;
#[cfg(feature = "workshop")]
pub mod workshop;
