//! `steam` command handlers: the Steam client's folders and its games'
//! prefixes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use libgaki::config::{Catalog, Runner};
use libgaki::library::shared_prefix::{self, Game, Kept, Prefix, Shared, Want};
use libgaki::library::steam_home::{Choice, Host, Library, Placed, Plan, Step};

use crate::cli::SteamLayoutArg;
use crate::commands::common::confirm_write;
use crate::output::style;

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
    cli_println!("{} {}", style::heading("Steam:"), host.root.display());
    let library = host.library()?;
    let library_text = library.to_string();
    let library_text = match library {
        Library::InPlace => style::yellow(&library_text),
        _ => style::green(&library_text),
    };
    cli_println!("{} {library_text}", style::heading("library:"));
    let configured = match configured_library()? {
        Some(root) => {
            let layout = match host.layout_of(&root) {
                Some(Choice::Gaki) => "gaki's layout",
                Some(Choice::Steam) => "Steam's layout",
                None => "empty",
            };
            format!("{} ({layout})", root.display())
        }
        None => style::yellow("not set"),
    };
    cli_println!("{} {configured}", style::heading("steam_library:"));

    let mut rows = vec![vec![
        "FOLDER".to_owned(),
        "STATE".to_owned(),
        "WHERE".to_owned(),
    ]];
    for place in host.places() {
        let (state, at) = match place.placed() {
            Placed::Moved => (style::green("moved"), place.target.display().to_string()),
            Placed::Here => (
                style::yellow("in Steam's folder"),
                place.path.display().to_string(),
            ),
            Placed::Missing => (
                "not there yet".to_owned(),
                place.target.display().to_string(),
            ),
            Placed::Elsewhere(at) => (style::red("links elsewhere"), at.display().to_string()),
        };
        rows.push(vec![place.name.to_owned(), state, at]);
    }
    for line in style::table(&rows) {
        cli_println!("{line}");
    }
    prefix_status()
}

/// The prefix of each game Steam runs through Proton, and its tool.
fn prefix_status() -> Result<()> {
    let prefixes = shared_prefix::Host::locate()?;
    let catalog = Catalog::load_user()?;
    let home = libgaki::core::config::home_dir()?;
    let setting = match &catalog.config.steam_prefix {
        Some(name) if catalog.config.experimental => format!("profile `{name}`"),
        Some(name) => format!(
            "profile `{name}` {}",
            style::yellow("(unused: `experimental` is off)")
        ),
        None => "not set".to_owned(),
    };
    cli_println!("{} {setting}", style::heading("steam_prefix:"));
    let games = prefixes.games()?;
    if games.is_empty() {
        cli_println!("No game Steam runs through Proton is installed.");
        return Ok(());
    }
    // Which profile a link leads to.
    let profile_of = |target: &Path| {
        catalog.profiles.iter().find_map(|profile| {
            let prefix = libgaki::config::expand_tilde(profile.prefix.as_ref()?, &home);
            shared_prefix::same_path(&prefix, target).then(|| profile.name.clone())
        })
    };
    let mut rows = vec![vec![
        "APPID".to_owned(),
        "GAME".to_owned(),
        "PREFIX".to_owned(),
        "TOOL".to_owned(),
        "WHERE".to_owned(),
    ]];
    for game in &games {
        let (state, at) = match prefixes.prefix(game)? {
            Prefix::None => ("none yet".to_owned(), "-".to_owned()),
            Prefix::Own => ("own".to_owned(), game.compat.display().to_string()),
            Prefix::Linked { target, ours } => {
                let state = match (profile_of(&target), ours) {
                    (Some(profile), true) => style::green(&format!("shared: {profile}")),
                    (Some(profile), false) => {
                        style::yellow(&format!("shared: {profile}, not by gaki"))
                    }
                    (None, _) => style::yellow("linked, not by gaki"),
                };
                (state, target.display().to_string())
            }
        };
        rows.push(vec![
            game.app_id.to_string(),
            game.name.clone(),
            state,
            game.tool
                .clone()
                .unwrap_or_else(|| "Steam's default".to_owned()),
            at,
        ]);
    }
    for line in style::table(&rows) {
        cli_println!("{line}");
    }
    Ok(())
}

