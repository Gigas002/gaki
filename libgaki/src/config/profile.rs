//! `profiles/<name>.toml`: one runner, one prefix, and their environment.
//!
//! The schema follows lootrs' `Profile` field for field (D9), so one set of
//! profile files can serve both tools. Fields only lootrs uses (`dxvk`,
//! `vkd3d`, `[wine]`, `[umu]`) are accepted and ignored.

use std::{collections::BTreeMap, fmt, path::PathBuf};

use serde::{Deserialize, Serialize};

use super::issue::{Issue, Issues, Subject, ValidationContext};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    /// Must match the file stem.
    pub name: String,
    pub runner: Runner,
    /// The Wine prefix (`STEAM_COMPAT_DATA_PATH`), shared by every game using
    /// this profile. `None` gives each game its own Steam `compatdata/<app id>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<PathBuf>,
    /// Extra arguments for the runner itself, before the game's own.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runner_args: Vec<String>,
    /// Commands the launch is wrapped in, outermost first (`mangohud`,
    /// `gamemoderun`, ...).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wrappers: Vec<String>,
    /// Turn on the runner's own logging.
    #[serde(default, skip_serializing_if = "is_false")]
    pub debug: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    /// DLL name to Wine override mode (`n`, `b`, `n,b`, ... or `""` to disable).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dll_overrides: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proton: Option<ProtonSection>,
    #[serde(default, skip_serializing_if = "Anticheat::is_off")]
    pub anticheat: Anticheat,
}

/// How a profile runs the game. `wine` and `umu` are lootrs runners: they
/// parse, so shared profile directories load, but gaki can't launch them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Runner {
    Proton,
    Native,
    Wine,
    Umu,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtonSection {
    /// A Steam-installed build name (`experimental`, `9.0`, ...) or the path of
    /// a Proton directory. `None` is the default build.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<String>,
}

/// Anti-cheat runtimes to make available to the game.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anticheat {
    #[serde(default)]
    pub battleye: bool,
    #[serde(default)]
    pub eac: bool,
}

impl Anticheat {
    pub fn is_off(&self) -> bool {
        !self.battleye && !self.eac
    }
}

fn is_false(value: &bool) -> bool {
    !value
}

impl Runner {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Proton => "proton",
            Self::Native => "native",
            Self::Wine => "wine",
            Self::Umu => "umu",
        }
    }

    /// Whether gaki can launch games with this runner.
    pub fn is_supported(self) -> bool {
        matches!(self, Self::Proton | Self::Native)
    }
}

impl fmt::Display for Runner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl ProtonSection {
    /// Whether `build` names a directory rather than a Steam build.
    pub fn build_is_path(&self) -> bool {
        self.build
            .as_deref()
            .is_some_and(|build| build.starts_with(['/', '~']) || build.contains('/'))
    }
}

impl Profile {
    pub fn new(name: impl Into<String>, runner: Runner) -> Self {
        Self {
            name: name.into(),
            runner,
            prefix: None,
            runner_args: Vec::new(),
            wrappers: Vec::new(),
            debug: false,
            env: BTreeMap::new(),
            dll_overrides: BTreeMap::new(),
            proton: None,
            anticheat: Anticheat::default(),
        }
    }

    pub fn validate(&self, ctx: &ValidationContext) -> Vec<Issue> {
        let mut issues = Issues::new(Subject::Profile(self.name.clone()));
        let runner = self.runner;
        let ignored = |field: &str| format!("`{field}` is ignored for runner `{runner}`");
        issues.name(&self.name);

        if !runner.is_supported() {
            issues.error(format!(
                "runner `{runner}` is a lootrs runner; gaki supports `proton` and `native`"
            ));
        }

        let proton = runner == Runner::Proton;
        if let Some(prefix) = &self.prefix {
            if proton {
                issues.absolute("prefix", prefix, ctx.home);
            } else {
                issues.warn(ignored("prefix"));
            }
        }
        match &self.proton {
            Some(section) if proton => {
                if section.build.as_deref() == Some("") {
                    issues.error("`proton.build` must not be empty");
                } else if section.build_is_path() {
                    let build = section.build.as_deref().unwrap_or_default();
                    issues.absolute("proton.build", build.as_ref(), ctx.home);
                }
            }
            Some(_) => issues.warn(ignored("[proton]")),
            None => {}
        }
        if runner == Runner::Native && !self.runner_args.is_empty() {
            issues.warn(ignored("runner_args"));
        }
        if !proton {
            if !self.dll_overrides.is_empty() {
                issues.warn(ignored("dll_overrides"));
            }
            if !self.anticheat.is_off() {
                issues.warn(ignored("[anticheat]"));
            }
        }

        for (dll, mode) in &self.dll_overrides {
            if dll.is_empty() || dll.contains(['=', ';', ',']) {
                issues.error(format!("`dll_overrides` key `{dll}` is not a DLL name"));
            }
            if let Err(message) = validate_dll_override(mode) {
                issues.error(format!("`dll_overrides.{dll}`: {message}"));
            }
        }
        for wrapper in &self.wrappers {
            if wrapper.trim().is_empty() {
                issues.error("`wrappers` must not contain empty commands");
            }
        }
        issues.env(&self.env);
        issues.finish()
    }
}

/// A Wine override mode: comma-separated `n`/`native`/`b`/`builtin`, or `""`
/// to disable the DLL.
pub fn validate_dll_override(mode: &str) -> Result<(), String> {
    if mode.is_empty() {
        return Ok(());
    }
    for part in mode.split(',') {
        if !matches!(part.trim(), "n" | "b" | "native" | "builtin") {
            return Err(format!(
                "override `{mode}` is not valid; use `n`, `b`, `n,b`, `b,n`, or `\"\"` to disable"
            ));
        }
    }
    Ok(())
}
