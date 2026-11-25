//! `steam` command handlers: the Steam client's folders.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use libgaki::library::steam_home::{Choice, Host, Library, Placed, Plan, Step};

use crate::cli::SteamLayoutArg;
use crate::commands::common::confirm_write;

/// `steam_library`, `~` expanded; `steam relocate` needs it set.
fn configured_library() -> Result<Option<PathBuf>> {
    let config = libgaki::ops::config::global()?;
    let home = libgaki::core::config::home_dir()?;
    Ok(config
        .steam_library
        .map(|path| libgaki::config::expand_tilde(&path, &home)))
}

/// `steam status`.
pub(crate) fn cmd_steam_status() -> Result<()> {
    let host = Host::locate()?;
    cli_println!("Steam: {}", host.root.display());
    cli_println!("library: {}", host.library()?);
    match configured_library()? {
        Some(root) => {
            let layout = match host.layout_of(&root) {
                Some(Choice::Gaki) => "gaki's layout",
                Some(Choice::Steam) => "Steam's layout",
                None => "empty",
            };
            cli_println!("steam_library: {} ({layout})", root.display());
        }
        None => cli_println!("steam_library: not set"),
    }
    for place in host.places() {
        let state = match place.placed() {
            Placed::Moved => format!("moved to {}", place.target.display()),
            Placed::Here => "in Steam's folder".to_owned(),
            Placed::Missing => "not there yet".to_owned(),
            Placed::Elsewhere(at) => format!("links to {}", at.display()),
        };
        cli_println!("{}: {state}", place.name);
    }
    Ok(())
}

/// `steam relocate [--layout gaki|steam] [--undo]`.
pub(crate) fn cmd_steam_relocate(
    layout: Option<SteamLayoutArg>,
    undo: bool,
    dry_run: bool,
    yes: bool,
) -> Result<()> {
    let host = Host::locate()?;
    let from = host.library()?;
    let to = if undo {
        Library::InPlace
    } else {
        let root = configured_library()?.context(
            "`steam_library` isn't set: set it to the library to share with Steam \
             (`gaki-cli config set steam_library ~/Games`)",
        )?;
        let choice = match layout {
            Some(SteamLayoutArg::Gaki) => Choice::Gaki,
            Some(SteamLayoutArg::Steam) => Choice::Steam,
            None => host
                .layout_of(&root)
                .unwrap_or(if host.gaki_layout_works(&root) {
                    Choice::Gaki
                } else {
                    Choice::Steam
                }),
        };
        host.target(choice, &root)
    };

    // Moving back, the caches go first: the shader cache returns into the
    // library before the library returns.
    let places = host.plan_places(undo)?;
    let library = host.plan_library(&from, &to)?;
    if places.is_empty() && library.is_empty() {
        cli_println!("Nothing to do: the library is {to}, the caches and logs are in place.");
        return Ok(());
    }
    cli_println!("library: {from} → {to}");
    let (first, second) = if undo {
        (&places, &library)
    } else {
        (&library, &places)
    };
    for step in first.steps.iter().chain(&second.steps) {
        cli_println!("  {step}");
    }
    if dry_run {
        return Ok(());
    }
    if libgaki::core::utils::is_steam_running() {
        bail!("Steam is running; quit it first");
    }
    if let Some((pid, name)) =
        libgaki::library::steam_home::busy(first.moving().chain(second.moving()))
    {
        bail!("{name} (pid {pid}) uses files that would move; stop it first");
    }
    confirm_write("Go ahead? [y/N] ", yes)?;

    run(&host, first)?;
    // The places plan depends on where the library is now.
    let second = if undo {
        host.plan_library(&host.library()?, &to)?
    } else {
        host.plan_places(false)?
    };
    run(&host, &second)?;
    cli_println!("Done: the library is {to}.");
    Ok(())
}

fn run(host: &Host, plan: &Plan) -> Result<()> {
    host.execute(plan, |step| {
        if let Step::Move { from, to } = step {
            tracing::info!(from = %from.display(), to = %to.display(), "moving");
        }
    })
}
