//! `gaki-cli doctor`: can this machine run what the configuration asks for?
//!
//! `config check` reads the files; this looks at the host around them: the
//! Steam library, the Proton builds the profiles and `default_proton` name,
//! the wrappers, and the programs some settings need (`sruman` for `slr`
//! profiles and `sruman_steam`, `steam` for `steam = "host"`,
//! `fossilize_replay` for `shader_precache`), plus the stored login. Nothing
//! is started, written or fetched.

#[cfg(test)]
mod tests;

use std::{
    ffi::OsStr,
    fmt,
    path::{Path, PathBuf},
};

use crate::{
    config::{Catalog, Profile, Runner, Severity, SteamIntegration, expand_tilde},
    core::config::LauncherConfig,
    launch::stages::apply_settings::{SRUMAN, find_in_path},
};

/// How bad a finding is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// Nothing wrong.
    Ok,
    /// Works, but something may go wrong.
    Warn,
    /// Won't work.
    Error,
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Ok => "ok",
            Self::Warn => "warning",
            Self::Error => "error",
        })
    }
}

/// One result of a check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Short name of the check, like `proton` or `wrappers`.
    pub check: &'static str,
    pub level: Level,
    /// What was found.
    pub message: String,
    /// What to do about it.
    pub hint: Option<String>,
}

impl Finding {
    fn new(check: &'static str, level: Level, message: impl Into<String>) -> Self {
        Self {
            check,
            level,
            message: message.into(),
            hint: None,
        }
    }

    fn ok(check: &'static str, message: impl Into<String>) -> Self {
        Self::new(check, Level::Ok, message)
    }

    fn warn(check: &'static str, message: impl Into<String>) -> Self {
        Self::new(check, Level::Warn, message)
    }

    fn error(check: &'static str, message: impl Into<String>) -> Self {
        Self::new(check, Level::Error, message)
    }

    fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

/// The worst level among `findings`; `Ok` for none.
pub fn worst(findings: &[Finding]) -> Level {
    findings.iter().map(|f| f.level).max().unwrap_or(Level::Ok)
}

/// What the checks look things up in.
pub struct Host<'a> {
    /// Expands `~` in paths.
    pub home: &'a Path,
    /// `PATH`, for programs named without a directory.
    pub path: Option<&'a OsStr>,
    /// Finds a Proton build by name (`experimental`, `GE-Proton9-20`) in the
    /// Steam library and `compatibilitytools.d`.
    pub find_proton: &'a dyn Fn(&str) -> Option<PathBuf>,
}

/// Every check, for the user's configuration and this machine.
pub fn run() -> anyhow::Result<Vec<Finding>> {
    let catalog = Catalog::load_user()?;
    let home = crate::core::config::home_dir()?;
    let path = std::env::var_os("PATH");
    let library =
        PathBuf::from(LauncherConfig::from_config(&catalog.config, Some(&home)).steam_library_path);
    let find_proton = |name: &str| crate::core::utils::resolve_runner_opt(name, &library);
    let host = Host {
        home: &home,
        path: path.as_deref(),
        find_proton: &find_proton,
    };

    let mut findings = vec![config(&crate::ops::config::check()?)];
    findings.extend(check(&catalog, &host));
    #[cfg(feature = "install")]
    findings.extend(fossilize(
        &catalog,
        crate::ops::shaders::find_replayer().as_deref(),
    ));
    #[cfg(feature = "authentication")]
    findings.push(login(&crate::auth::stored_account()));
    Ok(findings)
}

/// The checks that need only the configuration and `host`.
pub fn check(catalog: &Catalog, host: &Host) -> Vec<Finding> {
    let mut findings = vec![library(catalog, host.home)];
    findings.extend(proton(catalog, host));
    findings.extend(wrappers(catalog, host));
    findings.extend(sruman(catalog, host.path));
    findings.extend(steam(catalog, host.path));
    findings
}

/// The files' own issues, counted; `config check` lists them.
fn config(report: &crate::ops::config::CheckReport) -> Finding {
    let errors = report.load_errors.len()
        + report
            .issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .count();
    let warnings = report
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Warning)
        .count();
    let hint = "`gaki-cli config check` lists them";
    match (errors, warnings) {
        (0, 0) => Finding::ok("config", "the config files have no problems"),
        (0, n) => Finding::warn(
            "config",
            format!("{} in the config files", plural(n, "warning")),
        )
        .hint(hint),
        (n, 0) => Finding::error(
            "config",
            format!("{} in the config files", plural(n, "error")),
        )
        .hint(hint),
        (e, w) => Finding::error(
            "config",
            format!(
                "{} and {} in the config files",
                plural(e, "error"),
                plural(w, "warning")
            ),
        )
        .hint(hint),
    }
}

