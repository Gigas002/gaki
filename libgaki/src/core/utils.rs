use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Extract the double-quoted values from a single VDF/ACF-style line, in order.
///
/// e.g. `"installdir"  "Half-Life 2"` -> `["installdir", "Half-Life 2"]`. Shared by the
/// appinfo/appmanifest/workshop-manifest parsers (see `steam_client`, `library`).
pub fn extract_quoted_values(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_quote = false;
    let mut current = String::new();
    for ch in line.chars() {
        if ch == '"' {
            if in_quote {
                out.push(std::mem::take(&mut current));
            }
            in_quote = !in_quote;
            continue;
        }
        if in_quote {
            current.push(ch);
        }
    }
    out
}

/// True only for one plain directory name.
pub fn is_safe_dir_name(name: &str) -> bool {
    let mut parts = Path::new(name).components();
    !name.trim().is_empty()
        && !name.contains(['/', '\\'])
        && matches!(
            (parts.next(), parts.next()),
            (Some(std::path::Component::Normal(_)), None)
        )
}

/// Current Unix time in seconds.
pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn build_runner_command(runner_path: &Path) -> Result<Command> {
    let mut final_path = runner_path.to_path_buf();

    // 1. Directory Resolution: If it's a directory, find the binary
    if final_path.is_dir() {
        if final_path.join("proton").exists() {
            final_path.push("proton");
        } else if final_path.join("bin/wine").exists() {
            final_path.push("bin/wine");
        } else if final_path.join("bin/wine64").exists() {
            final_path.push("bin/wine64");
        }
    }

    // 2. Identification and Command Building
    if let Some(file_name) = final_path.file_name().and_then(|f| f.to_str()) {
        if file_name == "proton" {
            let mut cmd = Command::new(&final_path);
            cmd.arg("run");
            return Ok(cmd);
        }
        if file_name == "wine" || file_name == "wine64" {
            return Ok(Command::new(&final_path));
        }
    }

    // 3. Last Resort: Just return the command if it exists
    if final_path.exists() && final_path.is_file() {
        return Ok(Command::new(&final_path));
    }

    bail!(
        "Failed to resolve a valid runner binary from {}",
        runner_path.display()
    )
}

/// Normalize a runtime name for fuzzy matching: lowercase, alphanumerics only.
/// So `experimental`, `Proton Experimental` and the on-disk `Proton - Experimental`
/// all reduce to comparable forms.
fn normalize_runner_name(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Resolve a Proton/Wine runtime *name* (e.g. `Proton 9.0`, `GE-Proton9-20`, the
/// legacy default `experimental`) or path to an on-disk runtime directory/binary.
///
/// Looks in [`crate::compat::proton::search_dirs`] order: the system-wide and
/// user `compatibilitytools.d`, then the Steam library's `steamapps/common`. Falls
/// back to a normalized fuzzy match so a configured name like `experimental`
/// still finds Steam's `Proton - Experimental` directory. Returns the name as a
/// path if nothing matches (the caller surfaces a clear error).
pub fn resolve_runner(name: &str, library_root: &Path) -> PathBuf {
    resolve_runner_opt(name, library_root).unwrap_or_else(|| {
        // Fallback: return the name verbatim as a path so the caller surfaces a clear
        // "binary not found" error. The warn lives here (not in resolve_runner_opt) so
        // callers that only want to *probe* resolvability can do so quietly.
        tracing::warn!(
            runner = %name,
            "could not resolve runner '{}' to an on-disk runtime under any known search path; \
             returning the name verbatim as a path (the launch will likely fail resolving the binary)",
            name
        );
        Path::new(name).to_path_buf()
    })
}

/// The resolution core behind [`resolve_runner`], returning `None` (quietly) when the
/// name matches no on-disk runtime. Use this when you want to *check* whether a runner
/// resolves without emitting the not-found warning [`resolve_runner`] logs.
pub fn resolve_runner_opt(name: &str, library_root: &Path) -> Option<PathBuf> {
    let name_path = Path::new(name);
    if name_path.is_absolute() || name_path.exists() {
        return Some(name_path.to_path_buf());
    }

    // System-wide, then user compatibilitytools.d, then Steam-installed Proton.
    let search_dirs: Vec<PathBuf> = crate::compat::proton::search_dirs(library_root)
        .into_iter()
        .map(|(dir, _)| dir)
        .collect();

    // 1. Exact directory-name match (fast path: "Proton 9.0", "GE-Proton9-20").
    for dir in &search_dirs {
        let exact = dir.join(name);
        if exact.exists() {
            return Some(exact);
        }
    }

    // 2. Fuzzy match on normalized names ("experimental" → "Proton - Experimental").
    let needle = normalize_runner_name(name);
    if !needle.is_empty() {
        let mut matches: Vec<PathBuf> = Vec::new();
        for dir in &search_dirs {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    if !entry.path().is_dir() {
                        continue;
                    }
                    let cand = normalize_runner_name(&entry.file_name().to_string_lossy());
                    if !cand.is_empty()
                        && (cand == needle || cand.contains(&needle) || needle.contains(&cand))
                    {
                        matches.push(entry.path());
                    }
                }
            }
        }
        // Prefer an exact normalized match; otherwise the shortest (closest) name.
        matches.sort_by_key(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().len())
                .unwrap_or(usize::MAX)
        });
        if let Some(exact) = matches.iter().find(|p| {
            normalize_runner_name(&p.file_name().unwrap_or_default().to_string_lossy()) == needle
        }) {
            return Some(exact.clone());
        }
        if let Some(first) = matches.into_iter().next() {
            return Some(first);
        }
    }

    None
}

