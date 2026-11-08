//! `profiles/<name>.toml`: one runner, one prefix, and their environment.
//!
//! The schema is lootrs' (D9): [`Profile`] and its parts come from
//! `liblootrs`, so one set of profile files serves both tools. gaki launches
//! `proton` and `native`; the lootrs-only fields (`dxvk`, `vkd3d`, `[wine]`,
//! `[umu]`) load, and gaki ignores them.
//! Each tool validates a profile for its own launches: gaki with
//! [`validate_profile`], lootrs with `Profile::validate`.

pub use liblootrs::model::{
    Anticheat, Profile, ProtonSection, Runner, ShaderCache, SteamIntegration,
};

use super::issue::{Issue, Issues, Subject, ValidationContext};

/// Whether gaki can launch games with `runner`: `wine` and `umu` are lootrs'.
pub fn is_supported(runner: Runner) -> bool {
    matches!(runner, Runner::Proton | Runner::Native)
}

/// What's wrong with `profile` for a gaki launch. A lootrs runner gets one
/// error and nothing else: its fields mean other things there.
pub fn validate_profile(profile: &Profile, ctx: &ValidationContext) -> Vec<Issue> {
    let mut issues = Issues::new(Subject::Profile(profile.name.clone()));
    let runner = profile.runner;
    let ignored = |field: &str| format!("`{field}` is ignored for runner `{runner}`");
    issues.name(&profile.name);

    if !is_supported(runner) {
        issues.error(format!(
            "runner `{runner}` is a lootrs runner; gaki supports `proton` and `native`"
        ));
        // The rest is lootrs' to judge: its fields mean other things there.
        return issues.finish();
    }

    if let Some(shader_cache) = &profile.shader_cache {
        issues.shader_cache(shader_cache, ctx.home);
    }
    let proton = runner == Runner::Proton;
    if let Some(prefix) = &profile.prefix {
        if proton {
            issues.absolute("prefix", prefix, ctx.home);
        } else {
            issues.warn(ignored("prefix"));
        }
    }
    match &profile.proton {
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
    if runner == Runner::Native && !profile.runner_args.is_empty() {
        issues.warn(ignored("runner_args"));
    }
    if !proton {
        if !profile.dll_overrides.is_empty() {
            issues.warn(ignored("dll_overrides"));
        }
        if !profile.anticheat.is_off() {
            issues.warn(ignored("[anticheat]"));
        }
    }

    for (dll, mode) in &profile.dll_overrides {
        if dll.is_empty() || dll.contains(['=', ';', ',']) {
            issues.error(format!("`dll_overrides` key `{dll}` is not a DLL name"));
        }
        if let Err(message) = validate_dll_override(mode) {
            issues.error(format!("`dll_overrides.{dll}`: {message}"));
        }
    }
    for wrapper in &profile.wrappers {
        if wrapper.trim().is_empty() {
            issues.error("`wrappers` must not contain empty commands");
        }
    }
    issues.env(&profile.env);
    issues.finish()
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
