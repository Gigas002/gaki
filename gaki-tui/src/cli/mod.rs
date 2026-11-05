//! Command-line arguments. Parsed once, in `main`; everything past
//! [`crate::settings`] sees only the resolved `Settings`.

use clap::{Parser, ValueEnum};

#[cfg(test)]
mod tests;

/// Terminal UI for the gaki Steam client.
#[derive(Debug, Parser)]
#[command(name = "gaki", version, about)]
pub struct CliOptions {
    /// Open a full-screen chat with this friend instead of the library. The
    /// Friends panel's "open in new terminal" action (`t`) starts this.
    #[arg(long, value_name = "STEAMID")]
    pub chat: Option<u64>,

    /// The chat partner's display name, shown in the chat header.
    #[arg(long, value_name = "NAME", requires = "chat")]
    pub name: Option<String>,

    /// How much to write to the log file.
    #[arg(long, value_enum, value_name = "LEVEL")]
    pub log_level: Option<LogLevel>,
}

/// Log verbosity, from quietest to loudest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LogLevel {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}