/// Coarse classification of a runner directory tree, used to decide how a process
/// must be invoked. See [`classify_runner`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum RunnerKind {
    /// A Proton tree: has a `proton` python launch script at the root and/or the
    /// Proton `files/bin/wine` layout. The game is launched through `proton run`
    /// (so protonfixes apply), but a bare wine binary for background Steam must be
    /// taken from `files/bin/wine64` directly — never `proton run`.
    Proton,
    /// A wine-tkg build: `bin/wine`(64) present, no `proton` script, name marks tkg.
    WineTkg,
    /// A plain upstream/custom Wine build: `bin/wine`(64) present, no `proton` script.
    PlainWine,
    /// None of the above — not a usable runner tree.
    Unknown,
}

/// Classify a runner directory by inspecting its on-disk layout.
///
/// Detection order matters: a Proton tree is identified first (it owns a `proton`
/// launch script and/or the `files/bin/wine` layout), then bare Wine trees
/// (`bin/wine`/`bin/wine64` with no `proton` script), split into wine-tkg vs plain
/// Wine by a name heuristic.
pub(crate) fn classify_runner(runner_dir: &Path) -> RunnerKind {
    // Proton: the `proton` launch script at the root is the canonical marker (it is
    // what `build_runner_command` probes to append `run`). The Proton `files/bin/wine`
    // layout is an additional signal.
    if runner_dir.join("proton").exists()
        || runner_dir.join("files/bin/wine").exists()
        || runner_dir.join("files/bin/wine64").exists()
    {
        return RunnerKind::Proton;
    }

    // Bare Wine tree: wine binaries under `bin/` with no Proton wrapper.
    if runner_dir.join("bin/wine").exists() || runner_dir.join("bin/wine64").exists() {
        let name = runner_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if name.contains("tkg") {
            return RunnerKind::WineTkg;
        }
        return RunnerKind::PlainWine;
    }

    RunnerKind::Unknown
}

pub fn setup_fake_steam_trap(config_dir: &Path) -> Result<PathBuf> {
    let trap_dir = config_dir.join("fake_env");
    std::fs::create_dir_all(&trap_dir)?;

    let dummy_script = "#!/bin/sh\nexit 0\n";

    let write_executable = |path: &Path| -> Result<()> {
        if path.exists() {
            return Ok(());
        }
        std::fs::write(path, dummy_script)?;
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(path)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(path, perms)?;
        }
        Ok(())
    };

    write_executable(&trap_dir.join("steam"))?;
    write_executable(&trap_dir.join("steam.sh"))?;

    Ok(trap_dir)
}

/// The host's real Steam client install directory, suitable as
/// `STEAM_COMPAT_CLIENT_INSTALL_PATH` so Proton's `lsteamclient` bridge can reach
/// the running Steam client (Steamworks online features, Family-Sharing licences).
/// Returns the first existing standard location, or `None` if Steam isn't found.
pub fn host_steam_client_path() -> Option<PathBuf> {
    crate::core::config::detect_steam_path()
}

/// Whether the host's Steam client is currently running. Checks `~/.steam/steam.pid`
/// first (the client writes its PID there), then falls back to scanning `/proc` for
/// the `steam` or `steamwebhelper` processes. Detection must be reliable: a
/// false negative would make [`ensure_steam_running`] run `steam` again, which only
/// brings the already-open client to the foreground (the opposite of `-silent`).
pub fn is_steam_running() -> bool {
    // ~/.steam/steam.pid holds the running client's PID.
    if let Ok(home) = std::env::var("HOME")
        && let Ok(pid) = std::fs::read_to_string(format!("{home}/.steam/steam.pid"))
        && let Ok(pid) = pid.trim().parse::<u32>()
        && std::path::Path::new(&format!("/proc/{pid}")).exists()
    {
        return true;
    }
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return false;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name.to_str() else { continue };
        if !pid.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        if let Ok(comm) = std::fs::read_to_string(entry.path().join("comm")) {
            let comm = comm.trim();
            if comm == "steam" || comm == "steamwebhelper" {
                return true;
            }
        }
    }
    false
}

/// Start the host's Steam client minimised to the tray (`steam -silent`) if it
/// isn't already running, so Steamworks/Family-Sharing has a client to talk to.
/// Fully detached (its own session, no stdio) so it outlives this launch and never
/// blocks it. Best-effort: a failure to start is logged, not fatal.
pub fn ensure_steam_running() {
    if is_steam_running() {
        tracing::info!("Steam client already running; not starting it");
        return;
    }
    tracing::info!("Steam client not running; starting `steam -silent` (tray)");
    match start_steam_detached() {
        Ok(()) => tracing::info!("Launched Steam client (-silent)"),
        Err(e) => tracing::warn!("could not start Steam client: {e:#}"),
    }
}

/// Spawn `steam -silent` fully detached (its own session, no stdio), so it
/// outlives this process and never blocks it.
pub fn start_steam_detached() -> Result<()> {
    let mut cmd = std::process::Command::new("steam");
    cmd.arg("-silent")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // Detach into its own session/process group so Steam keeps running independently
    // of this launch (and isn't torn down with the game's process tree on stop).
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!("failed to run `steam -silent`: {e}"))
}

/// Ask the running Steam client to exit (`steam -shutdown`) and wait up to
/// `timeout` for it to go away. Steam rewrites appmanifests and
/// `libraryfolders.vdf` on exit, so callers editing those must wait for this.
pub fn shutdown_steam(timeout: std::time::Duration) -> Result<()> {
    let status = std::process::Command::new("steam")
        .arg("-shutdown")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| anyhow::anyhow!("failed to run `steam -shutdown`: {e}"))?;
    if !status.success() {
        bail!("`steam -shutdown` exited with {status}");
    }
    if !wait_until(
        || !is_steam_running(),
        timeout,
        std::time::Duration::from_millis(250),
    ) {
        bail!(
            "Steam is still running {}s after `steam -shutdown`; close it manually",
            timeout.as_secs()
        );
    }
    Ok(())
}

/// Poll `done` every `interval` until it returns `true` or `timeout` passes.
/// Returns whether it finished in time.
pub fn wait_until(
    mut done: impl FnMut() -> bool,
    timeout: std::time::Duration,
    interval: std::time::Duration,
) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if done() {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(interval);
    }
}

