//! `SteamClient` methods: play/launch, update/verify, download driver.
//!
//! Split out of `steam_client.rs` for readability; the struct, shared imports
//! and free helpers live in the parent module (in scope via `use super::*`).
use super::*;

/// Pick the launch entry to run.
///
/// Only one platform's depot is installed, so a launch entry is only usable if its
/// executable is actually on disk. A game commonly advertises Windows, macOS and
/// Linux entries; picking one by order alone can select a build that was never
/// installed. After a platform switch, files from the previous platform may also
/// still be on disk, so "executable exists" alone is not enough either — prefer
/// the entry matching the *installed* platform, then any entry whose executable
/// exists, then the declared order.
///
/// With `prefer_windows_target` (a native Windows launch or an explicit/configured
/// Proton runner), the Windows entry wins whenever its executable is installed —
/// but a Proton request can't override which depot is actually on disk, so a
/// native-Linux-only install still runs its native build.
pub fn select_launch_entry(
    launch_options: &[LaunchInfo],
    app: &LibraryGame,
    prefer_windows_target: bool,
    prefer_native_target: bool,
) -> Option<LaunchInfo> {
    let on_disk = |rel: &str| on_disk_path(app, rel);
    pick_launch_entry(
        launch_options,
        app,
        prefer_windows_target,
        prefer_native_target,
        &on_disk,
    )
    .map(|mut entry| {
        if let Some(exe) = on_disk(&entry.executable) {
            entry.executable = exe;
        }
        if let Some(wd) = entry
            .workingdir
            .as_deref()
            .filter(|wd| !wd.is_empty())
            .and_then(on_disk)
        {
            entry.workingdir = Some(wd);
        }
        entry
    })
}

/// On-disk spelling of `rel`, ignoring case.
///
/// Windows manifests ignore path case.
fn on_disk_path(app: &LibraryGame, rel: &str) -> Option<String> {
    let dir = std::path::Path::new(app.install_path.as_deref()?);
    let found = crate::core::utils::resolve_path_ignore_case(dir, rel)?;
    let found = found
        .strip_prefix(dir)
        .map(|p| p.to_path_buf())
        .unwrap_or(found);
    Some(found.to_string_lossy().into_owned())
}

/// Is `target`'s executable installed?
fn target_installed(
    launch_options: &[LaunchInfo],
    app: &LibraryGame,
    target: LaunchTarget,
) -> bool {
    launch_options.iter().any(|o| {
        o.target == target && !o.executable.is_empty() && on_disk_path(app, &o.executable).is_some()
    })
}

/// One build plus Steam mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchTargetChoice {
    /// Stable id, e.g. `windows-host`.
    pub id: &'static str,
    /// `linux` or `windows`.
    pub platform: &'static str,
    /// `host` (bridge to host Steam) or `auto` (bridge a running host Steam, else
    /// standalone).
    pub steam_mode: Option<&'static str>,
    /// `gaki-cli play` flags selecting it.
    pub flags: Vec<String>,
    /// Launchable right now.
    pub available: bool,
    /// `build` or `host-steam`.
    pub missing: Option<&'static str>,
    /// What plain `play` picks.
    pub default: bool,
}

/// List every build/Steam combination the game offers.
///
/// `default_entry` and `steam_flag` mirror `play`.
pub fn launch_targets(
    launch_options: &[LaunchInfo],
    app: &LibraryGame,
    host_installed: bool,
    steam_flag: bool,
    default_entry: Option<LaunchTarget>,
) -> Vec<LaunchTargetChoice> {
    let flags = |extra: &[&str]| extra.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let offers = |target: LaunchTarget| launch_options.iter().any(|o| o.target == target);
    let mut targets = Vec::new();

    if offers(LaunchTarget::NativeLinux) {
        let installed = target_installed(launch_options, app, LaunchTarget::NativeLinux);
        let mut f = flags(&["--platform", "linux"]);
        if steam_flag {
            f.push("--steam".to_string());
        }
        targets.push(LaunchTargetChoice {
            id: "linux",
            platform: "linux",
            steam_mode: None,
            flags: f,
            available: installed,
            missing: (!installed).then_some("build"),
            default: installed && default_entry == Some(LaunchTarget::NativeLinux),
        });
    }

    if offers(LaunchTarget::WindowsProton) {
        let installed = target_installed(launch_options, app, LaunchTarget::WindowsProton);
        let default_windows = installed && default_entry == Some(LaunchTarget::WindowsProton);
        let modes = [
            (
                "windows-host",
                "host",
                &["--platform", "windows", "--steam"][..],
                (!host_installed).then_some("host-steam"),
                steam_flag,
            ),
            (
                "windows",
                "auto",
                &["--platform", "windows"][..],
                None,
                !steam_flag,
            ),
        ];
        for (id, mode, mode_flags, steam_missing, is_default_mode) in modes {
            // Missing build outranks missing Steam.
            let missing = if installed {
                steam_missing
            } else {
                Some("build")
            };
            targets.push(LaunchTargetChoice {
                id,
                platform: "windows",
                steam_mode: Some(mode),
                flags: flags(mode_flags),
                available: missing.is_none(),
                missing,
                default: default_windows && missing.is_none() && is_default_mode,
            });
        }
    }

    // Fallback default: first available.
    if !targets.iter().any(|t| t.default)
        && let Some(first) = targets.iter_mut().find(|t| t.available)
    {
        first.default = true;
    }
    targets
}

