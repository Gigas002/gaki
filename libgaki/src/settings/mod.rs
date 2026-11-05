//! Resolves the TOML configuration into the settings of one launch.
//!
//! Layers, later ones winning: built-in defaults, `config.toml`, the profile,
//! the game's `games.toml` entry, and the command line ([`Overrides`]). Both
//! binaries resolve through [`resolve`], so a game launches the same way from
//! either.
//!
//! The **profile** is the first one named by the command line, the game entry,
//! or `default_profile`. The **platform** is the first one set by the command
//! line, the game entry, an explicitly chosen profile's runner (`proton` means
//! Windows, `native` means Linux), or `default_platform`; when none sets it,
//! it stays `None` and the launch picks from the builds the game has. An
//! explicitly chosen profile that contradicts the platform is an error; the
//! default profile is just left out for that launch.

#[cfg(test)]
mod tests;

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use crate::config::{Anticheat, Catalog, Platform, Profile, ProtonSection, Runner, expand_tilde};

/// The command-line layer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Overrides {
    /// Profile to run with, instead of the game's or the default.
    pub profile: Option<String>,
    pub platform: Option<Platform>,
    /// Proton build (a Steam build name or a path), overriding the profile's.
    /// Implies the Windows build.
    pub proton: Option<String>,
    /// Launch script, instead of the game's.
    pub script: Option<PathBuf>,
    /// Run without any launch script.
    pub no_script: bool,
}

/// Which Proton to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtonBuild {
    /// gaki's default build.
    Default,
    /// A Steam-installed build, by name (`experimental`, `9.0`, ...).
    Steam(String),
    /// A Proton directory.
    Path(PathBuf),
}

/// The Proton half of a launch: present only when the profile runs Proton.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtonSettings {
    pub build: ProtonBuild,
    /// The shared prefix, `~` expanded. `None` is the game's own Steam
    /// `compatdata/<app id>`.
    pub prefix: Option<PathBuf>,
    pub dll_overrides: BTreeMap<String, String>,
    pub anticheat: Anticheat,
}

/// Everything one launch of one game needs from the configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchSettings {
    pub app_id: u32,
    /// The profile used, by name.
    pub profile: Option<String>,
    /// `None` lets the launch pick from the game's builds.
    pub platform: Option<Platform>,
    /// Set when the launch may run under Proton. With `platform` unset it
    /// applies only if the launch picks the Windows build.
    pub proton: Option<ProtonSettings>,
    pub runner_args: Vec<String>,
    /// Outermost first.
    pub wrappers: Vec<String>,
    pub debug: bool,
    /// The profile's variables, then the game's on top.
    pub env: BTreeMap<String, String>,
    /// Arguments passed to the game.
    pub args: Vec<String>,
    /// The launch script, `~` expanded.
    pub script: Option<PathBuf>,
    pub cloud_sync: bool,
}

/// Why a game's settings can't be resolved.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SettingsError {
    #[error("profile `{0}` doesn't exist")]
    UnknownProfile(String),
    #[error("profile `{profile}` uses runner `{runner}`, which gaki doesn't support")]
    UnsupportedRunner { profile: String, runner: Runner },
    #[error(
        "profile `{profile}` runs the {runner_platform} build, but the {platform} build was asked for"
    )]
    PlatformConflict {
        profile: String,
        runner_platform: Platform,
        platform: Platform,
    },
    #[error("a Proton build was given, but the {0} build was asked for")]
    ProtonOnLinux(Platform),
}

/// Where a layer's value came from, for the "explicit vs default" rules.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Origin {
    Explicit,
    Default,
}