/// Latest logged-on state in connection-log text.
fn connection_log_logged_on(text: &str) -> Option<bool> {
    let mut state = None;
    for line in text.lines() {
        if line.contains("[Logged On") {
            state = Some(true);
        } else if line.contains("[Logged Off") {
            state = Some(false);
        }
    }
    state
}

/// Read the final `max_bytes` of a file.
fn read_tail(path: &Path, max_bytes: u64) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    f.seek(SeekFrom::Start(len.saturating_sub(max_bytes)))
        .ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// Host Steam finished logging on.
pub fn steam_is_logged_on() -> bool {
    let Some(root) = crate::core::config::detect_steam_path() else {
        return false;
    };
    let log = root.join("logs/connection_log.txt");
    let Some(text) = read_tail(&log, 64 * 1024) else {
        return false;
    };
    connection_log_logged_on(&text).unwrap_or(false)
}

/// Wait until host Steam is logged on.
pub async fn wait_for_steam_logged_on(timeout: std::time::Duration) -> bool {
    let start = std::time::Instant::now();
    let mut announced = false;
    loop {
        if steam_is_logged_on() {
            return true;
        }
        if start.elapsed() >= timeout {
            return false;
        }
        if !announced {
            tracing::info!("waiting for host Steam to reach logged-on before launch");
            announced = true;
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RunnerComponents {
    pub dxvk: Option<ComponentInfo>,
    pub vkd3d_proton: Option<ComponentInfo>,
    pub vkd3d: Option<ComponentInfo>,
    pub nvapi: Option<ComponentInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentInfo {
    pub version: String,
    pub source: ComponentSource,
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ComponentSource {
    BundledWithRunner,
    InstalledInPrefix,
    SystemWide,
}

impl std::fmt::Display for ComponentSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BundledWithRunner => write!(f, "bundled"),
            Self::InstalledInPrefix => write!(f, "in prefix"),
            Self::SystemWide => write!(f, "system"),
        }
    }
}

pub fn derive_runner_root(binary_path: &Path) -> PathBuf {
    let parent = if binary_path.is_file() {
        binary_path.parent().unwrap_or(binary_path)
    } else {
        binary_path
    };
    // If it's in a 'bin' directory (like wine-tkg), the root is one level up
    if parent.file_name().is_some_and(|n| n == "bin") {
        return parent.parent().unwrap_or(parent).to_path_buf();
    }

    // Otherwise (like proton script), the root is the parent directory
    parent.to_path_buf()
}

pub fn detect_runner_components(runner_path: &Path, wineprefix: Option<&Path>) -> RunnerComponents {
    let root = derive_runner_root(runner_path);

    RunnerComponents {
        dxvk: detect_dxvk(&root, wineprefix),
        vkd3d_proton: detect_vkd3d_proton(&root, wineprefix),
        vkd3d: detect_vkd3d(&root, wineprefix),
        nvapi: detect_nvapi(&root, wineprefix),
    }
}

// ── DXVK ────────────────────────────────────────────────────────────────────

fn detect_dxvk(root: &Path, prefix: Option<&Path>) -> Option<ComponentInfo> {
    // 1. Bundled inside runner (Modern Wine-TKG layout)
    if let Some(info) = detect_bundled_modern(
        root,
        &crate::compat::proton::wine_component_dirs("dxvk"),
        |_| {
            &[
                "d3d11.dll",
                "dxgi.dll",
                "d3d9.dll",
                "d3d8.dll",
                "d3d10core.dll",
            ]
        },
    ) {
        return Some(info);
    }

    // Legacy/Proton fallback
    if let Some(info) = check_bundled_legacy(root, "dxvk", "d3d11.dll") {
        return Some(info);
    }

    // 2. Installed into WINEPREFIX (winetricks / manual)
    if let Some(pfx) = prefix {
        let prefix_dlls = [
            "drive_c/windows/system32/d3d11.dll",
            "drive_c/windows/syswow64/d3d11.dll",
        ];
        if let Some(info) = check_prefix(pfx, &prefix_dlls) {
            return Some(info);
        }
    }

    // 3. System-wide (package manager install)
    let system_paths = [
        "/usr/share/dxvk/x64/d3d11.dll",
        "/usr/lib/dxvk/d3d11.dll",
        "/usr/lib/x86_64-linux-gnu/dxvk/d3d11.dll",
        "/usr/local/share/dxvk/x64/d3d11.dll",
    ];
    check_system(&system_paths)
}

// ── VKD3D-Proton ─────────────────────────────────────────────────────────────

fn detect_vkd3d_proton(root: &Path, prefix: Option<&Path>) -> Option<ComponentInfo> {
    // 1. Modern Wine-TKG layout
    if let Some(info) = detect_bundled_modern(
        root,
        &crate::compat::proton::wine_component_dirs("vkd3d-proton"),
        |_| &["d3d12.dll", "d3d12core.dll"],
    ) {
        return Some(info);
    }

    // Legacy/Proton fallback
    if let Some(info) = check_bundled_legacy(root, "vkd3d-proton", "d3d12.dll") {
        return Some(info);
    }

    // VKD3D-Proton replaces d3d12.dll — check prefix for it
    if let Some(pfx) = prefix
        && let Some(info) = check_prefix_d3d12(pfx, true)
    {
        return Some(info);
    }

    let system_paths = [
        "/usr/share/vkd3d-proton/x64/d3d12.dll",
        "/usr/lib/vkd3d-proton/d3d12.dll",
        "/usr/local/share/vkd3d-proton/x64/d3d12.dll",
    ];
    check_system(&system_paths)
}

// ── VKD3D (upstream) ─────────────────────────────────────────────────────────

fn detect_nvapi(root: &Path, prefix: Option<&Path>) -> Option<ComponentInfo> {
    // 1. Bundled inside runner (Modern Wine-TKG layout)
    if let Some(info) = detect_bundled_modern(
        root,
        &crate::compat::proton::wine_component_dirs("nvapi"),
        |arch| {
            if arch == "x86_64-windows" {
                &["nvapi64.dll"]
            } else {
                &["nvapi.dll"]
            }
        },
    ) {
        return Some(info);
    }

    // 2. Installed into WINEPREFIX
    if let Some(pfx) = prefix {
        let prefix_dlls = [
            "drive_c/windows/system32/nvapi64.dll",
            "drive_c/windows/syswow64/nvapi.dll",
        ];
        if let Some(info) = check_prefix(pfx, &prefix_dlls) {
            return Some(info);
        }
    }

    None
}

fn detect_vkd3d(root: &Path, prefix: Option<&Path>) -> Option<ComponentInfo> {
    // 1. Modern Wine-TKG layout
    if let Some(info) = detect_bundled_modern(
        root,
        &crate::compat::proton::wine_component_dirs("vkd3d"),
        |_| &["libvkd3d-1.dll", "libvkd3d-shader-1.dll"],
    ) {
        return Some(info);
    }

    // Legacy/Proton fallback (upstream libvkd3d-1.dll).
    if let Some(info) = check_bundled_legacy(root, "vkd3d", "libvkd3d-1.dll") {
        return Some(info);
    }

    if let Some(pfx) = prefix
        && let Some(info) = check_prefix_d3d12(pfx, false)
    {
        return Some(info);
    }

    let system_paths = [
        "/usr/lib/x86_64-linux-gnu/libvkd3d.so.1",
        "/usr/lib64/libvkd3d.so.1",
        "/usr/local/lib/libvkd3d.so.1",
    ];
    check_system(&system_paths)
}

// ── Shared helpers ────────────────────────────────────────────────────────────

/// Detect a component bundled inside a runner using the modern Wine-TKG layout:
/// `<root>/<subdir>/<arch>/<dlls>` with a `version` file in the arch or component
/// folder. `required_for_arch` yields the DLLs that must all be present for a given
/// arch (NVAPI ships arch-specific names, the rest share one list).
fn detect_bundled_modern<S: AsRef<str>>(
    root: &Path,
    subdirs: &[S],
    required_for_arch: impl Fn(&str) -> &'static [&'static str],
) -> Option<ComponentInfo> {
    for subdir in subdirs {
        let comp_path = root.join(subdir.as_ref());
        if !comp_path.is_dir() {
            continue;
        }
        // Only the Windows PE arch dirs hold the DLLs we match here; the `-unix`
        // WOW64 bridge dirs from ARCH_SUBDIRS are irrelevant to this check.
        for arch in crate::compat::proton::ARCH_SUBDIRS
            .iter()
            .filter(|a| a.ends_with("-windows"))
        {
            let arch = *arch;
            let arch_path = comp_path.join(arch);
            let required = required_for_arch(arch);
            if required.iter().all(|dll| arch_path.join(dll).exists()) {
                let version = ["version", "../version"] // check in arch or component folder
                    .iter()
                    .filter_map(|v| std::fs::read_to_string(arch_path.join(v)).ok())
                    .map(|s| parse_short_version(&s))
                    .find(|s| s != "unknown")
                    .unwrap_or_else(|| "found".to_string());
                return Some(ComponentInfo {
                    version,
                    source: ComponentSource::BundledWithRunner,
                    path: Some(arch_path),
                });
            }
        }
    }
    None
}

/// Legacy bundled-layout probe per component.
fn check_bundled_legacy(root: &Path, component: &str, dll: &str) -> Option<ComponentInfo> {
    let dll_candidates: Vec<String> = [
        "files/lib64",
        "files/lib",
        "dist/lib64",
        "dist/lib",
        "lib64",
        "lib",
    ]
    .iter()
    .map(|base| format!("{base}/wine/{component}/{dll}"))
    .collect();
    let version_files: Vec<String> = ["files/share", "dist/share", "share"]
        .iter()
        .map(|base| format!("{base}/{component}/version"))
        .collect();
    let dll_refs: Vec<&str> = dll_candidates.iter().map(String::as_str).collect();
    let version_refs: Vec<&str> = version_files.iter().map(String::as_str).collect();
    check_bundled(root, &dll_refs, &version_refs)
}

/// Attribute prefix `d3d12.dll` by marker.
fn check_prefix_d3d12(pfx: &Path, want_proton: bool) -> Option<ComponentInfo> {
    for rel in [
        "drive_c/windows/system32/d3d12.dll",
        "drive_c/windows/syswow64/d3d12.dll",
    ] {
        let p = pfx.join(rel);
        if p.exists() && dll_contains_string(&p, "vkd3d-proton") == want_proton {
            let version = extract_version_from_dll(&p).unwrap_or_else(|| "unknown".to_string());
            return Some(ComponentInfo {
                version,
                source: ComponentSource::InstalledInPrefix,
                path: Some(p),
            });
        }
    }
    None
}

fn check_bundled(
    root: &Path,
    dll_candidates: &[&str],
    version_files: &[&str],
) -> Option<ComponentInfo> {
    let found_dll = dll_candidates.iter().find(|rel| root.join(rel).exists())?;
    tracing::debug!(
        "Found bundled component DLL at: {}",
        root.join(found_dll).display()
    );

    let version = version_files
        .iter()
        .filter_map(|rel| {
            let p = root.join(rel);
            if p.exists() {
                tracing::debug!("Found version file: {}", p.display());
                std::fs::read_to_string(p).ok()
            } else {
                None
            }
        })
        .map(|s| parse_short_version(&s))
        .find(|s| s != "unknown")
        .or_else(|| {
            dll_candidates
                .iter()
                .map(|rel| root.join(rel))
                .find(|p| p.exists())
                .and_then(|p| extract_version_from_dll(&p))
        })
        .unwrap_or_else(|| "unknown".to_string());

    Some(ComponentInfo {
        version,
        source: ComponentSource::BundledWithRunner,
        path: root.join(found_dll).parent().map(|p| p.to_path_buf()),
    })
}

fn check_prefix(prefix: &Path, dll_candidates: &[&str]) -> Option<ComponentInfo> {
    for rel in dll_candidates {
        let p = prefix.join(rel);
        if p.exists() {
            // Exclude Wine's own built-in wined3d stubs (very small, < 50KB)
            let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            if size < 51_200 {
                continue;
            }

            let version = extract_version_from_dll(&p).unwrap_or_else(|| "unknown".to_string());
            return Some(ComponentInfo {
                version,
                source: ComponentSource::InstalledInPrefix,
                path: Some(p),
            });
        }
    }
    None
}

fn check_system(paths: &[&str]) -> Option<ComponentInfo> {
    for path in paths {
        let p = Path::new(path);
        if p.exists() {
            let version = extract_version_from_dll(p)
                .or_else(|| read_adjacent_version_file(p))
                .unwrap_or_else(|| "unknown".to_string());
            return Some(ComponentInfo {
                version,
                source: ComponentSource::SystemWide,
                path: Some(p.to_path_buf()),
            });
        }
    }
    None
}

fn read_adjacent_version_file(dll: &Path) -> Option<String> {
    let parent = dll.parent()?;
    let version_file = parent.join("version");
    std::fs::read_to_string(version_file)
        .ok()
        .map(|s| parse_short_version(&s))
        .filter(|s| !s.is_empty())
}

pub fn parse_short_version(s: &str) -> String {
    let s = s.trim();
    if s.is_empty() {
        return "unknown".to_string();
    }

    // Try to find content inside parentheses first (Wine-TKG style)
    let v = if let (Some(start), Some(end)) = (s.find('('), s.rfind(')')) {
        if start < end { &s[start + 1..end] } else { s }
    } else {
        // If no parentheses, it might be a simple version string
        // or a Wine-TKG style without () but with multiple space-separated parts.
        if s.contains(' ') {
            s.split_whitespace().last().unwrap_or(s)
        } else {
            s
        }
    };

    let mut v = v.trim();

    // Strip component name prefixes (like 'dxvk-', 'vkd3d-proton-', 'vkd3d-')
    for prefix in &["vkd3d-proton-", "vkd3d-", "dxvk-"] {
        if let Some(stripped) = v.strip_prefix(prefix) {
            v = stripped;
            break;
        }
    }

    // Strip leading 'v' if followed by a digit
    if v.starts_with('v') && v.len() > 1 && v.as_bytes()[1].is_ascii_digit() {
        v = &v[1..];
    }

    // Strip trailing git hash suffix: -g[0-9a-f]{7,10}
    if let Some(hyphen_idx) = v.rfind("-g") {
        let suffix = &v[hyphen_idx + 2..];
        if suffix.len() >= 7 && suffix.len() <= 10 && suffix.chars().all(|c| c.is_ascii_hexdigit())
        {
            return v[..hyphen_idx].to_string();
        }
    }

    v.to_string()
}

fn dll_contains_string(path: &Path, needle: &str) -> bool {
    let needle_lower = needle.to_ascii_lowercase();
    std::fs::read(path)
        .map(|bytes| {
            bytes.windows(needle.len()).any(|w| {
                w.iter()
                    .zip(needle_lower.bytes())
                    .all(|(b, n)| b.to_ascii_lowercase() == n)
            })
        })
        .unwrap_or(false)
}

fn extract_version_from_dll(dll_path: &Path) -> Option<String> {
    let data = std::fs::read(dll_path).ok()?;

    // Collect all printable ASCII runs of length >= 4
    let mut runs: Vec<String> = Vec::new();
    let mut current = Vec::new();
    for &byte in &data {
        if (0x20..0x7f).contains(&byte) {
            current.push(byte as char);
        } else {
            if current.len() >= 4 {
                runs.push(current.iter().collect());
            }
            current.clear();
        }
    }
    if current.len() >= 4 {
        runs.push(current.iter().collect());
    }

    // Match semver-like patterns: optional 'v', digits, dots, optional suffix
    // e.g. "2.3.1", "v1.10.3", "2.4-dirty", "v2.0.0-alpha.1+git"
    static SEMVER_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"^v?(\d{1,3})\.(\d{1,3})(\.\d{1,3})?([-.][a-zA-Z0-9._-]+)?$").unwrap()
    });

    // Prefer strings that look like "vX.Y.Z" over bare "X.Y"
    let mut candidates: Vec<String> = runs
        .into_iter()
        .filter(|s| SEMVER_RE.is_match(s))
        .filter(|s| {
            // Exclude obviously non-version strings (all zeros, single digit etc.)
            let parts: Vec<&str> = s.trim_start_matches('v').splitn(2, '.').collect();
            parts.len() >= 2 && parts[0].parse::<u32>().unwrap_or(100) <= 99
        })
        .collect();

    // Sort: longer (more specific) versions first
    candidates.sort_by_key(|c| std::cmp::Reverse(c.len()));
    candidates.into_iter().next()
}

