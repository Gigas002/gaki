//! gaki's logic: Steam session, library, install, cloud, launch and the
//! config/profile model. Each capability is a Cargo feature (see
//! `docs/FEATURES.md`); without any, the crate still covers config, profiles,
//! Proton discovery, the local library scan and the launch pipeline.

#[cfg(feature = "authentication")]
pub mod auth;
pub mod compat;
pub mod config;
pub mod core;
pub mod infra;
pub mod launch;
pub mod library;
pub mod ops;
pub mod settings;
#[cfg(feature = "authentication")]
pub mod steam_client;
#[cfg(feature = "authentication")]
pub mod web;