/// `profile`'s prefix to share, `None` when it has no `prefix` (its games
/// keep their own). Refuses a profile Steam can't run games with.
fn profile_prefix(catalog: &Catalog, name: &str, steam_root: &Path) -> Result<Option<Shared>> {
    let profile = catalog
        .profile(name)
        .with_context(|| format!("no profile `{name}`"))?;
    if profile.runner != Runner::Proton {
        bail!(
            "profile `{name}` runs `{}`; Steam's games need a `proton` profile",
            profile.runner
        );
    }
    let home = libgaki::core::config::home_dir()?;
    let Some(prefix) = profile
        .prefix
        .as_ref()
        .map(|prefix| libgaki::config::expand_tilde(prefix, &home))
    else {
        return Ok(None);
    };
    let build = profile
        .proton
        .as_ref()
        .and_then(|proton| proton.build.clone())
        .or_else(|| catalog.config.default_proton.clone())
        .unwrap_or_else(|| "experimental".to_owned());
    let mut dir = libgaki::core::utils::resolve_runner_opt(&build, steam_root)
        .with_context(|| format!("profile `{name}`'s Proton build `{build}` isn't installed"))?;
    if dir.is_file() {
        dir.pop();
    }
    let tool = shared_prefix::steam_tool_name(&dir).with_context(|| {
        format!(
            "can't tell Steam's name for profile `{name}`'s Proton build {}; without it, \
             Steam may run the games with another build, which converts the shared prefix",
            dir.display()
        )
    })?;
    Ok(Some(Shared {
        profile: name.to_owned(),
        prefix,
        tool: Some(tool),
    }))
}

/// Which prefix each game wants: with `own`, its own; with a default
/// profile, its `games.toml` profile's when that's a `proton` one, else the
/// default's; without either, none (prefixes stay as they are).
fn wants(
    catalog: &Catalog,
    default: Option<&str>,
    own: bool,
    steam_root: &Path,
) -> Result<BTreeMap<u32, Want>> {
    let games = shared_prefix::Host::locate()?.games()?;
    let mut wants = BTreeMap::new();
    if own {
        for game in &games {
            wants.insert(game.app_id, Want::Own);
        }
        return Ok(wants);
    }
    let Some(default) = default else {
        return Ok(wants);
    };
    let mut resolved: BTreeMap<String, Option<Shared>> = BTreeMap::new();
    let mut resolve = |name: &str| -> Result<Want> {
        if !resolved.contains_key(name) {
            resolved.insert(name.to_owned(), profile_prefix(catalog, name, steam_root)?);
        }
        Ok(match &resolved[name] {
            Some(shared) => Want::Shared(shared.clone()),
            None => Want::Own,
        })
    };
    if resolve(default)? == Want::Own {
        bail!("profile `{default}` has no `prefix` to share");
    }
    for game in &games {
        let own_profile = catalog
            .game(game.app_id)
            .and_then(|entry| entry.profile.as_deref())
            .and_then(|name| catalog.profile(name))
            .filter(|profile| profile.runner == Runner::Proton);
        let want = match own_profile {
            Some(profile) => resolve(&profile.name)?,
            None => resolve(default)?,
        };
        wants.insert(game.app_id, want);
    }
    Ok(wants)
}

/// The prefixes' plan for `wants`, from where the games are now.
fn plan_prefixes(
    wants: &BTreeMap<u32, Want>,
    replace: bool,
) -> Result<(shared_prefix::Host, shared_prefix::Plan)> {
    let host = shared_prefix::Host::locate()?;
    let games = host.games()?;
    let plan = host.plan(
        &games,
        |game: &Game| wants.get(&game.app_id).cloned(),
        replace,
    )?;
    Ok((host, plan))
}