/// Returns the WINEDLLOVERRIDES string needed to activate installed layers.
#[expect(
    clippy::too_many_arguments,
    reason = "flag soup; replaced by resolved profile settings in Phase 5"
)]
pub fn build_dll_overrides(
    dxvk_active: bool,
    vkd3d_proton_active: bool,
    vkd3d_active: bool,
    no_overlay: bool,
    force_builtin_d3d: bool,
    game_dir: Option<&std::path::Path>,
    strict_dxvk: bool,
    steam_enabled: bool,
) -> String {
    // In standalone mode gaki neutralises Steam's client DLLs and disables
    // `lsteamclient` so a game runs without a Steam client. When Steam is enabled
    // those must be left to Proton's defaults (`lsteamclient` loads builtin and
    // bridges to the running client), otherwise Steamworks init fails and
    // Family-Shared / online games can't start.
    let mut overrides: Vec<String> = if steam_enabled {
        Vec::new()
    } else {
        vec![
            "vstdlib_s=n".into(),
            "tier0_s=n".into(),
            "steamclient=n".into(),
            "steamclient64=n".into(),
            "steam_api=n".into(),
            "steam_api64=n".into(),
            "lsteamclient=".into(),
        ]
    };

    if no_overlay {
        overrides.push("GameOverlayRenderer=n".into());
        overrides.push("GameOverlayRenderer64=n".into());
    }

    if force_builtin_d3d {
        // Explicitly force Wine's own builtins for all D3D DLLs.
        // This overrides any native DLL sitting in the prefix's system32
        // from a previous DXVK/VKD3D install.
        for dll in &[
            "d3d8",
            "d3d9",
            "d3d10core",
            "d3d11",
            "dxgi",
            "d3d12",
            "d3d12core",
        ] {
            overrides.push(format!("{dll}=b"));
        }
        return overrides.join(";");
    }

    if dxvk_active {
        // If the game ships its own d3d DLLs, don't fight them — just
        // ensure native wins without specifying which native.
        // Wine searches exe-dir before system32, so "n,b" is fine UNLESS
        // a foreign dll landed in system32. We skip the override entirely
        // for DLLs the game already provides locally.
        let game_has = |dll: &str| -> bool { game_dir.is_some_and(|d| d.join(dll).exists()) };

        for dll in &[
            "d3d8.dll",
            "d3d9.dll",
            "d3d10core.dll",
            "d3d11.dll",
            "dxgi.dll",
        ] {
            let stem = dll.trim_end_matches(".dll");
            let mode = if strict_dxvk { "n" } else { "n,b" };

            if strict_dxvk || !game_has(dll) {
                overrides.push(format!("{stem}={mode}"));
            }
            // If the game ships it locally and we are not in strict mode,
            // leave Wine's default search order alone — exe-dir native wins automatically.
        }
    }

    if vkd3d_proton_active || vkd3d_active {
        overrides.push("d3d12=n,b".into());
        overrides.push("d3d12core=n,b".into());
        if vkd3d_active {
            overrides.push("libvkd3d-1=n,b".into());
            overrides.push("libvkd3d-shader-1=n,b".into());
        }
    }

    overrides.join(";")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedGpu {
    pub name: String,
    pub pci_id: Option<String>,
    pub is_discrete: bool,
}

pub fn list_available_gpus() -> Vec<DetectedGpu> {
    let mut gpus = Vec::new();

    // Try scanning /sys/class/drm/card* to find GPUs
    // This is more reliable than just checking /dev/dri/
    let drm_path = Path::new("/sys/class/drm");
    if let Ok(entries) = std::fs::read_dir(drm_path) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("card") && !name.contains('-') {
                let card_path = entry.path();

                // Read vendor and device IDs if available
                let device_path = card_path.join("device");
                let vendor = std::fs::read_to_string(device_path.join("vendor"))
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
                let device = std::fs::read_to_string(device_path.join("device"))
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();

                let pci_id = if !vendor.is_empty() && !device.is_empty() {
                    Some(format!(
                        "{}:{}",
                        vendor.replace("0x", ""),
                        device.replace("0x", "")
                    ))
                } else {
                    None
                };

                // Heuristic for discrete vs integrated
                // This is a bit simplified, but often works on Linux
                let is_discrete = pci_id.as_ref().is_some_and(|id| {
                    // NVIDIA, AMD (discrete), etc.
                    id.starts_with("10de") || id.starts_with("1002")
                });

                let gpu_name = match pci_id.as_deref() {
                    Some(id) if id.starts_with("10de") => format!("NVIDIA GPU ({})", name),
                    Some(id) if id.starts_with("1002") => format!("AMD GPU ({})", name),
                    Some(id) if id.starts_with("8086") => format!("Intel GPU ({})", name),
                    _ => format!("Unknown GPU ({})", name),
                };

                gpus.push(DetectedGpu {
                    name: gpu_name,
                    pci_id,
                    is_discrete,
                });
            }
        }
    }

    // Fallback if /sys scan failed but we have NVIDIA tools or similar
    if gpus.is_empty() && Path::new("/dev/nvidia0").exists() {
        gpus.push(DetectedGpu {
            name: "NVIDIA Discrete GPU".to_string(),
            pci_id: Some("10de:unknown".to_string()),
            is_discrete: true,
        });
    }

    gpus.sort_by_key(|g| std::cmp::Reverse(g.is_discrete));
    gpus
}