fn pick_launch_entry(
    launch_options: &[LaunchInfo],
    app: &LibraryGame,
    prefer_windows_target: bool,
    prefer_native_target: bool,
    on_disk: &dyn Fn(&str) -> Option<String>,
) -> Option<LaunchInfo> {
    let exe_exists =
        |o: &LaunchInfo| -> bool { !o.executable.is_empty() && on_disk(&o.executable).is_some() };
    let installed_target = app.platform.as_deref().and_then(|p| match p {
        "linux" => Some(LaunchTarget::NativeLinux),
        "windows" => Some(LaunchTarget::WindowsProton),
        _ => None,
    });

    // Explicit per-game preference beats the manifest.
    if prefer_native_target
        && let Some(entry) = launch_options
            .iter()
            .find(|o| o.target == LaunchTarget::NativeLinux && exe_exists(o))
    {
        return Some(entry.clone());
    }

    if prefer_windows_target {
        launch_options
            .iter()
            .find(|o| o.target == LaunchTarget::WindowsProton && exe_exists(o))
            .or_else(|| {
                launch_options
                    .iter()
                    .find(|o| installed_target.is_some_and(|t| o.target == t) && exe_exists(o))
            })
            .or_else(|| launch_options.iter().find(|o| exe_exists(o)))
            .or_else(|| {
                launch_options
                    .iter()
                    .find(|o| o.target == LaunchTarget::WindowsProton)
            })
            .or_else(|| launch_options.first())
            .cloned()
    } else {
        launch_options
            .iter()
            .find(|o| installed_target.is_some_and(|t| o.target == t) && exe_exists(o))
            .or_else(|| launch_options.iter().find(|o| exe_exists(o)))
            .or_else(|| launch_options.first())
            .cloned()
    }
}

/// Called once the launch target is known, with its id (`None` for native).
pub type TargetResolvedHook<'a> = dyn Fn(Option<&str>) + Send + Sync + 'a;

