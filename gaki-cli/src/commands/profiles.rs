//! `profiles` command handlers.

use anyhow::Result;
use libgaki::config::{Runner, Severity};
use libgaki::ops::config::{EnvChange, ProfileChange, ProfileInfo};

use super::common::confirm_write;
use super::config::change;
use crate::cli::ProfileArgs;

/// What a profile runs, for the listing.
fn runs(info: &ProfileInfo) -> String {
    let profile = &info.profile;
    match profile.runner {
        Runner::Proton => {
            let build = match profile.proton.as_ref().and_then(|p| p.build.as_deref()) {
                // A directory: its name says enough here; `show` has the path.
                Some(build) if profile.proton.as_ref().is_some_and(|p| p.build_is_path()) => {
                    std::path::Path::new(build)
                        .file_name()
                        .map_or(build.to_owned(), |n| n.to_string_lossy().into_owned())
                }
                Some(build) => build.to_owned(),
                None => "default build".to_owned(),
            };
            format!("proton ({build})")
        }
        runner => runner.to_string(),
    }
}

fn prefix(info: &ProfileInfo) -> String {
    match (&info.profile.prefix, info.profile.runner) {
        (Some(prefix), _) => prefix.display().to_string(),
        (None, Runner::Proton) => "per-game compatdata".into(),
        (None, _) => "-".into(),
    }
}

fn has_errors(info: &ProfileInfo) -> bool {
    info.issues.iter().any(|i| i.severity == Severity::Error)
}

/// `profiles list`.
pub(crate) fn cmd_profiles_list() -> Result<()> {
    let (dir, infos, errors) = libgaki::ops::config::profiles()?;
    if infos.is_empty() && errors.is_empty() {
        cli_println!("No profiles in {}.", dir.display());
        cli_println!("See `examples/profiles/` in the gaki repository for a starting point.");
        return Ok(());
    }
    let name_width = infos
        .iter()
        .map(|i| i.profile.name.len() + 2)
        .max()
        .unwrap_or(0)
        .max(4);
    let runs_width = infos
        .iter()
        .map(|i| runs(i).len())
        .max()
        .unwrap_or(0)
        .max(6);
    cli_println!(
        "{:name_width$}  {:runs_width$}  {:5}  PREFIX",
        "NAME",
        "RUNNER",
        "GAMES"
    );
    for info in &infos {
        let mut name = info.profile.name.clone();
        if info.is_default {
            name.push_str(" *");
        }
        let mut line = format!(
            "{name:name_width$}  {:runs_width$}  {:5}  {}",
            runs(info),
            info.games.len(),
            prefix(info)
        );
        if has_errors(info) {
            line.push_str("  (has errors: `gaki-cli profiles show`)");
        }
        cli_println!("{line}");
    }
    if infos.iter().any(|i| i.is_default) {
        cli_println!("* default_profile");
    }
    for error in &errors {
        cli_println!("error: {error}");
    }
    Ok(())
}

/// `profiles show NAME`.
pub(crate) fn cmd_profiles_show(name: &str) -> Result<()> {
    let info = libgaki::ops::config::profile(name)?;
    cli_println!("# {}", info.path.display());
    match std::fs::read_to_string(&info.path) {
        Ok(text) => cli_print!("{}", text.trim_end().to_owned() + "\n"),
        Err(_) => cli_println!("(the file could not be read)"),
    }
    cli_println!();
    if info.is_default {
        cli_println!("This is the default profile.");
    }
    if info.games.is_empty() {
        cli_println!("No games name it in games.toml.");
    } else {
        let games: Vec<String> = info.games.iter().map(ToString::to_string).collect();
        cli_println!("Used by: {}", games.join(", "));
    }
    for issue in &info.issues {
        cli_println!("{}: {}", issue.severity, issue.message);
    }
    Ok(())
}

/// `KEY=VALUE` texts as a map.
fn vars(texts: &[String]) -> Result<std::collections::BTreeMap<String, String>> {
    texts
        .iter()
        .map(|text| libgaki::ops::config::parse_env_var(text))
        .collect()
}

/// The flags as a [`ProfileChange`].
pub(crate) fn profile_change(runner: Option<Runner>, args: ProfileArgs) -> Result<ProfileChange> {
    let ProfileArgs {
        prefix,
        no_prefix,
        proton,
        no_proton,
        steam,
        no_steam,
        wrappers,
        no_wrappers,
        runner_args,
        no_runner_args,
        debug,
        no_debug,
        env,
        unset_env,
        no_env,
        dlls,
        unset_dlls,
        no_dlls,
        shader_cache,
        no_shader_cache,
    } = args;
    Ok(ProfileChange {
        runner,
        prefix: change(no_prefix, prefix),
        proton_build: change(no_proton, proton),
        steam: change(no_steam, steam.map(Into::into)),
        wrappers: change(no_wrappers, (!wrappers.is_empty()).then_some(wrappers)),
        runner_args: change(
            no_runner_args,
            (!runner_args.is_empty()).then_some(runner_args),
        ),
        debug: (debug || no_debug).then_some(debug),
        env: EnvChange {
            clear: no_env,
            unset: unset_env,
            set: vars(&env)?,
        },
        dll_overrides: EnvChange {
            clear: no_dlls,
            unset: unset_dlls,
            set: vars(&dlls)?,
        },
        shader_cache: change(
            no_shader_cache,
            shader_cache.map(|text| libgaki::config::ShaderCache::parse(&text)),
        ),
    })
}

fn print_warnings(info: &ProfileInfo) {
    for issue in &info.issues {
        cli_println!("{}: {}", issue.severity, issue.message);
    }
}

/// `profiles new NAME`.
pub(crate) fn cmd_profiles_new(name: &str, runner: Runner, args: ProfileArgs) -> Result<()> {
    let change = profile_change(None, args)?;
    let info = libgaki::ops::config::create_profile(name, runner, &change)?;
    cli_println!("Created {}.", info.path.display());
    print_warnings(&info);
    Ok(())
}

/// `profiles set NAME`.
pub(crate) fn cmd_profiles_set(
    name: &str,
    runner: Option<Runner>,
    args: ProfileArgs,
) -> Result<()> {
    let change = profile_change(runner, args)?;
    let (info, changed) = libgaki::ops::config::update_profile(name, &change)?;
    if changed {
        cli_println!("Updated {}.", info.path.display());
    } else {
        cli_println!("Profile `{name}` already has these settings.");
    }
    print_warnings(&info);
    Ok(())
}

/// `profiles delete NAME`.
pub(crate) fn cmd_profiles_delete(name: &str, force: bool, yes: bool) -> Result<()> {
    let info = libgaki::ops::config::profile(name)?;
    if !force && (info.is_default || !info.games.is_empty()) {
        // Refused before asking; nothing is deleted.
        libgaki::ops::config::delete_profile(name, false)?;
    }
    confirm_write(
        &format!("Delete {}? Its prefix stays. [y/N] ", info.path.display()),
        yes,
    )?;
    let info = libgaki::ops::config::delete_profile(name, force)?;
    cli_println!("Deleted {}.", info.path.display());
    if info.is_default {
        cli_println!("`default_profile` still names it: `gaki-cli config unset default_profile`.");
    }
    if !info.games.is_empty() {
        let games: Vec<String> = info.games.iter().map(ToString::to_string).collect();
        cli_println!(
            "These games still name it in games.toml: {} (`gaki-cli config game <id> --profile ...`).",
            games.join(", ")
        );
    }
    Ok(())
}
