use crate::infra::runners::{CommandSpec, LaunchContext, Runner};
use crate::launch::pipeline::{LaunchError, LaunchErrorKind};
use anyhow::anyhow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct WineTkgRunner;

/// Steam integration facts, derived per phase.
struct SteamLayout {
    library_root: PathBuf,
    steam_mode: SteamMode,
    compat_data_path: PathBuf,
    game_prefix: PathBuf,
}

impl SteamLayout {
    /// Derive the layout from `ctx`.
    fn derive(ctx: &LaunchContext) -> Self {
        let library_root = PathBuf::from(&ctx.launcher_config.steam_library_path);
        // A profile's shared prefix, or the game's own compatdata.
        let compat_data_path = ctx.compat_data();
        Self {
            steam_mode: resolve_steam_mode(
                ctx.steam_enabled,
                ctx.settings.as_ref().map(|s| s.steam).unwrap_or_default(),
            ),
            game_prefix: compat_data_path.join("pfx"),
            compat_data_path,
            library_root,
        }
    }
}

#[async_trait::async_trait]
impl Runner for WineTkgRunner {
    fn name(&self) -> &str {
        "Wine-TKG"
    }
    async fn prepare_prefix(&self, ctx: &LaunchContext) -> std::result::Result<(), LaunchError> {
        let SteamLayout {
            compat_data_path,
            game_prefix,
            ..
        } = SteamLayout::derive(ctx);
        // Proton's `pfx.lock` needs existing compatdata.
        for dir in [&game_prefix, &compat_data_path] {
            std::fs::create_dir_all(dir).map_err(|e| {
                LaunchError::new(
                    LaunchErrorKind::Permission,
                    format!("failed creating {}", dir.display()),
                )
                .with_source(anyhow!(e))
            })?;
        }
        tracing::info!("Game prefix: {}", game_prefix.display());

        let (_install_dir, _executable, game_working_dir) = ctx.game_paths()?;
        if let Err(e) = crate::core::utils::ensure_steam_appid(&game_working_dir, ctx.app.app_id) {
            tracing::warn!("could not write steam_appid.txt: {e}");
        }

        Ok(())
    }