impl SteamClient {
    pub async fn play_game(
        &mut self,
        app: &LibraryGame,
        proton_path: Option<&str>,
        settings: &crate::settings::LaunchSettings,
        steam_enabled: bool,
        prefer_native_target: bool,
        on_target_resolved: Option<&TargetResolvedHook<'_>>,
    ) -> Result<LaunchInfo> {
        // A Family-Shared game (licensed to another account) can only be authorised
        // by a running Steam client, so it always needs Steam integration regardless
        // of the user's preference.
        if !app.is_owned && settings.steam == crate::config::SteamIntegration::None {
            tracing::warn!(
                appid = app.app_id,
                "`steam = \"none\"` is ignored: a Family-Shared game needs the host Steam client"
            );
        }
        let steam_enabled = steam_enabled || !app.is_owned;

        // With Steam integration the game talks to the host Steam client; make sure
        // one is running (start it silently if not) so Steamworks/Family-Sharing can
        // initialise. Best-effort.
        if steam_enabled {
            crate::core::utils::ensure_steam_running();
            // Avoid racing a cold client.
            let timeout = std::time::Duration::from_secs(30);
            if crate::core::utils::wait_for_steam_logged_on(timeout).await {
                tracing::info!("host Steam logged on; proceeding with launch");
            } else {
                tracing::warn!(
                    "host Steam not logged on within {}s; launching anyway (Steamworks auth may fail)",
                    timeout.as_secs()
                );
            }
        }

        let launch_options = self.get_product_info(app.app_id).await?;

        let prefer_windows_target = proton_path.is_some();
        let launch_info = select_launch_entry(
            &launch_options,
            app,
            prefer_windows_target,
            prefer_native_target,
        )
        .ok_or_else(|| anyhow!("no launch options"))?;

        let launcher_config = load_launcher_config().await?;
        let library_root = PathBuf::from(&launcher_config.steam_library_path);
        // An explicit Proton, then the profile's build, then the global default.
        let windows_proton = proton_path
            .map(str::to_owned)
            .or_else(|| settings.proton.as_ref().and_then(|p| p.build.as_runner()))
            .unwrap_or_else(|| launcher_config.proton_version.clone());

        match launch_info.target {
            LaunchTarget::NativeLinux => {
                tracing::info!(
                    app_id = app.app_id,
                    "resolved launch platform: native Linux"
                );
            }
            LaunchTarget::WindowsProton => {
                tracing::info!(
                    app_id = app.app_id,
                    proton = windows_proton.as_str(),
                    "resolved launch platform: Windows via Proton"
                );
            }
        }

        // None = native Linux; Some = the Proton runner.
        if let Some(cb) = on_target_resolved {
            cb(match launch_info.target {
                LaunchTarget::WindowsProton => Some(windows_proton.as_str()),
                LaunchTarget::NativeLinux => None,
            });
        }

        let chosen_proton_path = match launch_info.target {
            LaunchTarget::NativeLinux => None,
            LaunchTarget::WindowsProton => Some(windows_proton.as_str()),
        };
        let wine_prefix = settings.wine_prefix(&library_root);

        #[cfg(feature = "cloud")]
        let cloud =
            if settings.cloud_sync && launcher_config.enable_cloud_sync && !self.is_offline() {
                Some(self.launch_cloud_sync_down(app, &wine_prefix).await?)
            } else {
                None
            };

        let mut child = self
            .spawn_game_process(
                app,
                &launch_info,
                chosen_proton_path,
                &launcher_config,
                settings,
                steam_enabled,
            )
            .await?;

        // Record the launch so a separate `gaki-cli stop <app_id>` invocation can
        // find and terminate the process while we block on `wait()` below.
        let wineprefix = Some(wine_prefix);
        let record = crate::compat::running::RunningGame::new(
            app.app_id,
            app.name.clone(),
            child.id(),
            wineprefix,
        );
        if let Err(e) = crate::compat::running::record_launch(&record) {
            tracing::warn!(appid = app.app_id, "could not record running game: {e:#}");
        }

        // On the blocking pool: `Child::wait` blocks its thread for the game's whole
        // run, and on a runtime worker that starves the tasks tied to it. In the
        // daemon that froze the shared Steam connection and every other forwarded
        // command (`running`, `stop`) until the game exited.
        let wait_result = tokio::task::spawn_blocking(move || child.wait())
            .await
            .context("game wait task failed")?
            .context("failed waiting for game process exit");
        // Wrappers exit while the game lives on; wait for the app's processes.
        loop {
            let survivors = crate::compat::running::processes_for_app(app.app_id);
            let Some(&pid) = survivors.first() else { break };
            let mut record = record.clone();
            record.set_pid(pid);
            let _ = crate::compat::running::record_launch(&record);
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
        crate::compat::running::clear(app.app_id);
        wait_result?;

        #[cfg(feature = "cloud")]
        if let Some(cloud) = &cloud {
            cloud.sync_up(app.app_id).await;
        }

        Ok(launch_info)
    }

    #[cfg(feature = "install")]
    pub async fn update_game(
        &self,
        appid: u32,
        shared_state: Arc<std::sync::RwLock<crate::core::models::DownloadState>>,
    ) -> Result<Receiver<DownloadProgress>> {
        self.start_manifest_download(appid, false, shared_state)
            .await
    }

    #[cfg(feature = "install")]
    pub async fn verify_game(
        &self,
        appid: u32,
        shared_state: Arc<std::sync::RwLock<crate::core::models::DownloadState>>,
    ) -> Result<Receiver<DownloadProgress>> {
        self.start_manifest_download(appid, true, shared_state)
            .await
    }

    #[cfg(feature = "install")]
    pub(crate) async fn start_manifest_download(
        &self,
        appid: u32,
        verify_mode: bool,
        shared_state: Arc<std::sync::RwLock<crate::core::models::DownloadState>>,
    ) -> Result<Receiver<DownloadProgress>> {
        let connection = self.require_connection_owned()?;

        let install_root = self.install_root_for_app(appid).await?;
        let manifest_path = self.appmanifest_path(appid).await?;
        let (tx, rx) = tokio::sync::mpsc::channel(128);

        let (local_manifests, active_branch) = self
            .local_manifest_info_for_appid(appid)
            .await
            .unwrap_or_else(|_| (HashMap::new(), "public".to_string()));

        let client_clone = self.clone();
        let shared_state_clone = shared_state.clone();
        // The name Steam already recorded for this copy.
        let recorded_name = std::fs::read_to_string(&manifest_path)
            .ok()
            .and_then(|raw| parse_name_from_acf(&raw))
            .filter(|n| !n.starts_with("App "));
        let game_name = match recorded_name.clone() {
            Some(name) => name,
            None => self.resolve_install_game_name(appid).await,
        };
        tokio::task::spawn(async move {
            // `on_manifest` fills the total per-depot.
            if let Ok(mut state) = shared_state_clone.write() {
                state.begin(
                    appid,
                    game_name.clone(),
                    0,
                    format!("Preparing operation for {}...", game_name),
                );
            }

            emit_queued(
                &tx,
                if verify_mode {
                    "verifying installed chunks"
                } else {
                    "resolving latest manifest"
                },
            )
            .await;

            let remote_manifests = if verify_mode {
                local_manifests.clone()
            } else {
                let mut remote =
                    SteamClient::remote_manifest_ids_static(&connection, appid, &active_branch)
                        .await
                        .unwrap_or_default();
                // An update refreshes only installed depots
                if !local_manifests.is_empty() {
                    remote.retain(|depot_id, _| local_manifests.contains_key(depot_id));
                }
                remote
            };

            let selections: Vec<ManifestSelection> = remote_manifests
                .iter()
                .map(|(depot_id, manifest_id)| ManifestSelection {
                    app_id: appid,
                    depot_id: *depot_id as u32,
                    manifest_id: *manifest_id,
                    appinfo_vdf: String::new(),
                })
                .collect();

            if selections.is_empty() {
                // In verify mode the selections come from the local appmanifest's
                // `InstalledDepots`. Empty means the app isn't fully installed (e.g.
                // only staged/partially downloaded), which is otherwise reported with
                // a confusing "no manifest/depot available" — spell it out instead.
                let message = if verify_mode {
                    format!(
                        "app {appid} has no installed depots to verify — it is not fully \
                         installed (its appmanifest lists no completed depots, e.g. a \
                         staged or partial download). Run `gaki-cli install {appid}` to \
                         complete the installation."
                    )
                } else {
                    format!(
                        "no manifest/depot available for app {appid} (no downloadable \
                         depot was resolved for the active branch)"
                    )
                };
                emit_failed(&tx, message).await;
                return;
            };

            // Hosts first: fail before reporter spawns.
            let Some(hosts) = fetch_content_hosts(&client_clone, &connection, &tx, appid).await
            else {
                if let Ok(mut state) = shared_state_clone.write() {
                    state.finish("Operation failed or paused");
                }
                return;
            };

            // Periodically forward the live byte counters over the channel.
            spawn_progress_reporter(
                tx.clone(),
                shared_state_clone.clone(),
                if verify_mode {
                    DownloadProgressState::Verifying
                } else {
                    DownloadProgressState::Downloading
                },
            );

            // Zero grand total: per-depot accumulation.
            let Some(successful_depots) = run_depot_loop(
                &client_clone,
                &connection,
                &tx,
                &shared_state_clone,
                appid,
                selections,
                &install_root,
                &hosts,
                &DepotLoopOpts {
                    verify_mode,
                    grand_total_bytes: 0,
                    manifest_overrides: None,
                },
            )
            .await
            else {
                if let Ok(mut state) = shared_state_clone.write() {
                    state.finish("Operation failed or paused");
                }
                return;
            };

            if let Ok(mut state) = shared_state_clone.write() {
                state.is_downloading = false;
                state.status_text = "Operation complete".to_string();
            }

            // The content was written into `install_root`
            let game_name = match recorded_name {
                Some(name) => name,
                None => client_clone.resolve_install_game_name(appid).await,
            };
            let installdir = install_root
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| sanitize_install_dir(&game_name));
            // Record the current build so Steam sees the install as up to date.
            let build_id =
                SteamClient::remote_buildid_static(&connection, appid, &active_branch).await;

            if let Err(err) = SteamClient::write_appmanifest(
                &manifest_path,
                appid,
                &game_name,
                &installdir,
                successful_depots,
                build_id.as_deref(),
                true,
                false,
            ) {
                tracing::warn!("failed writing appmanifest for {}: {}", appid, err);
            }
            emit_completed(
                &tx,
                if verify_mode {
                    "verify completed"
                } else {
                    "update completed"
                },
            )
            .await;
        });

        Ok(rx)
    }
}

