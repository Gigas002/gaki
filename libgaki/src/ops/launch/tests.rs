use super::*;

fn game(installed: bool) -> LibraryGame {
    LibraryGame {
        app_id: 1,
        name: "Game".into(),
        playtime_forever_minutes: None,
        is_installed: installed,
        install_path: None,
        local_manifest_ids: Default::default(),
        update_available: false,
        update_queued: false,
        active_branch: "public".into(),
        is_owned: true,
        is_family_shared: false,
        online_required: None,
        platform: None,
    }
}

#[test]
fn an_uninstalled_game_is_refused() {
    assert!(require_installed(&game(true)).is_ok());
    let err = require_installed(&game(false)).unwrap_err();
    assert_eq!(
        crate::core::error::classify(&err).kind,
        ErrorKind::InvalidInput
    );
}
