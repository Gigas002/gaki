//! `profiles/<name>.toml`: one runner, one prefix, and their environment.
//!
//! The format is libgameprofiles' (D9), shared with lootrs and sruman, so one
//! set of profile files serves all three. gaki launches `proton` and
//! `native` itself, and native games with an `slr` profile through sruman
//! (`sruman execute -p <profile>`); `wine` and `umu` profiles load, and gaki
//! leaves them to lootrs. [`validate_profile`] is libgameprofiles' check of
//! the file plus what a gaki launch needs.

pub use libgameprofiles::{
    Anticheat, LibOverride, Profile, ProtonSection, Runner, ShaderCache, SlrMode, SlrSection,
    SteamIntegration, profile::validate_dll_override,
};

use super::issue::{Issue, Issues, Subject, ValidationContext};

/// Whether gaki makes and edits profiles like `profile`: `proton` or
/// `native`. `wine` and `umu` are lootrs', `slr` is sruman's.
pub fn is_supported(profile: &Profile) -> bool {
    match profile.runner {
        Runner::Proton | Runner::Native => true,
        Runner::Wine | Runner::Umu | Runner::Slr => false,
    }
}

/// Whether gaki can launch games with `profile`: its own runners, and `slr`
/// through sruman.
pub fn launches(profile: &Profile) -> bool {
    is_supported(profile) || profile.runner == Runner::Slr
}

/// What's wrong with `profile` for a gaki launch: the file's own issues, and
/// a runner gaki doesn't launch.
pub fn validate_profile(profile: &Profile, ctx: &ValidationContext) -> Vec<Issue> {
    let mut issues = Issues::new(Subject::Profile(profile.name.clone()));
    issues.extend(
        profile
            .validate(ctx.home)
            .into_iter()
            .map(|issue| issue.map_subject(Subject::from)),
    );
    let runner = profile.runner;
    if matches!(runner, Runner::Wine | Runner::Umu) {
        issues.error(format!(
            "runner `{runner}` is a lootrs runner; gaki supports `proton` and `native`"
        ));
    }
    issues.finish()
}
