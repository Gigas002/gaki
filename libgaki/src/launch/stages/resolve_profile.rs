use crate::launch::pipeline::{LaunchError, LaunchErrorKind, PipelineContext, PipelineStage};
use async_trait::async_trait;

/// Makes sure the launch has its resolved settings: the ones the caller
/// passed in, or else the game's configured settings with no overrides.
pub struct ResolveProfileStage;

#[async_trait]
impl PipelineStage for ResolveProfileStage {
    fn name(&self) -> &str {
        "ResolveProfile"
    }
    async fn execute(&self, ctx: &mut PipelineContext) -> std::result::Result<(), LaunchError> {
        if ctx.launch_info.is_none() {
            return Err(LaunchError::new(
                LaunchErrorKind::Validation,
                "LaunchInfo missing in ResolveProfileStage",
            )
            .with_context("app_id", ctx.app_id.to_string()));
        }
        if ctx.settings.is_none() {
            let settings =
                crate::settings::resolve_user(ctx.app_id, &crate::settings::Overrides::default())
                    .map_err(|e| {
                    LaunchError::new(LaunchErrorKind::Validation, format!("{e:#}"))
                        .with_context("app_id", ctx.app_id.to_string())
                })?;
            ctx.settings = Some(settings);
        }
        if let Some(profile) = ctx.settings.as_ref().and_then(|s| s.profile.as_deref()) {
            tracing::info!(appid = ctx.app_id, profile, "launch profile");
        }
        Ok(())
    }
}