/// Resolve `app_id`'s settings. `home` expands `~` in paths.
pub fn resolve(
    catalog: &Catalog,
    app_id: u32,
    overrides: &Overrides,
    home: &Path,
) -> Result<LaunchSettings, SettingsError> {
    let config = &catalog.config;
    let game = catalog.game(app_id);

    let named = overrides
        .profile
        .as_deref()
        .or_else(|| game.and_then(|game| game.profile.as_deref()))
        .map(|name| (name, Origin::Explicit))
        .or_else(|| {
            config
                .default_profile
                .as_deref()
                .map(|name| (name, Origin::Default))
        });
    let mut profile = match named {
        Some((name, origin)) => {
            let profile = catalog
                .profile(name)
                .ok_or_else(|| SettingsError::UnknownProfile(name.to_owned()))?;
            if !profile.runner.is_supported() {
                return Err(SettingsError::UnsupportedRunner {
                    profile: profile.name.clone(),
                    runner: profile.runner,
                });
            }
            Some((profile, origin))
        }
        None => None,
    };

    // The command line's Proton means Windows, unless it says otherwise.
    let cli_platform = match (&overrides.proton, overrides.platform) {
        (Some(_), Some(Platform::Linux)) => {
            return Err(SettingsError::ProtonOnLinux(Platform::Linux));
        }
        (Some(_), None) => Some(Platform::Windows),
        (_, platform) => platform,
    };
    let platform = cli_platform
        .or_else(|| game.and_then(|game| game.platform))
        .or_else(|| match profile {
            Some((profile, Origin::Explicit)) => Some(runner_platform(profile.runner)),
            _ => None,
        })
        .or(config.default_platform);

    if let (Some((chosen, origin)), Some(platform)) = (profile, platform) {
        let runs = runner_platform(chosen.runner);
        if runs != platform {
            match origin {
                Origin::Explicit => {
                    return Err(SettingsError::PlatformConflict {
                        profile: chosen.name.clone(),
                        runner_platform: runs,
                        platform,
                    });
                }
                Origin::Default => profile = None,
            }
        }
    }
    let profile = profile.map(|(profile, _)| profile);

    let proton = proton_settings(profile, overrides.proton.as_deref(), platform, home);
    let mut env = profile.map(|p| p.env.clone()).unwrap_or_default();
    let mut args = Vec::new();
    let mut script = None;
    if let Some(game) = game {
        env.extend(game.env.clone());
        args.clone_from(&game.args);
        script.clone_from(&game.script);
    }
    if overrides.script.is_some() {
        script.clone_from(&overrides.script);
    }
    if overrides.no_script {
        script = None;
    }

    Ok(LaunchSettings {
        app_id,
        profile: profile.map(|p| p.name.clone()),
        platform,
        proton,
        runner_args: profile.map(|p| p.runner_args.clone()).unwrap_or_default(),
        wrappers: profile.map(|p| p.wrappers.clone()).unwrap_or_default(),
        debug: profile.is_some_and(|p| p.debug),
        env,
        args,
        script: script.map(|script| expand_tilde(&script, home)),
        cloud_sync: config.cloud_sync,
    })
}

/// The build a runner runs. Only called for supported runners.
fn runner_platform(runner: Runner) -> Platform {
    match runner {
        Runner::Native => Platform::Linux,
        _ => Platform::Windows,
    }
}

/// The Proton settings, when the launch may run under Proton: a proton
/// profile, or a Proton build from the command line.
fn proton_settings(
    profile: Option<&Profile>,
    cli_build: Option<&str>,
    platform: Option<Platform>,
    home: &Path,
) -> Option<ProtonSettings> {
    if platform == Some(Platform::Linux) {
        return None;
    }
    let proton_profile = profile.filter(|p| p.runner == Runner::Proton);
    if proton_profile.is_none() && cli_build.is_none() {
        return None;
    }
    let section = proton_profile.and_then(|p| p.proton.as_ref());
    let build = match cli_build {
        Some(build) => proton_build(build, home),
        None => section
            .and_then(|section: &ProtonSection| section.build.as_deref())
            .map_or(ProtonBuild::Default, |build| proton_build(build, home)),
    };
    Some(ProtonSettings {
        build,
        prefix: proton_profile
            .and_then(|p| p.prefix.as_deref())
            .map(|prefix| expand_tilde(prefix, home)),
        dll_overrides: proton_profile
            .map(|p| p.dll_overrides.clone())
            .unwrap_or_default(),
        anticheat: proton_profile.map(|p| p.anticheat).unwrap_or_default(),
    })
}

fn proton_build(build: &str, home: &Path) -> ProtonBuild {
    let section = ProtonSection {
        build: Some(build.to_owned()),
    };
    if section.build_is_path() {
        ProtonBuild::Path(expand_tilde(Path::new(build), home))
    } else {
        ProtonBuild::Steam(build.to_owned())
    }
}
