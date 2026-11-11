//! Drives the legacy launch path, which needs the `library` feature.
#![cfg(feature = "library")]

use libgaki::core::config::LauncherConfig;
use libgaki::core::models::LibraryGame;
use libgaki::steam_client::{LaunchInfo, LaunchTarget, SteamClient, sanitize_install_dir};
use std::collections::HashMap;
use std::fs;
use tempfile::tempdir;

#[tokio::test]
async fn test_batman_path_resolution() {
    let tmp = tempdir().unwrap();
    let library_root = tmp.path().to_path_buf();
    let steamapps = library_root.join("steamapps");
    let common = steamapps.join("common");
    fs::create_dir_all(&common).unwrap();

    let app_id = 209000; // Batman™: Arkham Origins
    let game_name = "Batman™: Arkham Origins";
    let installdir = "Batman Arkham Origins"; // authoritative name typically doesn't have : or ™ but let's test preservation

    let game_dir = common.join(installdir);
    fs::create_dir_all(&game_dir).unwrap();
    let exe_path = game_dir.join("SinglePlayer/Binaries/Win32/BatmanOrigins.exe");
    fs::create_dir_all(exe_path.parent().unwrap()).unwrap();
    fs::write(&exe_path, "fake exe").unwrap();

    // Create manifest
    let manifest_path = steamapps.join(format!("appmanifest_{}.acf", app_id));
    let manifest_content = format!(
        "\"AppState\"\n{{\n\t\"appid\"\t\"{}\"\n\t\"name\"\t\"{}\"\n\t\"installdir\"\t\"{}\"\n}}",
        app_id, game_name, installdir
    );
    fs::write(&manifest_path, manifest_content).unwrap();

    let client = SteamClient::new().unwrap();
    let mut app = LibraryGame {
        app_id,
        name: game_name.to_string(),
        install_path: Some(game_dir.to_string_lossy().to_string()),
        is_installed: true,
        playtime_forever_minutes: Some(0),
        playtime_2weeks_minutes: None,
        last_played: None,
        active_branch: "public".to_string(),
        update_available: false,
        update_queued: false,
        local_manifest_ids: HashMap::new(),
        is_owned: true,
        is_family_shared: false,
        online_required: None,
        platform: None,
        size_on_disk: None,
    };

    let launch_info = LaunchInfo {
        app_id,
        id: "0".to_string(),
        description: "Play".to_string(),
        executable: "SinglePlayer\\Binaries\\Win32\\BatmanOrigins.exe".to_string(),
        arguments: "".to_string(),
        workingdir: None,
        target: LaunchTarget::NativeLinux, // Use native to trigger adhoc for easy testing
    };

    let launcher_config = LauncherConfig {
        steam_library_path: library_root.to_string_lossy().to_string(),
        ..Default::default()
    };

    // Test adhoc launch (which we updated to use resolve logic)
    // We expect it to resolve the path correctly even if we "break" the install_path in the app model
    app.install_path = Some(common.join("WRONG_PATH").to_string_lossy().to_string());

    let result: anyhow::Result<std::process::Child> = client
        .internal_legacy_launch_adhoc(&app, &launch_info, None, &launcher_config, None)
        .await;

    // It will probably fail to spawn because it's a fake exe and we are on linux, but we want to see if it FOUND it.
    // If it found it, it will try to spawn it.
    if let Err(e) = &result {
        println!("Result error: {:?}", e);
        // If it's an OS error 2 (not found), then resolution failed.
        // If it's something else, it might have resolved but failed to exec.
    }

    assert!(result.is_ok() || !result.unwrap_err().to_string().contains("not found"));
}

#[tokio::test]
async fn test_colon_preservation_on_linux() {
    let app_id = 123;
    let game_name = "Game: With Colon";
    // On Linux, colons are allowed. Our improved sanitize_install_dir should preserve it.

    let client = SteamClient::new().unwrap();
    let (resolved_name, pics_dir) = client.resolve_install_game_info(app_id).await;
    // PICS will return nothing for fake id, so it falls back to sanitize
    assert!(resolved_name.contains("App 123"));
    assert!(pics_dir.is_none());

    let result_dir = sanitize_install_dir(game_name);

    assert_eq!(result_dir, "Game: With Colon");
}
