//! `profiles/<name>.toml`: one runner, one prefix, and their environment.
//!
//! The format is libgameprofiles' (D9), shared with lootrs and sruman, so one
//! set of profile files serves all three. gaki launches `proton` and
//! `native`; the other runners' profiles load, and gaki leaves them to their
//! tools (`wine` and `umu` to lootrs, `slr` to sruman). [`validate_profile`] is libgameprofiles' check of the file plus what
//! a gaki launch needs.

pub use libgameprofiles::{
    Anticheat, LibOverride, Profile, ProtonSection, Runner, ShaderCache, SlrMode, SlrSection,
    SteamIntegration, profile::validate_dll_override,
};

use super::issue::{Issue, Issues, Subject, ValidationContext};

/// Whether gaki can launch games with `profile`: `proton` or `native`.
/// `wine` and `umu` are lootrs', `slr` is sruman's.
pub fn is_supported(profile: &Profile) -> bool {
    match profile.runner {
        Runner::Proton | Runner::Native => true,
        Runner::Wine | Runner::Umu | Runner::Slr => false,
    }
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
    } else if runner == Runner::Slr {
        issues.error(SLR_UNSUPPORTED);
    }
    issues.finish()
}

/// Why an `slr` profile doesn't launch in gaki.
pub const SLR_UNSUPPORTED: &str =
    "`slr` profiles run with a Steam Linux Runtime: use sruman; gaki can't launch them yet";