/// The Steam library gaki installs into: there, a directory, writable.
fn library(catalog: &Catalog, home: &Path) -> Finding {
    let dir =
        PathBuf::from(LauncherConfig::from_config(&catalog.config, Some(home)).steam_library_path);
    let shown = dir.display();
    match std::fs::metadata(&dir) {
        Err(_) => Finding::warn("library", format!("{shown} doesn't exist"))
            .hint("create it, or point `steam_library` in config.toml at a library (`gaki-cli libraries`)"),
        Ok(meta) if !meta.is_dir() => Finding::error("library", format!("{shown} isn't a directory"))
            .hint("point `steam_library` in config.toml at a directory"),
        Ok(_) if rustix::fs::access(&dir, rustix::fs::Access::WRITE_OK).is_err() => {
            Finding::error("library", format!("{shown} isn't writable"))
                .hint("installs and updates write there; fix its permissions or pick another library")
        }
        Ok(_) => Finding::ok("library", shown.to_string()),
    }
}

/// Each `proton` profile's build, and `default_proton` (what Windows games
/// without a profile run, and `proton` profiles without a build).
fn proton(catalog: &Catalog, host: &Host) -> Vec<Finding> {
    let mut findings = Vec::new();
    let default = catalog
        .config
        .default_proton
        .clone()
        .unwrap_or_else(|| crate::core::config::DEFAULT_PROTON.to_owned());
    findings.push(match find_build(&default, host) {
        Ok(dir) => Finding::ok("proton", format!("default `{default}`: {}", dir.display())),
        Err(problem) => Finding::warn("proton", format!("default `{default}` {problem}")).hint(
            "Windows games without a profile build run it: `gaki-cli proton install <name>`, or set \
             `default_proton` to one `gaki-cli config protons` lists",
        ),
    });
    for profile in proton_profiles(catalog) {
        let Some(build) = profile.proton.as_ref().and_then(|p| p.build.as_deref()) else {
            continue;
        };
        findings.push(match find_build(build, host) {
            Ok(dir) => Finding::ok(
                "proton",
                format!("profile `{}`: {}", profile.name, dir.display()),
            ),
            Err(problem) => Finding::error(
                "proton",
                format!("profile `{}`: `{build}` {problem}", profile.name),
            )
            .hint("`gaki-cli config protons` lists the installed builds"),
        });
    }
    findings
}

fn proton_profiles(catalog: &Catalog) -> impl Iterator<Item = &Profile> {
    catalog
        .profiles
        .iter()
        .filter(|p| p.runner == Runner::Proton)
}

/// `build`'s directory, which must hold the `proton` script.
fn find_build(build: &str, host: &Host) -> Result<PathBuf, &'static str> {
    let is_path = crate::config::ProtonSection {
        build: Some(build.to_owned()),
    }
    .build_is_path();
    let dir = if is_path {
        Some(expand_tilde(Path::new(build), host.home)).filter(|dir| dir.exists())
    } else {
        (host.find_proton)(build)
    };
    let dir = dir.ok_or("isn't installed")?;
    // A build may also be named by its `proton` script.
    if dir.is_file() || dir.join("proton").is_file() {
        Ok(dir)
    } else {
        Err("has no `proton` script")
    }
}

/// The wrappers of the profiles gaki applies (`proton`, `native`; sruman
/// applies an `slr` profile's) and of the games.
fn wrappers(catalog: &Catalog, host: &Host) -> Vec<Finding> {
    let profiles = catalog
        .profiles
        .iter()
        .filter(|p| crate::config::is_supported(p))
        .map(|p| (format!("profile `{}`", p.name), &p.wrappers));
    let games = catalog.games.iter().map(|g| {
        let name = g.name.clone().unwrap_or_else(|| g.app_id.to_string());
        (format!("game `{name}`"), &g.wrappers)
    });
    let mut findings = Vec::new();
    let mut found = 0;
    for (owner, wrappers) in profiles.chain(games) {
        for wrapper in wrappers {
            let Some(program) = wrapper.split_whitespace().next() else {
                continue;
            };
            if find_program(program, host).is_some() {
                found += 1;
            } else {
                findings.push(
                    Finding::error("wrappers", format!("{owner}: `{program}` not found"))
                        .hint("install it, or remove it from `wrappers`"),
                );
            }
        }
    }
    if findings.is_empty() && found > 0 {
        findings.push(Finding::ok(
            "wrappers",
            format!("{} found", plural(found, "wrapper")),
        ));
    }
    findings
}

