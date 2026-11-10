use std::process::ExitCode;

use clap::Parser;

use gaki_tui::app;
use gaki_tui::cli::CliOptions;
use gaki_tui::logger;
use gaki_tui::settings::{Settings, Start};
use gaki_tui::ui::theme::Theme;

fn main() -> ExitCode {
    // TLS runs on ring, which must be installed before any HTTPS client is
    // built; the places that build one install it too.
    libgaki::core::net::install_crypto_provider();
    let cli = CliOptions::parse();
    let settings = match Settings::resolve(cli) {
        Ok(settings) => settings,
        Err(err) => {
            eprintln!("gaki-tui: {err:#}");
            return ExitCode::FAILURE;
        }
    };
    // Key bindings: a bad file refuses to start, naming the line, so a typo
    // never leaves keys silently doing the wrong thing.
    match gaki_tui::utils::paths::keybinds_file()
        .map_err(|e| e.to_string())
        .and_then(|path| gaki_tui::keymap::Keymap::load(&path).map_err(|e| e.to_string()))
    {
        Ok(map) => gaki_tui::keymap::install(map),
        Err(err) => {
            eprintln!("gaki-tui: keybinds: {err}");
            return ExitCode::FAILURE;
        }
    }
    // The theme: a bad file refuses to start, like the key bindings. One
    // named with `--theme` must exist; the default may be missing.
    let theme = match &settings.theme {
        Some(path) => Theme::load(path, true),
        None => match gaki_tui::utils::paths::theme_file() {
            Ok(path) => Theme::load(&path, false),
            Err(err) => {
                eprintln!("gaki-tui: theme: {err}");
                return ExitCode::FAILURE;
            }
        },
    };
    match theme {
        Ok(theme) => gaki_tui::ui::theme::install(theme, settings.theme.clone()),
        Err(err) => {
            eprintln!("gaki-tui: theme: {err}");
            return ExitCode::FAILURE;
        }
    }
    // Route libgaki's (and the artwork loader's) HTTP through the configured
    // proxy. This sets env vars, so it must run before any thread starts.
    libgaki::core::net::install_proxy_env(&settings.proxy);
    // Still on a cooked terminal, so a warning here is readable. Running
    // without a log file beats refusing to start.
    if let Err(err) = logger::init(&settings) {
        eprintln!("gaki-tui: logging disabled: {err:#}");
    }

    let result = match settings.start {
        #[cfg(feature = "social")]
        Start::Chat { steamid, name } => app::chat::run(steamid, name),
        Start::Library => app::library::run(),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            // The run loop's terminal guard has restored the console by now.
            tracing::error!(error = %err, "gaki-tui exited with an error");
            eprintln!("gaki-tui: {err}");
            ExitCode::FAILURE
        }
    }
}