    async fn build_env(
        &self,
        ctx: &LaunchContext,
    ) -> std::result::Result<HashMap<String, String>, LaunchError> {
        let mut env = HashMap::new();
        let app_id_str = ctx.app.app_id.to_string();

        let layout = SteamLayout::derive(ctx);
        let SteamLayout {
            ref library_root,
            steam_mode,
            ref compat_data_path,
            game_prefix: ref effective_game_prefix,
            ..
        } = layout;

        env.insert("SteamAppId".to_string(), app_id_str.clone());
        env.insert("SteamGameId".to_string(), app_id_str.clone());
        env.insert("STEAM_COMPAT_APP_ID".to_string(), app_id_str);
        env.insert(
            "WINEPREFIX".to_string(),
            effective_game_prefix.to_string_lossy().to_string(),
        );
        env.insert(
            "STEAM_COMPAT_DATA_PATH".to_string(),
            compat_data_path.to_string_lossy().to_string(),
        );

        // Add user identity context if available
        #[cfg(feature = "authentication")]
        if let Ok(session) = crate::auth::load_session().await {
            if let Some(steam_id) = session.steam_id {
                env.insert("SteamUser".to_string(), steam_id.to_string());
            }
            if let Some(account_name) = session.account_name {
                env.insert("SteamAppUser".to_string(), account_name);
            }
        }

        let glc = ctx
            .user_config
            .as_ref()
            .map(|c| c.graphics_layers.clone())
            .unwrap_or_default();
        // Keep Steam's in-game overlay disabled.
        let no_overlay = true;

        let (_install_dir, _executable, game_working_dir) = ctx.game_paths()?;

        // Resolve proton version for component detection and DLL path building
        let proton = ctx
            .proton_path
            .as_deref()
            .filter(|p| !p.is_empty())
            .unwrap_or("wine");

        let active_runner_path = crate::core::utils::resolve_runner(proton, library_root);
        let _components = crate::core::utils::detect_runner_components(
            &active_runner_path,
            Some(effective_game_prefix),
        );

        // 1. Resolve DX8-11 policy (GraphicsBackendPolicy) - CONSERVATIVE
        let (policy_dxvk, force_builtin, strict_dxvk) = match glc.graphics_backend_policy {
            // Auto is now conservative: it does NOT automatically enable DXVK
            // even if detected on disk. It prefers default Wine behavior.
            crate::core::models::GraphicsBackendPolicy::Auto => (false, false, false),
            crate::core::models::GraphicsBackendPolicy::WineD3D => (false, true, false),
            crate::core::models::GraphicsBackendPolicy::DXVK => (true, false, true),
        };

        // Manual override takes precedence if enabled
        let effective_dxvk = glc.dxvk_enabled || policy_dxvk;

        // If user explicitly selected WineD3D and didn't force DXVK, we use builtins.
        let force_builtin_d3d = force_builtin && !effective_dxvk;

        // 2. Resolve DX12 policy (D3D12ProviderPolicy) - CONSERVATIVE
        let (policy_vkd3dp, policy_vkd3dw) = match glc.d3d12_policy {
            // Auto is now conservative: no forced D3D12 provider unless explicitly requested.
            crate::core::models::D3D12ProviderPolicy::Auto => (false, false),
            crate::core::models::D3D12ProviderPolicy::Vkd3dProton => (true, false),
            crate::core::models::D3D12ProviderPolicy::Vkd3dWine => (false, true),
        };
        // Manual overrides take precedence
        let effective_vkd3d_proton = glc.vkd3d_proton_enabled || policy_vkd3dp;
        let effective_vkd3d = glc.vkd3d_enabled || policy_vkd3dw;

        // NVAPI Support
        let nvapi_enabled_cfg = ctx
            .user_config
            .as_ref()
            .map(|c| c.graphics_layers.nvapi_enabled)
            .unwrap_or(true);
        let nvapi_active = _components.nvapi.is_some() && nvapi_enabled_cfg;
        if nvapi_active {
            tracing::info!("NVAPI component detected and enabled, will be exposed to game");
        } else if _components.nvapi.is_some() {
            tracing::info!("NVAPI component detected but disabled by per-game settings");
        }

        // Only the host-bridge mode leaves Steam's client DLLs at Proton's defaults (so
        // `lsteamclient` bridges to the host client). Standalone wants the
        // native/neutralised set (`steamclient=n`, …) so the game loads none at all.
        let host_bridge = steam_mode == SteamMode::HostBridge;

        let use_symlinks = glc.use_symlinks_in_prefix;
        let mut dll_overrides = crate::core::utils::build_dll_overrides(
            effective_dxvk,
            effective_vkd3d_proton,
            effective_vkd3d,
            no_overlay,
            force_builtin_d3d,
            Some(&game_working_dir),
            strict_dxvk,
            host_bridge,
        );

        // Enhance overrides with resolved DLL providers
        for res in &ctx.dll_resolutions {
            if res.chosen_provider == crate::launch::dll_provider_resolver::DllProvider::GameLocal
                || (res.chosen_provider
                    == crate::launch::dll_provider_resolver::DllProvider::Custom
                    && !use_symlinks)
                || (res.chosen_provider
                    == crate::launch::dll_provider_resolver::DllProvider::Runner
                    && res.name.contains("nvapi"))
            {
                // Ensure native wins for game-local or non-symlinked custom DLLs
                if !dll_overrides.contains(&format!("{}=n", res.name)) {
                    tracing::info!(
                        "Adding native override for resolved DLL: {} (provider: {:?})",
                        res.name,
                        res.chosen_provider
                    );
                    dll_overrides.push_str(&format!(";{}=n", res.name));
                }
            } else if res.chosen_provider
                == crate::launch::dll_provider_resolver::DllProvider::Internal
            {
                tracing::info!(
                    "Resolved DLL {} is handled internally (alias), skipping explicit override",
                    res.name
                );
            }
        }

        // Merge auto-fixup DLL overrides from the per-game registry. Existing
        // entries (policy/provider-derived, i.e. the explicit resolution) WIN: a
        // fixup is only appended for a DLL not already present in the override string.
        if !ctx.game_fixups.dll_overrides.is_empty() {
            let existing_names: std::collections::HashSet<String> = dll_overrides
                .split(';')
                .filter_map(|e| e.split('=').next())
                .map(|s| s.trim().trim_end_matches(".dll").to_lowercase())
                .filter(|s| !s.is_empty())
                .collect();
            for (dll, mode) in &ctx.game_fixups.dll_overrides {
                let key = dll.trim_end_matches(".dll").to_lowercase();
                if existing_names.contains(&key) {
                    tracing::info!(
                        "Skipping fixup DLL override {}={} (explicit override already present)",
                        dll,
                        mode
                    );
                    continue;
                }
                tracing::info!("Applying fixup DLL override: {}={}", dll, mode);
                if !dll_overrides.is_empty() {
                    dll_overrides.push(';');
                }
                dll_overrides.push_str(&format!("{}={}", dll, mode));
            }
        }

        // The profile's overrides are explicit configuration: they replace
        // whatever the policy, providers or fixups chose for the same DLL.
        if let Some(proton) = ctx.settings.as_ref().and_then(|s| s.proton.as_ref())
            && !proton.dll_overrides.is_empty()
        {
            dll_overrides = merge_dll_overrides(&dll_overrides, &proton.dll_overrides);
        }

        tracing::info!("Final WINEDLLOVERRIDES: {}", dll_overrides);
        env.insert("WINEDLLOVERRIDES".to_string(), dll_overrides);

        // Track effective state for diagnostics (HACK: should ideally be done in a separate stage)
        // This is safe because WineTkgRunner is currently the only one implementing this logic.
        // We'll see if we can move it to PipelineContext later.

        // Translate Runner-resolved DLL paths into WINEDLLPATH so Wine can
        // actually find the bundled DLLs (VKD3D-Proton, DXVK, etc.) in the runner.
        // WITHOUT THIS, d3d12=n,b finds whatever is in the prefix's system32 instead.
        // CONSERVATIVE: only include paths for DLLs that are actually requested to be native.
        let mut wine_dll_dirs: Vec<String> = Vec::new();

        for res in &ctx.dll_resolutions {
            if (res.chosen_provider == crate::launch::dll_provider_resolver::DllProvider::Runner
                || res.chosen_provider == crate::launch::dll_provider_resolver::DllProvider::Custom)
                && !use_symlinks
            {
                // Check if this DLL is actually selected for use by the current policy/overrides
                let name = res.name.to_lowercase();
                let is_dxvk_dll = matches!(
                    name.as_str(),
                    "d3d8" | "d3d9" | "d3d10" | "d3d10_1" | "d3d10core" | "d3d11" | "dxgi"
                );
                let is_d3d12_dll = matches!(
                    name.as_str(),
                    "d3d12" | "d3d12core" | "libvkd3d-1" | "libvkd3d-shader-1"
                );

                let is_nvapi_dll = matches!(name.as_str(), "nvapi" | "nvapi64" | "nvofapi64");
                let selected = (is_dxvk_dll && effective_dxvk)
                    || (is_d3d12_dll && (effective_vkd3d_proton || effective_vkd3d))
                    || is_nvapi_dll;

                if !selected {
                    continue;
                }

                if let Some(path) = &res.chosen_path
                    && let Some(parent) = path.parent()
                {
                    push_dll_dir_once(&mut wine_dll_dirs, parent);

                    // For Wine-TKG and similar layouts, we must ensure both 64-bit and 32-bit
                    // architecture folders are in WINEDLLPATH if they exist, so that both
                    // architectures of a game find their respective native DLLs.
                    let folder_name = parent.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    let sibling_arch = match folder_name {
                        "x86_64-windows" => Some("i386-windows"),
                        "i386-windows" => Some("x86_64-windows"),
                        _ => None,
                    };
                    if let Some(arch) = sibling_arch {
                        let sibling = parent.parent().unwrap().join(arch);
                        if sibling.exists() {
                            push_dll_dir_once(&mut wine_dll_dirs, &sibling);
                        }
                    }
                }
            }
        }

        // Also add the runner's main lib/wine directories so Wine can find the
        // .dll.so PE loader stubs it needs to bridge into native DLLs. Composed from
        // the shared unified-layout constants: the `*/wine` roots plus the bare base
        // dirs (`files/lib`, `files/lib64`) where modern WOW64 builds keep their
        // unix-bridge `.so` libs (so ntdll.dll resolves in syswow64), and both the
        // `-windows` (PE) and `-unix` (WOW64 bridge) arch subdirs.
        let active_runner = crate::core::utils::resolve_runner(proton, library_root);
        let runner_root = crate::core::utils::derive_runner_root(&active_runner);
        for lib_sub in crate::compat::proton::UNIFIED_LIB_SUBDIRS {
            let p = runner_root.join(lib_sub);
            if p.exists() {
                push_dll_dir_once(&mut wine_dll_dirs, &p);

                // Ensure architecture-specific subdirectories are also in WINEDLLPATH.
                // This is critical for PE-based runners where Wine expects DLLs in
                // x86_64-windows/i386-windows folders, and for WOW64 builds whose unix
                // bridge libs live under x86_64-unix/i386-unix.
                for arch in crate::compat::proton::ARCH_SUBDIRS {
                    let arch_p = p.join(arch);
                    if arch_p.exists() {
                        push_dll_dir_once(&mut wine_dll_dirs, &arch_p);
                    }
                }
            }
        }

        if !wine_dll_dirs.is_empty() {
            // Preserve any WINEDLLPATH the user may have set in env_variables
            let existing = env.get("WINEDLLPATH").cloned().unwrap_or_default();
            let combined = if existing.is_empty() {
                wine_dll_dirs.join(":")
            } else {
                format!("{}:{}", wine_dll_dirs.join(":"), existing)
            };
            env.insert("WINEDLLPATH".to_string(), combined);
        }

        // Append runner DLL directories to WINEPATH to aid native PE loading
        let mut wine_path = vec!["C:\\Program Files (x86)\\Steam".to_string()];
        wine_path.extend(wine_dll_dirs.iter().cloned());
        env.insert("WINEPATH".to_string(), wine_path.join(";"));

        // Expose the Steam client install path the game's DRM/Steamworks bootstrap
        // reads (`STEAM_COMPAT_CLIENT_INSTALL_PATH`), matching the resolved mode:
        //   - HostBridge → the host Steam install (Proton's lsteamclient bridges).
        //   - Standalone → the fake-Steam trap (no DRM).
        let fake_trap =
            |env: &mut HashMap<String, String>| -> std::result::Result<(), LaunchError> {
                let fake_env = crate::infra::runners::insert_fake_steam_trap(env)?;
                ctx.with_verification(|v| {
                    v.steam_client_install_path_exposed_to_game =
                        Some(fake_env.to_string_lossy().to_string());
                    v.steam_client_install_path_source = Some("fake_trap".to_string());
                });
                Ok(())
            };
        let expose = |env: &mut HashMap<String, String>, path: PathBuf, source: &str| {
            env.insert(
                "STEAM_COMPAT_CLIENT_INSTALL_PATH".to_string(),
                path.to_string_lossy().to_string(),
            );
            ctx.with_verification(|v| {
                v.steam_client_install_path_exposed_to_game =
                    Some(path.to_string_lossy().to_string());
                v.steam_client_install_path_source = Some(source.to_string());
            });
        };

        match steam_mode {
            SteamMode::HostBridge => match crate::core::utils::host_steam_client_path() {
                Some(path) => expose(&mut env, path, "real_host"),
                None if ctx.steam_enabled => {
                    return Err(LaunchError::new(
                        LaunchErrorKind::Environment,
                        "Steam integration was requested (`--steam`, or a Family-Shared game) \
                         but no official Steam client is installed. Install Steam with your \
                         package manager.",
                    ));
                }
                // An opportunistic bridge whose Steam install vanished mid-launch.
                None => fake_trap(&mut env)?,
            },
            SteamMode::Standalone => fake_trap(&mut env)?,
        }

        if let Ok(display) = std::env::var("DISPLAY") {
            env.insert("DISPLAY".to_string(), display);
        }
        if let Ok(wayland) = std::env::var("WAYLAND_DISPLAY") {
            env.insert("WAYLAND_DISPLAY".to_string(), wayland);
        }
        if let Ok(xdg_runtime) = std::env::var("XDG_RUNTIME_DIR") {
            env.insert("XDG_RUNTIME_DIR".to_string(), xdg_runtime);
        }
        // X11 servers that use cookie authentication reject Wine's winex11 driver
        // unless XAUTHORITY is forwarded. Modern desktops (GDM/systemd/Xwayland)
        // place the cookie under $XDG_RUNTIME_DIR rather than ~/.Xauthority, so
        // Wine can't find it implicitly — without this the game fails to create a
        // window ("nodrv_CreateWindow: no driver could be loaded") and runs
        // invisibly even though DISPLAY is set.
        if let Ok(xauthority) = std::env::var("XAUTHORITY") {
            env.insert("XAUTHORITY".to_string(), xauthority);
        }

        // Apply GPU preference if specified. CONSERVATIVE: No forced offload if unset.
        if let Some(gpu_pref) = ctx
            .user_config
            .as_ref()
            .and_then(|c| c.gpu_preference.as_ref())
        {
            let available_gpus = crate::core::utils::list_available_gpus();
            if let Some(gpu) = available_gpus.iter().find(|g| &g.name == gpu_pref) {
                if gpu.name.contains("NVIDIA") {
                    env.insert("__NV_PRIME_RENDER_OFFLOAD".to_string(), "1".to_string());
                    env.insert(
                        "__NV_PRIME_RENDER_OFFLOAD_PROVIDER".to_string(),
                        "NVIDIA-G0".to_string(),
                    );
                    env.insert(
                        "__VK_LAYER_NV_optimus".to_string(),
                        "NVIDIA_only".to_string(),
                    );
                    env.insert(
                        "__GLX_VENDOR_LIBRARY_NAME".to_string(),
                        "nvidia".to_string(),
                    );
                } else if gpu.name.contains("AMD")
                    || gpu.name.contains("Intel")
                    || gpu.name.contains("Unknown")
                {
                    // Standard DRI_PRIME for non-NVIDIA discrete/specific GPUs
                    // Try to find "cardN" and extract N
                    static CARD_RE: std::sync::LazyLock<regex::Regex> =
                        std::sync::LazyLock::new(|| regex::Regex::new(r"card(\d+)").unwrap());
                    if let Some(card_idx) = CARD_RE
                        .captures(&gpu.name)
                        .and_then(|caps| caps.get(1))
                        .and_then(|m| m.as_str().parse::<u32>().ok())
                    {
                        // DRI_PRIME=1 is the most common way to select the second GPU
                        // For now we use the standard PRIME offload if it's not card0.
                        let dri_prime = if card_idx > 0 { "1" } else { "0" };
                        env.insert("DRI_PRIME".to_string(), dri_prime.to_string());
                    }
                }
            }
        }

        // Merge auto-fixup env vars from the per-game registry. Applied BEFORE the
        // user's explicit `env_variables` below so that on conflict the user's value
        // wins (an explicit per-game env var overwrites the auto-fixup).
        if !ctx.game_fixups.env.is_empty() {
            let user_has = |key: &str| {
                ctx.user_config
                    .as_ref()
                    .is_some_and(|c| c.env_variables.contains_key(key))
            };
            for (key, val) in &ctx.game_fixups.env {
                if user_has(key) {
                    tracing::info!(
                        "Skipping fixup env {} (explicit per-game value present)",
                        key
                    );
                    continue;
                }
                tracing::info!("Applying fixup env: {}={}", key, val);
                env.insert(key.clone(), val.clone());
            }
        }

        if let Some(config) = &ctx.user_config {
            for (key, val) in &config.env_variables {
                env.insert(key.clone(), val.clone());
            }

            // Add debug toggles
            if effective_dxvk && !env.contains_key("DXVK_HUD") {
                env.insert("DXVK_HUD".to_string(), "compiler".to_string());
            }
            if (effective_vkd3d_proton || effective_vkd3d) && !env.contains_key("VKD3D_DEBUG") {
                env.insert("VKD3D_DEBUG".to_string(), "warn".to_string());
            }
        }

        let wants_mangohud = ctx
            .user_config
            .as_ref()
            .map(|c| {
                c.env_variables.contains_key("MANGOHUD")
                    || c.launch_options
                        .split_whitespace()
                        .any(|a| a == "-mangohud" || a == "--mangohud")
            })
            .unwrap_or(false);

        if wants_mangohud {
            match crate::core::utils::find_mangohud_lib() {
                Some(lib) => {
                    let existing = std::env::var("LD_PRELOAD").unwrap_or_default();
                    let new_preload = if existing.is_empty() {
                        lib.to_string_lossy().to_string()
                    } else {
                        format!("{}:{}", lib.to_string_lossy(), existing)
                    };
                    env.insert("LD_PRELOAD".to_string(), new_preload);
                    env.insert("MANGOHUD".to_string(), "1".to_string());
                    env.insert("MANGOHUD_DLSYM".to_string(), "1".to_string());
                }
                None => {
                    tracing::warn!("MangoHud requested but libMangoHud.so not found; skipping");
                }
            }
        }

        env.insert(
            "WINEDEBUG".to_string(),
            "err+all,warn+module,warn+loaddll".to_string(),
        );

        if let Some(settings) = &ctx.settings {
            if settings.debug {
                env.insert("PROTON_LOG".to_string(), "1".to_string());
            }
            if let Some(proton) = &settings.proton {
                anticheat_env(&mut env, proton.anticheat, library_root);
            }
        }

        let log_dir = crate::core::config::config_dir()
            .unwrap_or_else(|_| PathBuf::from("/tmp"))
            .join("logs");
        let log_path = log_dir.join(format!("wine_{}.log", ctx.app.app_id));
        env.insert(
            "WINE_LOG_OUTPUT".to_string(),
            log_path.to_string_lossy().to_string(),
        );

        Ok(env)
    }