/// `steam relocate [--layout gaki|steam] [--prefix PROFILE | --own-prefixes]
/// [--undo]`.
pub(crate) fn cmd_steam_relocate(
    layout: Option<SteamLayoutArg>,
    prefix: Option<String>,
    replace_prefixes: bool,
    own_prefixes: bool,
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

    let catalog = Catalog::load_user()?;
    let default_prefix = match (prefix, &catalog.config.steam_prefix) {
        (Some(name), _) if !catalog.config.experimental => bail!(
            "`--prefix {name}` is experimental: set `experimental = true` to use it \
             (`gaki-cli config set experimental true`)"
        ),
        (Some(name), _) => Some(name),
        (None, Some(name)) if !catalog.config.experimental => {
            tracing::warn!(
                "`steam_prefix = \"{name}\"` is experimental and `experimental` is off; \
                 leaving Steam's prefixes alone"
            );
            None
        }
        (None, name) => name.clone(),
    };
    let default_prefix = default_prefix.filter(|_| !own_prefixes);
    let wants = wants(
        &catalog,
        default_prefix.as_deref(),
        undo || own_prefixes,
        &host.root,
    )?;
    let (_, prefixes) = plan_prefixes(&wants, replace_prefixes)?;

    // Moving back, the prefixes and caches go first: the shader cache
    // returns into the library before the library returns.
    let places = host.plan_places(undo)?;
    let library = host.plan_library(&from, &to)?;
    for (game, kept) in &prefixes.kept {
        match kept {
            Kept::Own(path) => cli_println!(
                "{game}: keeps its own prefix {} (`--replace-prefixes` moves it aside)",
                path.display()
            ),
            Kept::Elsewhere(target) => cli_println!(
                "{game}: its prefix links to {}, not by gaki; left alone",
                target.display()
            ),
        }
    }
    if places.is_empty() && library.is_empty() && prefixes.is_empty() {
        cli_println!(
            "Nothing to do: the library is {to}, the caches, logs and prefixes are in place."
        );
        return Ok(());
    }
    if !library.is_empty() {
        cli_println!("library: {from} → {to}");
    }
    let (first, second) = if undo {
        (&places, &library)
    } else {
        (&library, &places)
    };
    if undo {
        print_prefix_steps(&prefixes);
    }
    for step in first.steps.iter().chain(&second.steps) {
        cli_println!("  {step}");
    }
    if !undo {
        print_prefix_steps(&prefixes);
    }
    if dry_run {
        return Ok(());
    }
    if libgaki::core::utils::is_steam_running() {
        bail!("Steam is running; quit it first");
    }
    if let Some((pid, name)) = libgaki::library::steam_home::busy(
        first
            .moving()
            .chain(second.moving())
            .chain(prefixes.moving()),
    ) {
        bail!("{name} (pid {pid}) uses files that would move; stop it first");
    }
    confirm_write("Go ahead? [y/N] ", yes)?;

    if undo {
        run_prefixes(&wants, replace_prefixes)?;
    }
    run(&host, first)?;
    // The places plan depends on where the library is now.
    let second = if undo {
        host.plan_library(&host.library()?, &to)?
    } else {
        host.plan_places(false)?
    };
    run(&host, &second)?;
    if !undo {
        // The prefixes are where the library is now.
        run_prefixes(&wants, replace_prefixes)?;
    }
    cli_println!("Done: the library is {to}.");
    Ok(())
}

fn print_prefix_steps(plan: &shared_prefix::Plan) {
    for step in &plan.steps {
        cli_println!("  {step}");
    }
}

fn run_prefixes(wants: &BTreeMap<u32, Want>, replace: bool) -> Result<()> {
    let (host, plan) = plan_prefixes(wants, replace)?;
    host.execute(&plan, |step| tracing::info!(%step, "prefix"))
}

fn run(host: &Host, plan: &Plan) -> Result<()> {
    host.execute(plan, |step| {
        if let Step::Move { from, to } = step {
            tracing::info!(from = %from.display(), to = %to.display(), "moving");
        }
    })
}