pub fn detect_exe_architecture(exe_path: &Path) -> crate::core::models::ExecutableArchitecture {
    use std::io::{Read, Seek, SeekFrom};

    let mut file = match std::fs::File::open(exe_path) {
        Ok(f) => f,
        Err(_) => return crate::core::models::ExecutableArchitecture::Unknown,
    };

    let mut mz_header = [0u8; 2];
    if file.read_exact(&mut mz_header).is_err() || &mz_header != b"MZ" {
        return crate::core::models::ExecutableArchitecture::Unknown;
    }

    // Offset 0x3C contains the offset to the PE header
    if file.seek(SeekFrom::Start(0x3C)).is_err() {
        return crate::core::models::ExecutableArchitecture::Unknown;
    }

    let mut pe_offset_buf = [0u8; 4];
    if file.read_exact(&mut pe_offset_buf).is_err() {
        return crate::core::models::ExecutableArchitecture::Unknown;
    }
    let pe_offset = u32::from_le_bytes(pe_offset_buf);

    if file.seek(SeekFrom::Start(pe_offset as u64)).is_err() {
        return crate::core::models::ExecutableArchitecture::Unknown;
    }

    let mut pe_signature = [0u8; 4];
    if file.read_exact(&mut pe_signature).is_err() || &pe_signature != b"PE\0\0" {
        return crate::core::models::ExecutableArchitecture::Unknown;
    }

    // COFF Header starts right after PE signature
    // Machine is the first 2 bytes
    let mut machine_buf = [0u8; 2];
    if file.read_exact(&mut machine_buf).is_err() {
        return crate::core::models::ExecutableArchitecture::Unknown;
    }
    let machine = u16::from_le_bytes(machine_buf);

    match machine {
        0x014c => crate::core::models::ExecutableArchitecture::X86,
        0x8664 => crate::core::models::ExecutableArchitecture::X86_64,
        _ => crate::core::models::ExecutableArchitecture::Unknown,
    }
}

