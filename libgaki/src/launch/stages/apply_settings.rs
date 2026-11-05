//! Applies the runner-independent parts of the resolved settings to the
//! command: the profile's and game's environment, the game's arguments, and
//! the profile's wrappers.
//!
//! Runs after `PreflightStage`, which checks the runner binary itself, and
//! before `ApplyLaunchScriptStage`, so a launch script receives the wrapped
//! command as `"$@"`.

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
        let (Some(settings), Some(spec)) = (&ctx.settings, ctx.command_spec.as_mut()) else {
            return Ok(());
        };
        apply(settings, spec);
        Ok(())
    }
}

/// Set `settings`' env on top of the runner's, append the game's arguments,
/// and wrap the command in the wrappers, outermost first.
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
