//! Application operations: the use cases behind every gaki command and view.
//!
//! Each submodule takes typed arguments and returns typed results. Nothing in
//! here prints, prompts or parses CLI flags, so `gaki-cli` and `gaki-tui` can
//! share the same behaviour and differ only in presentation.

pub mod install;
pub mod library;
pub mod progress;
pub mod session;
pub mod steam;
pub mod store;