/// Place a single DLL at `dest` as a symlink to `src` (a plain copy on non-unix),
/// first clearing any existing entry: a real (non-symlink) file is moved aside to
/// `*.dll.bak` (or removed if a backup already exists), and a stale symlink is
/// removed. When `log` is set the backup and symlink steps emit info-level traces;
/// the sibling-arch deploy passes `log = false` to stay quiet, preserving prior
/// behavior exactly.
fn deploy_one_dll_symlink(src: &Path, dest: &Path, log: bool) -> Result<()> {
    // Safety check: if it exists and is not a symlink, back it up or skip?
    // Usually we want to replace it if it's a Wine builtin.
    if dest.exists() {
        let meta = std::fs::symlink_metadata(dest)?;
        if !meta.file_type().is_symlink() {
            let backup = dest.with_extension("dll.bak");
            if !backup.exists() {
                if log {
                    tracing::info!(
                        "Backing up original DLL: {} -> {}",
                        dest.display(),
                        backup.display()
                    );
                }
                std::fs::rename(dest, &backup)?;
            } else {
                // Backup already exists, just remove the original to make room for symlink
                std::fs::remove_file(dest)?;
            }
        } else {
            // It's already a symlink, remove it to update
            std::fs::remove_file(dest)?;
        }
    }

    if log {
        tracing::info!("Symlinking {} -> {}", src.display(), dest.display());
    }
    std::os::unix::fs::symlink(src, dest)?;

    Ok(())
}