/// Cloud state held across a launch: saves are pulled before the game starts
/// and pushed after it exits.
#[cfg(feature = "cloud")]
struct LaunchCloud {
    client: CloudClient,
    resolver: CloudPathResolver,
    specs: Vec<UfsSaveSpec>,
}

#[cfg(feature = "cloud")]
impl SteamClient {
    /// Pull the game's Cloud saves before launch. Best-effort: problems are
    /// logged and the game launches with whatever is on disk.
    async fn launch_cloud_sync_down(
        &mut self,
        app: &LibraryGame,
        wine_prefix: &Path,
    ) -> Result<LaunchCloud> {
        let client = CloudClient::new(self.require_connection_owned()?);
        let remote_root = default_cloud_root(client.steam_id(), app.app_id)?.join("remote");
        // `%Win*%` cloud roots point inside the game's Proton prefix; without it
        // every Auto-Cloud save of a Windows game would be silently skipped.
        let resolver =
            CloudPathResolver::new(remote_root, app.install_path.as_ref().map(PathBuf::from))
                .with_wine_prefix(Some(wine_prefix.to_path_buf()));
        tracing::info!(appid = app.app_id, "Syncing Cloud...");
        // Conflict-safe: a divergent save is left untouched (never clobbered),
        // so the user can resolve it via `cloud sync` / the Heroic chooser. The
        // game launches with whatever is currently on disk.
        match client.sync_down(app.app_id, &resolver).await {
            Ok(outcome) => {
                if outcome.has_conflicts() {
                    tracing::warn!(
                        appid = app.app_id,
                        "{} Cloud save(s) diverged from local — left untouched; resolve with `gaki-cli cloud sync`",
                        outcome.conflicts.len()
                    );
                }
                // Never let this pass unremarked: the game is about to start
                // with saves missing that the user believes were synced.
                if outcome.has_skips() {
                    tracing::warn!(
                        appid = app.app_id,
                        "{} Cloud save(s) could not be placed on disk (unmapped root token(s): {}) — the game may start without them; see `gaki-cli cloud sync {}`",
                        outcome.skipped.len(),
                        outcome
                            .skipped_tokens()
                            .iter()
                            .map(|t| format!("%{t}%"))
                            .collect::<Vec<_>>()
                            .join(", "),
                        app.app_id
                    );
                }
                // The worst case for the player: enough files arrived that the
                // game starts, but not the ones holding their progress.
                if outcome.has_failures() {
                    tracing::error!(
                        appid = app.app_id,
                        "{} Cloud save(s) FAILED to download — the save set on disk is incomplete and the game may show no progress. Run `gaki-cli cloud sync {} --down` before playing. First error: {}",
                        outcome.failed.len(),
                        app.app_id,
                        outcome.failed.first().map_or("", |f| f.error.as_str())
                    );
                }
            }
            Err(e) => tracing::error!(
                appid = app.app_id,
                "Cloud sync-down failed, launching anyway — saves may be missing or incomplete: {e:#}"
            ),
        }
        // UFS rules let sync_up discover brand-new local saves; best-effort.
        let specs = self
            .fetch_ufs_save_specs(app.app_id)
            .await
            .unwrap_or_default();
        Ok(LaunchCloud {
            client,
            resolver,
            specs,
        })
    }
}

#[cfg(feature = "cloud")]
impl LaunchCloud {
    /// Push saves after the game exits. Best-effort, like the pull.
    async fn sync_up(&self, app_id: u32) {
        // The game has already run and exited, so a cloud-upload failure must not
        // be surfaced as a launch failure. Log it and continue (this mirrors the
        // best-effort sync_down before launch).
        match self
            .client
            .sync_up(app_id, &self.resolver, &self.specs)
            .await
        {
            Ok(outcome) if outcome.has_conflicts() => tracing::warn!(
                appid = app_id,
                "{} Cloud save(s) diverged on upload — left untouched; resolve with `gaki-cli cloud sync`",
                outcome.conflicts.len()
            ),
            Ok(_) => tracing::info!(appid = app_id, "Upload Complete"),
            Err(e) => {
                tracing::warn!(appid = app_id, "Cloud upload failed (continuing): {e:#}")
            }
        }
    }
}