/// `program` as a launch runs it: a path (`~/` expanded) or a name in `PATH`.
fn find_program(program: &str, host: &Host) -> Option<PathBuf> {
    let expanded = match program.strip_prefix("~/") {
        Some(rest) => host.home.join(rest),
        None => PathBuf::from(program),
    };
    if expanded.components().count() > 1 {
        expanded.is_file().then_some(expanded)
    } else {
        find_in_path(program, host.path)
    }
}

/// sruman, for `slr` profiles and `sruman_steam`.
fn sruman(catalog: &Catalog, path: Option<&OsStr>) -> Option<Finding> {
    let slr: Vec<&str> = catalog
        .profiles
        .iter()
        .filter(|p| p.runner == Runner::Slr)
        .map(|p| p.name.as_str())
        .collect();
    if slr.is_empty() && !catalog.config.sruman_steam {
        return None;
    }
    if let Some(found) = find_in_path(SRUMAN, path) {
        return Some(Finding::ok("sruman", found.display().to_string()));
    }
    let hint = "install sruman (https://github.com/Gigas002/sruman)";
    Some(if slr.is_empty() {
        Finding::warn(
            "sruman",
            "`sruman_steam` is on, but there's no `sruman` in PATH: Steam keeps its own runtimes",
        )
        .hint(hint)
    } else {
        Finding::error(
            "sruman",
            format!(
                "no `sruman` in PATH, which `slr` profiles need: {}",
                quoted(&slr)
            ),
        )
        .hint(hint)
    })
}

/// The Steam client, for `steam = "host"`: gaki starts it when needed.
fn steam(catalog: &Catalog, path: Option<&OsStr>) -> Option<Finding> {
    let host = SteamIntegration::Host;
    let mut users: Vec<String> = catalog
        .profiles
        .iter()
        .filter(|p| p.steam == Some(host))
        .map(|p| format!("profile `{}`", p.name))
        .collect();
    users.extend(
        catalog
            .games
            .iter()
            .filter(|g| g.steam == Some(host))
            .map(|g| {
                format!(
                    "game `{}`",
                    g.name.clone().unwrap_or_else(|| g.app_id.to_string())
                )
            }),
    );
    if users.is_empty() {
        return None;
    }
    Some(match find_in_path("steam", path) {
        Some(found) => Finding::ok("steam", found.display().to_string()),
        None => Finding::error(
            "steam",
            format!(
                "no `steam` in PATH, which `steam = \"host\"` needs: {}",
                users.join(", ")
            ),
        )
        .hint("install the Steam client, or set `steam` to `auto` or `none`"),
    })
}

/// `fossilize_replay`, which compiles the pre-cached shaders.
#[cfg(feature = "install")]
fn fossilize(catalog: &Catalog, replayer: Option<&Path>) -> Option<Finding> {
    if !catalog.config.shader_precache {
        return None;
    }
    Some(match replayer {
        Some(found) => Finding::ok("shaders", format!("fossilize_replay: {}", found.display())),
        None => Finding::warn(
            "shaders",
            "no `fossilize_replay`: pre-cached shaders are fetched but not compiled",
        )
        .hint("install Steam or fossilize, or set `shader_precache = false`"),
    })
}

/// Whether a login is stored. Read without the keyring or the network.
#[cfg(feature = "authentication")]
fn login(stored: &crate::auth::StoredAccount) -> Finding {
    use crate::auth::StoredAccount;
    match stored {
        StoredAccount::Account(name) => Finding::ok("login", format!("signed in as {name}")),
        StoredAccount::Unknown => Finding::ok("login", "a session is stored"),
        StoredAccount::None => Finding::warn("login", "not signed in")
            .hint("library, installs and launches need it: `gaki-cli login`"),
    }
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{n} {word}s")
    }
}

fn quoted(names: &[&str]) -> String {
    names
        .iter()
        .map(|n| format!("`{n}`"))
        .collect::<Vec<_>>()
        .join(", ")
}