pub fn deploy_dll_symlinks(
    prefix: &Path,
    resolutions: &[crate::launch::dll_provider_resolver::DllResolution],
    target_arch: &crate::core::models::ExecutableArchitecture,
) -> Result<Vec<PathBuf>> {
    let mut deployed = Vec::new();
    let is_64bit_prefix = prefix.join("drive_c/windows/syswow64").exists();

    for res in resolutions {
        if res.chosen_provider != crate::launch::dll_provider_resolver::DllProvider::Runner
            && res.chosen_provider != crate::launch::dll_provider_resolver::DllProvider::Custom
        {
            continue;
        }

        if let Some(src_path) = &res.chosen_path {
            let dll_name = format!("{}.dll", res.name);

            // Determine destination directory in prefix
            let dest_dir = match target_arch {
                crate::core::models::ExecutableArchitecture::X86_64 => {
                    prefix.join("drive_c/windows/system32")
                }
                crate::core::models::ExecutableArchitecture::X86 => {
                    if is_64bit_prefix {
                        prefix.join("drive_c/windows/syswow64")
                    } else {
                        prefix.join("drive_c/windows/system32")
                    }
                }
                _ => continue,
            };

            if !dest_dir.exists() {
                continue;
            }

            let dest_path = dest_dir.join(&dll_name);

            deploy_one_dll_symlink(src_path, &dest_path, true)?;

            deployed.push(dest_path);

            // Also try to deploy the "other" architecture if it's a 64-bit prefix and we have it
            if is_64bit_prefix {
                let (other_arch, other_dir) = match target_arch {
                    crate::core::models::ExecutableArchitecture::X86_64 => (
                        crate::core::models::ExecutableArchitecture::X86,
                        prefix.join("drive_c/windows/syswow64"),
                    ),
                    crate::core::models::ExecutableArchitecture::X86 => (
                        crate::core::models::ExecutableArchitecture::X86_64,
                        prefix.join("drive_c/windows/system32"),
                    ),
                    _ => continue,
                };

                // We need to find the sibling DLL.
                // This is a bit tricky because we don't have the full resolution for the other arch here.
                // But we can guess based on common layouts.
                if let Some(other_src) = find_sibling_dll(src_path, target_arch, &other_arch) {
                    let other_dest = other_dir.join(&dll_name);
                    deploy_one_dll_symlink(&other_src, &other_dest, false)?;
                    deployed.push(other_dest);
                }
            }
        }
    }

    Ok(deployed)
}

fn find_sibling_dll(
    path: &Path,
    current_arch: &crate::core::models::ExecutableArchitecture,
    target_arch: &crate::core::models::ExecutableArchitecture,
) -> Option<PathBuf> {
    use crate::core::models::ExecutableArchitecture as Arch;
    let flip = match (current_arch, target_arch) {
        (Arch::X86_64, Arch::X86) => false,
        (Arch::X86, Arch::X86_64) => true,
        _ => return None,
    };

    let path_str = path.to_string_lossy();
    for (tag64, tag32) in [("x86_64", "i386"), ("x64", "x32")] {
        let (current_tag, target_tag) = if flip { (tag32, tag64) } else { (tag64, tag32) };
        if path_str.contains(current_tag) {
            let other_path = PathBuf::from(path_str.replace(current_tag, target_tag));
            if other_path.exists() {
                return Some(other_path);
            }
        }
    }

    None
}

pub fn cleanup_dll_symlinks(prefix: &Path) -> Result<()> {
    let target_dlls = [
        "d3d8.dll",
        "d3d9.dll",
        "dxgi.dll",
        "d3d10core.dll",
        "d3d11.dll",
        "d3d12.dll",
        "d3d12core.dll",
        "libvkd3d-1.dll",
        "libvkd3d-shader-1.dll",
    ];

    let dirs = [
        prefix.join("drive_c/windows/system32"),
        prefix.join("drive_c/windows/syswow64"),
    ];

    for dir in dirs {
        if !dir.exists() {
            continue;
        }
        for dll in &target_dlls {
            let p = dir.join(dll);
            if p.exists() {
                let meta = std::fs::symlink_metadata(&p)?;
                if meta.file_type().is_symlink() {
                    tracing::info!("Cleaning up symlink: {}", p.display());
                    std::fs::remove_file(&p)?;

                    // Restore backup if it exists
                    let backup = p.with_extension("dll.bak");
                    if backup.exists() {
                        tracing::info!("Restoring backup: {} -> {}", backup.display(), p.display());
                        std::fs::rename(&backup, &p)?;
                    }
                }
            }
        }
    }

    Ok(())
}

