use crate::launch::pipeline::{LaunchError, LaunchErrorKind, PipelineContext, PipelineStage};
use async_trait::async_trait;

use crate::infra::runners::{CommandSpec, LaunchContext, Runner};
use std::collections::HashMap;
use std::path::PathBuf;

pub struct ResolveComponentsStage;

pub struct NativeRunner;

#[async_trait::async_trait]
impl Runner for NativeRunner {
    fn name(&self) -> &str {
        "Native"
    }
    async fn prepare_prefix(&self, _ctx: &LaunchContext) -> std::result::Result<(), LaunchError> {
        Ok(())
    }
    async fn build_env(
        &self,
        ctx: &LaunchContext,
    ) -> std::result::Result<HashMap<String, String>, LaunchError> {
        let mut env = HashMap::new();
        env.insert("SteamAppId".to_string(), ctx.app.app_id.to_string());
        if let Some(config) = &ctx.user_config {
            env.extend(
                config
                    .env_variables
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone())),
            );
        }
        Ok(env)
    }
    async fn build_command(
        &self,
        ctx: &LaunchContext,
    ) -> std::result::Result<CommandSpec, LaunchError> {
        let install_path =
            ctx.app.install_path.as_ref().ok_or_else(|| {
                LaunchError::new(LaunchErrorKind::GameData, "Install path missing")
            })?;

        let exe_rel = ctx.launch_info.executable.replace('\\', "/");
        let mut args: Vec<String> = ctx
            .launch_info
            .arguments
            .split_whitespace()
            .map(str::to_string)
            .collect();
        if let Some(config) = &ctx.user_config {
            args.extend(config.launch_options.split_whitespace().map(str::to_string));
        }
        let program = PathBuf::from(install_path).join(&exe_rel);
        let mut env = self.build_env(ctx).await?;

        // 32-bit natives need scout runtime libraries.
        if needs_scout_libs(&program, std::path::Path::new(install_path)) {
            if let Some(libs) = scout_library_path() {
                let existing = env
                    .get("LD_LIBRARY_PATH")
                    .cloned()
                    .or_else(|| std::env::var("LD_LIBRARY_PATH").ok())
                    .filter(|v| !v.is_empty());
                let value = match existing {
                    Some(rest) => format!("{libs}:{rest}"),
                    None => libs,
                };
                env.insert("LD_LIBRARY_PATH".to_string(), value);
                tracing::info!("32-bit native game: using Steam scout runtime libraries");
            } else {
                tracing::warn!(
                    "32-bit native game but no Steam scout runtime found; \
                     libraries like libopenal.so.1 may be missing"
                );
            }
        }

        Ok(CommandSpec {
            program,
            args,
            cwd: Some(PathBuf::from(install_path)),
            env,
        })
    }
    fn launch(&self, spec: &CommandSpec) -> std::result::Result<std::process::Child, LaunchError> {
        let spawn = |program: &std::path::Path, script: Option<&std::path::Path>| {
            let mut cmd = std::process::Command::new(program);
            if let Some(script) = script {
                cmd.arg(script);
            }
            cmd.args(&spec.args);
            if let Some(cwd) = &spec.cwd {
                cmd.current_dir(cwd);
            }
            cmd.envs(&spec.env);
            match session_log_file(spec, crate::launch::game_env::ENV_STDOUT_LOG) {
                Some(file) => {
                    cmd.stdout(file);
                }
                None => {
                    cmd.stdout(crate::infra::logging::relay::child_stdio());
                }
            }
            match session_log_file(spec, crate::launch::game_env::ENV_STDERR_LOG) {
                Some(file) => {
                    cmd.stderr(file);
                }
                None => {
                    cmd.stderr(crate::infra::logging::relay::child_stdio());
                }
            }
            cmd.spawn().map(crate::infra::logging::relay::attach)
        };
        let mut result = spawn(&spec.program, None);
        // Shebang-less script: retry via sh.
        if result.as_ref().err().and_then(|e| e.raw_os_error()) == Some(libc::ENOEXEC) {
            tracing::info!(
                "{} is not executable directly (ENOEXEC); retrying via /bin/sh",
                spec.program.display()
            );
            result = spawn(std::path::Path::new("/bin/sh"), Some(&spec.program));
        }
        result.map_err(|e| {
            LaunchError::new(LaunchErrorKind::Process, "Native launch failed")
                .with_source(anyhow::anyhow!(e))
        })
    }
}

/// First `n` bytes of the file.
fn file_head(path: &std::path::Path, n: usize) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut buf = vec![0u8; n];
    std::fs::File::open(path).ok()?.read_exact(&mut buf).ok()?;
    Some(buf)
}

/// Is the file a 32-bit ELF?
fn is_elf32(path: &std::path::Path) -> bool {
    file_head(path, 5).is_some_and(|h| h[..4] == *b"\x7fELF" && h[4] == 1)
}

/// Does this launch need scout runtime libraries?
///
/// True for a 32-bit ELF program, or a shell-script program (like GoldSrc's
/// `hl.sh`) whose install directory carries a 32-bit ELF at the top level.
fn needs_scout_libs(program: &std::path::Path, install_dir: &std::path::Path) -> bool {
    if is_elf32(program) {
        return true;
    }
    if !file_head(program, 2).is_some_and(|h| h == *b"#!") {
        return false;
    }
    std::fs::read_dir(install_dir)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .take(200)
        .any(|e| is_elf32(&e.path()))
}

/// The scout runtime's library search path, when installed.
fn scout_library_path() -> Option<String> {
    let runtime = crate::library::steam_root_candidates()
        .into_iter()
        .map(|root| root.join("ubuntu12_32/steam-runtime"))
        .find(|p| p.is_dir())?;
    let joined = [
        "lib/i386-linux-gnu",
        "usr/lib/i386-linux-gnu",
        "lib/x86_64-linux-gnu",
        "usr/lib/x86_64-linux-gnu",
    ]
    .iter()
    .map(|d| runtime.join(d))
    .filter(|p| p.is_dir())
    .map(|p| p.to_string_lossy().into_owned())
    .collect::<Vec<_>>()
    .join(":");
    (!joined.is_empty()).then_some(joined)
}

/// Open the session log named by `key`.
fn session_log_file(spec: &CommandSpec, key: &str) -> Option<std::fs::File> {
    let path = std::path::PathBuf::from(spec.env.get(key)?);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::File::create(&path).ok()
}

#[async_trait]
impl PipelineStage for ResolveComponentsStage {
    fn name(&self) -> &str {
        "ResolveComponents"
    }
    async fn execute(&self, ctx: &mut PipelineContext) -> std::result::Result<(), LaunchError> {
        use crate::core::models::LaunchTarget;
        use crate::infra::runners::WineTkgRunner;

        if ctx.runner.is_none() {
            let Some(info) = &ctx.launch_info else {
                return Err(LaunchError::new(
                    LaunchErrorKind::Validation,
                    "LaunchInfo missing in ResolveComponentsStage",
                ));
            };
            ctx.runner = Some(match info.target {
                LaunchTarget::NativeLinux => Box::new(NativeRunner) as Box<dyn Runner>,
                LaunchTarget::WindowsProton => Box::new(WineTkgRunner),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
