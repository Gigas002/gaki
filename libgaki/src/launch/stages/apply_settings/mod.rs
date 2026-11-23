//! Applies the runner-independent parts of the resolved settings to the
//! command: the `GAKI_*` variables, the profile's and game's environment, the
//! game's arguments, sruman for an `slr` profile, and the wrappers.
//!
//! Runs after `PreflightStage`, which checks the runner binary itself, and
//! right before the spawn.

use std::path::PathBuf;

use async_trait::async_trait;

use crate::infra::runners::CommandSpec;
use crate::launch::pipeline::{LaunchError, PipelineContext, PipelineStage};
use crate::settings::LaunchSettings;

pub struct ApplySettingsStage;

#[async_trait]
impl PipelineStage for ApplySettingsStage {
    fn name(&self) -> &str {
        "ApplySettings"
    }

    async fn execute(&self, ctx: &mut PipelineContext) -> std::result::Result<(), LaunchError> {
        let name = ctx.app.as_ref().map(|a| a.name.clone()).unwrap_or_default();
        let game_dir = ctx.resolved_install_dir.clone().or_else(|| {
            ctx.app
                .as_ref()
                .and_then(|a| a.install_path.as_ref().map(PathBuf::from))
        });
        let app_id = ctx.app_id;
        let Some(spec) = ctx.command_spec.as_mut() else {
            return Ok(());
        };
        if let Some(profile) = ctx.settings.as_ref().and_then(|s| s.sruman.as_deref())
            && find_in_path(SRUMAN, std::env::var_os("PATH").as_deref()).is_none()
        {
            return Err(LaunchError::new(
                crate::launch::pipeline::LaunchErrorKind::Validation,
                format!(
                    "profile `{profile}` runs the game in a Steam Linux Runtime through sruman, \
                     which isn't installed (no `{SRUMAN}` in PATH)"
                ),
            ));
        }
        crate::launch::game_env::export(spec, app_id, &name, game_dir.as_deref());
        if let Some(settings) = &ctx.settings {
            if let Some(dir) = settings.shader_cache_dir() {
                let runner = if settings.proton.is_some() {
                    crate::config::Runner::Proton
                } else {
                    crate::config::Runner::Native
                };
                if let Err(e) = crate::config::ShaderCache::create_dirs(&dir, runner) {
                    // The drivers fall back to their own defaults.
                    tracing::warn!(dir = %dir.display(), error = %e, "can't create the shader cache");
                } else {
                    shader_cache_env(spec, &dir, |key| std::env::var_os(key).is_some());
                }
            }
            apply(settings, spec);
        }
        Ok(())
    }
}

/// Point DXVK, VKD3D-Proton and the drivers at `dir`, the game's shader
/// cache, and hand it to Proton as `STEAM_COMPAT_SHADER_PATH`; when shader
/// pre-caching fetched Steam's transcoded media into it, as
/// `STEAM_COMPAT_TRANSCODED_MEDIA_PATH` too. A variable
/// the runner or a fixup already set, or one gaki's own environment has
/// (`inherited`), is left alone; the profile's and the game's `env` are
/// applied after this, so they win too.
pub fn shader_cache_env(
    spec: &mut CommandSpec,
    dir: &std::path::Path,
    inherited: impl Fn(&str) -> bool,
) {
    let proton = spec.env.contains_key("STEAM_COMPAT_DATA_PATH");
    let runner = if proton {
        crate::config::Runner::Proton
    } else {
        crate::config::Runner::Native
    };
    let mut vars = crate::config::ShaderCache::env(dir, runner);
    if proton {
        vars.push(("STEAM_COMPAT_SHADER_PATH", dir.to_path_buf()));
        // Proton's media converter reads `transcoded_{video,audio}.foz` there.
        if ["transcoded_video.foz", "transcoded_audio.foz"]
            .iter()
            .any(|f| dir.join(f).is_file())
        {
            vars.push(("STEAM_COMPAT_TRANSCODED_MEDIA_PATH", dir.to_path_buf()));
        }
    }
    for (key, value) in vars {
        if !inherited(key) {
            spec.env
                .entry(key.to_owned())
                .or_insert_with(|| value.to_string_lossy().into_owned());
        }
    }
}

/// sruman's binary, found in `PATH`.
pub const SRUMAN: &str = "sruman";

/// `name`'s first executable file in `path`.
pub fn find_in_path(name: &str, path: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    std::env::split_paths(path?)
        .map(|dir| dir.join(name))
        .find(|p| {
            p.metadata()
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        })
}

/// Set `settings`' env on top of the runner's, append the game's arguments,
/// hand the game to sruman for an `slr` profile, and wrap the command in the
/// wrappers, outermost first: the profile's, then the game's.
pub fn apply(settings: &LaunchSettings, spec: &mut CommandSpec) {
    spec.env
        .extend(settings.env.iter().map(|(k, v)| (k.clone(), v.clone())));
    spec.args.extend(settings.args.iter().cloned());
    if let Some(profile) = &settings.sruman {
        through_sruman(spec, profile);
    }

    // `mangohud --dlsym` is one wrapper with its own arguments.
    let mut words = settings
        .wrappers
        .iter()
        .flat_map(|wrapper| wrapper.split_whitespace())
        .map(str::to_owned);
    let Some(outermost) = words.next() else {
        return;
    };
    let mut args: Vec<String> = words.collect();
    args.push(spec.program.to_string_lossy().into_owned());
    args.append(&mut spec.args);
    spec.args = args;
    spec.program = outermost.into();
}

/// `sruman execute -p <profile> --wait <game> -- <args>`: sruman runs the
/// game in the profile's runtime and prefix, with the profile's env and
/// wrappers, and stays in the foreground with the game's exit code. Its
/// working directory is the game executable's.
fn through_sruman(spec: &mut CommandSpec, profile: &str) {
    let mut args = vec![
        "execute".to_owned(),
        "-p".to_owned(),
        profile.to_owned(),
        "--wait".to_owned(),
        spec.program.to_string_lossy().into_owned(),
        "--".to_owned(),
    ];
    args.append(&mut spec.args);
    spec.args = args;
    spec.program = SRUMAN.into();
}

#[cfg(test)]
mod tests;
