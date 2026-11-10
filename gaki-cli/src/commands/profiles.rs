//! `profiles` command handlers.

use anyhow::Result;
use libgaki::config::{Runner, Severity};
use libgaki::ops::config::ProfileInfo;

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