/// The game's wine prefix — the one Proton itself uses: `$STEAM_COMPAT_DATA_PATH/pfx`,
/// where `STEAM_COMPAT_DATA_PATH` is the game's profile's shared prefix, or
/// `<library>/steamapps/compatdata/<appid>` without one. Saves of Windows games
/// live here, so Steam Cloud sync resolves `%Win*%` roots against it.
///
/// Uses the game's configured settings; a launch with command-line overrides
/// uses [`crate::settings::LaunchSettings::wine_prefix`] instead.
pub fn proton_compat_prefix(
    config: &crate::core::config::LauncherConfig,
    app_id: u32,
) -> std::path::PathBuf {
    let library_root = std::path::Path::new(&config.steam_library_path);
    match crate::settings::resolve_user(app_id, &crate::settings::Overrides::default()) {
        Ok(settings) => settings.wine_prefix(library_root),
        Err(e) => {
            tracing::warn!(
                appid = app_id,
                "could not resolve the game's settings: {e:#}"
            );
            compat_data_dir(library_root, app_id).join("pfx")
        }
    }
}

/// `steamapps/compatdata/<appid>` under a library root.
pub fn compat_data_dir(library_root: &Path, app_id: u32) -> PathBuf {
    library_root
        .join("steamapps")
        .join("compatdata")
        .join(app_id.to_string())
}

/// Resolve `rel` under `base`, ignoring case.
///
/// Exact component matches win; `None` if missing. Names come from the directory
/// listing, not `exists()`: on a case-insensitive filesystem (NTFS, exFAT) any
/// spelling "exists", which would hand back the caller's spelling instead.
pub fn resolve_path_ignore_case(base: &Path, rel: &str) -> Option<PathBuf> {
    let mut current = if rel.starts_with('/') {
        PathBuf::from("/")
    } else {
        base.to_path_buf()
    };
    for part in rel
        .split(['/', '\\'])
        .filter(|p| !p.is_empty() && *p != ".")
    {
        let mut matches: Vec<PathBuf> = std::fs::read_dir(&current)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|n| n.eq_ignore_ascii_case(part))
            })
            .map(|e| e.path())
            .collect();
        let exact = current.join(part);
        current = if let Some(i) = matches
            .iter()
            .position(|m| m.file_name() == exact.file_name())
        {
            matches.swap_remove(i)
        } else if !matches.is_empty() {
            // Stable pick among case variants.
            matches.sort();
            matches.swap_remove(0)
        } else if exact.exists() {
            // Never listed, e.g. `..`.
            exact
        } else {
            return None;
        };
    }
    current.exists().then_some(current)
}

/// Free bytes on the filesystem that `path` lives on, or `None` if no mounted
/// disk contains it.
pub fn available_space_for(path: &Path) -> Option<u64> {
    let disks = sysinfo::Disks::new_with_refreshed_list().ok()?;
    disks
        .iter()
        .filter(|disk| path.starts_with(disk.mount_point()))
        // Longest matching mount point wins (a nested mount over its parent).
        .max_by_key(|disk| disk.mount_point().as_os_str().len())
        .map(|disk| disk.available_space())
}

/// Normalize a store country code: uppercase two letters; `UK` becomes `GB`.
pub fn normalize_country(raw: &str) -> Option<String> {
    let code = raw.trim().to_ascii_uppercase();
    if code.len() != 2 || !code.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    Some(if code == "UK" { "GB".to_string() } else { code })
}

/// Format a byte count as a human-readable size (binary units).
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    if bytes == 0 {
        return "0 B".to_string();
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

#[cfg(test)]
#[path = "utils_wait_until_tests.rs"]
mod wait_until_tests;

#[cfg(test)]
#[path = "utils_country_tests.rs"]
mod country_tests;

#[cfg(test)]
#[path = "utils_path_ignore_case_tests.rs"]
mod path_ignore_case_tests;

#[cfg(test)]
#[path = "utils_save_prefix_tests.rs"]
mod save_prefix_tests;

#[cfg(test)]
#[path = "utils_resolve_runner_tests.rs"]
mod resolve_runner_tests;

#[cfg(test)]
#[path = "utils_runner_classification_tests.rs"]
mod runner_classification_tests;

#[cfg(test)]
#[path = "utils_safe_dir_name_tests.rs"]
mod safe_dir_name_tests;

#[cfg(test)]
#[path = "utils_steam_logon_tests.rs"]
mod steam_logon_tests;

/// The MangoHud library on this system, if installed.
pub fn find_mangohud_lib() -> Option<PathBuf> {
    // Common install locations across distros
    let candidates = [
        "/usr/lib/mangohud/libMangoHud.so",
        "/usr/lib/mangohud/libMangoHud_dlsym.so",
        "/usr/lib/x86_64-linux-gnu/mangohud/libMangoHud.so",
        "/usr/lib64/mangohud/libMangoHud.so",
        "/usr/local/lib/mangohud/libMangoHud.so",
        "/usr/local/lib/x86_64-linux-gnu/mangohud/libMangoHud.so",
    ];

    for path in candidates {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }

    // Try ldconfig as fallback
    if let Ok(output) = std::process::Command::new("ldconfig").args(["-p"]).output() {
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            if line.contains("libMangoHud")
                && let Some(path) = line.split("=>").nth(1)
            {
                let p = PathBuf::from(path.trim());
                if p.exists() {
                    return Some(p);
                }
            }
        }
    }

    None
}