    async fn build_command(
        &self,
        ctx: &LaunchContext,
    ) -> std::result::Result<CommandSpec, LaunchError> {
        let library_root = PathBuf::from(&ctx.launcher_config.steam_library_path);

        let proton = resolve_proton_required(ctx)?;
        let active_runner = crate::core::utils::resolve_runner(proton, &library_root);

        // Classify the GAME runner to record the launch route. A real Proton tree is
        // launched via its `proton run` script (protonfixes apply naturally); plain
        // Wine / wine-tkg launch via bare wine and rely on the data-driven fixup layer
        // (merged in build_env). In every case the game binary is a DIRECT spawn arg
        // to the runner — never a `steam://run/<appid>` or `-applaunch` handoff.
        match crate::core::utils::classify_runner(&active_runner) {
            crate::core::utils::RunnerKind::Proton => {
                tracing::info!(
                    "Game runner classified as Proton: launching via `proton run` (protonfixes active)"
                );
            }
            crate::core::utils::RunnerKind::WineTkg | crate::core::utils::RunnerKind::PlainWine => {
                tracing::info!(
                    fixup_env = ctx.game_fixups.env.len(),
                    fixup_dll = ctx.game_fixups.dll_overrides.len(),
                    "Game runner classified as bare Wine: launching via wine + fixup layer"
                );
            }
            crate::core::utils::RunnerKind::Unknown => {
                tracing::warn!(
                    "Game runner '{}' did not classify as Proton or Wine; relying on build_runner_command resolution",
                    active_runner.display()
                );
            }
        }

        let mut spec = CommandSpec::default();

        // Build the base command (handles 'proton run' wrapper and directory resolution)
        let base_cmd = crate::core::utils::build_runner_command(&active_runner).map_err(|e| {
            LaunchError::new(
                LaunchErrorKind::Runner,
                format!(
                    "Invalid Compatibility Layer path: {}",
                    active_runner.display()
                ),
            )
            .with_source(e)
        })?;
        spec.program = base_cmd.get_program().into();
        spec.args = base_cmd
            .get_args()
            .map(|s| s.to_string_lossy().to_string())
            .collect();

        let (_install_dir, executable, game_working_dir) = ctx.game_paths()?;

        spec.cwd = Some(game_working_dir);
        // The profile's `runner_args` go to the runner, ahead of the game.
        if let Some(settings) = &ctx.settings {
            spec.args.extend(settings.runner_args.iter().cloned());
        }
        spec.args.push(executable.to_string_lossy().to_string());

        // Split args from launch_info
        let args = ctx
            .launch_info
            .arguments
            .split_whitespace()
            .map(ToString::to_string);
        spec.args.extend(args);

        // Split user launch args
        let user_launch_args = ctx
            .user_config
            .as_ref()
            .map(|c| {
                c.launch_options
                    .split_whitespace()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
            .into_iter()
            .filter(|a| a != "-mangohud" && a != "--mangohud");
        spec.args.extend(user_launch_args);

        spec.env = self.build_env(ctx).await?;

        Ok(spec)
    }

    fn launch(&self, spec: &CommandSpec) -> std::result::Result<std::process::Child, LaunchError> {
        let mut cmd = Command::new(&spec.program);
        cmd.args(&spec.args);
        if let Some(cwd) = &spec.cwd {
            cmd.current_dir(cwd);
        }
        for (key, val) in &spec.env {
            cmd.env(key, val);
        }

        let log_path = spec.env.get("WINE_LOG_OUTPUT").map(PathBuf::from);
        if let Some(path) = log_path {
            std::fs::create_dir_all(path.parent().unwrap()).ok();
            if let Ok(log_file) = std::fs::File::create(&path) {
                cmd.stderr(log_file);
            } else {
                cmd.stderr(crate::infra::logging::relay::child_stdio());
            }
        } else {
            cmd.stderr(crate::infra::logging::relay::child_stdio());
        }

        cmd.stdout(crate::infra::logging::relay::child_stdio());

        tracing::debug!(
            program = ?cmd.get_program(),
            args = ?cmd.get_args().collect::<Vec<_>>(),
            working_dir = ?cmd.get_current_dir(),
            "runner launch",
        );

        cmd.spawn()
            .map(crate::infra::logging::relay::attach)
            .map_err(|e| {
                LaunchError::new(LaunchErrorKind::Process, "failed to spawn runner process")
                    .with_source(anyhow!(e))
            })
    }
}

/// `existing` (a `WINEDLLOVERRIDES` value, `a,b=n;c=b`) with every DLL in
/// `overrides` set to its mode there instead.
pub fn merge_dll_overrides(
    existing: &str,
    overrides: &std::collections::BTreeMap<String, String>,
) -> String {
    let normalize = |name: &str| name.trim().trim_end_matches(".dll").to_lowercase();
    let replaced: std::collections::HashSet<String> =
        overrides.keys().map(|name| normalize(name)).collect();
    let mut entries: Vec<String> = Vec::new();
    for entry in existing.split(';').filter(|e| !e.trim().is_empty()) {
        let (names, mode) = entry.split_once('=').unwrap_or((entry, ""));
        let kept: Vec<&str> = names
            .split(',')
            .filter(|name| !replaced.contains(&normalize(name)))
            .collect();
        if !kept.is_empty() {
            entries.push(format!("{}={mode}", kept.join(",")));
        }
    }
    entries.extend(
        overrides
            .iter()
            .map(|(name, mode)| format!("{name}={mode}")),
    );
    entries.join(";")
}

/// The Steam runtimes Proton loads for EAC and BattlEye games.
const EAC_RUNTIME: (&str, &str, u32) = (
    "PROTON_EAC_RUNTIME",
    "Proton EasyAntiCheat Runtime",
    1826330,
);
const BATTLEYE_RUNTIME: (&str, &str, u32) = (
    "PROTON_BATTLEYE_RUNTIME",
    "Proton BattlEye Runtime",
    1161040,
);

/// Point Proton at the anti-cheat runtimes the profile asks for, when they're
/// installed in the library.
fn anticheat_env(
    env: &mut HashMap<String, String>,
    anticheat: crate::config::Anticheat,
    library_root: &Path,
) {
    for (wanted, (var, dir, app_id)) in [
        (anticheat.eac, EAC_RUNTIME),
        (anticheat.battleye, BATTLEYE_RUNTIME),
    ] {
        if !wanted {
            continue;
        }
        let path = library_root.join("steamapps").join("common").join(dir);
        if path.is_dir() {
            env.insert(var.to_string(), path.to_string_lossy().into_owned());
        } else {
            tracing::warn!(
                "the profile enables {dir}, but it isn't installed; install app {app_id} \
                 (`gaki-cli install {app_id}`)"
            );
        }
    }
}

/// Push dir if not already present.
fn push_dll_dir_once(dirs: &mut Vec<String>, path: &Path) {
    let s = path.to_string_lossy().to_string();
    if !dirs.contains(&s) {
        dirs.push(s);
    }
}

/// Which Steam-integration mode a launch runs in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SteamMode {
    /// Bridge to the **host** Steam client via Proton's `lsteamclient` (full online
    /// Steamworks, Family-Shared licences). DLL overrides stay at Proton's defaults so
    /// `lsteamclient` loads builtin and bridges to the running client.
    HostBridge,
    /// No Steam client — the standalone fake-Steam trap. DRM-protected games can't run.
    Standalone,
}

/// Resolve the Steam-integration mode. `steam_enabled` (`--steam`, `steam =
/// "host"`, or a Family-Shared game) always bridges to the host client;
/// `steam = "none"` never does; `auto` bridges to a host Steam that is already
/// running, and the launch is standalone when none is.
pub(super) fn resolve_steam_mode(
    steam_enabled: bool,
    integration: crate::config::SteamIntegration,
) -> SteamMode {
    if steam_enabled {
        return SteamMode::HostBridge;
    }
    if integration == crate::config::SteamIntegration::None {
        return SteamMode::Standalone;
    }
    if crate::core::utils::host_steam_client_path().is_some()
        && crate::core::utils::is_steam_running()
    {
        tracing::info!("host Steam detected; bridging Steamworks");
        return SteamMode::HostBridge;
    }
    SteamMode::Standalone
}

/// Resolve the Proton/runner identifier, erroring if none is available.
fn resolve_proton_required(ctx: &LaunchContext) -> std::result::Result<&str, LaunchError> {
    ctx.proton_path
        .as_deref()
        .filter(|p| !p.is_empty())
        .ok_or_else(|| {
            LaunchError::new(
                LaunchErrorKind::Environment,
                "proton path is required for Windows launch",
            )
        })
}
