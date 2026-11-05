//! Applies the runner-independent parts of the resolved settings to the
//! command: the `GAKI_*` variables, the profile's and game's environment, the
//! game's arguments, and the wrappers.
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
        crate::launch::game_env::export(spec, app_id, &name, game_dir.as_deref());
        if let Some(settings) = &ctx.settings {
            apply(settings, spec);
        }
        Ok(())
    }
}

/// Set `settings`' env on top of the runner's, append the game's arguments,
/// and wrap the command in the wrappers, outermost first: the profile's, then
/// the game's.
pub fn apply(settings: &LaunchSettings, spec: &mut CommandSpec) {
    spec.env
        .extend(settings.env.iter().map(|(k, v)| (k.clone(), v.clone())));
    spec.args.extend(settings.args.iter().cloned());

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

#[cfg(test)]
#[path = "apply_settings_tests.rs"]
mod tests;
