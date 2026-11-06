use super::*;
use crate::app::game::Game;
use crate::app::game::status::GameStatus;
use crate::app::steam::{InstallAction, InstallControl, LibraryGame};

fn game(id: i32) -> Game {
    Game::from_library(LibraryGame {
        app_id: id as u32,
        name: "Game".to_string(),
        is_installed: false,
        install_path: None,
        update_available: false,
        is_owned: true,
        is_family_shared: false,
        platform: None,
        active_branch: None,
        assets: None,
        store_url: None,
    })
}

fn set_state(g: &Game, state: &str) {
    *g.status_counter().lock().unwrap() = Some(GameStatus {
        state: state.to_string(),
        installdir: String::new(),
        size: 0.0,
    });
}

#[test]
fn install_phase_only_tracks_started_installs() {
    let mut b = Browser::new(vec![]);
    let g = game(42);

    // An active-looking status on a game we never started installing is Idle.
    set_state(&g, "downloading 12.0%");
    assert_eq!(b.install_phase(&g), InstallPhase::Idle);

    // Once we begin an install, the live status drives the phase.
    b.begin_install(42, None);
    assert_eq!(b.install_phase(&g), InstallPhase::Active);

    set_state(&g, "paused 12.0%");
    assert_eq!(b.install_phase(&g), InstallPhase::Paused);

    // A finished install reads as Idle (so Space/c no longer act).
    set_state(&g, "Installed!");
    assert_eq!(b.install_phase(&g), InstallPhase::Idle);
}

#[test]
fn poll_game_refresh_patches_install_flags_and_clears_status() {
    let g = game(99); // helper builds it not-installed
    let mut b = Browser::new(vec![g]);
    // A leftover transient status from the finished install.
    set_state(b.items.iter().find(|x| x.id == 99).unwrap(), "Installed!");

    // Simulate the off-thread worker dropping a fresh "now installed" entry.
    b.game_refresh_slot.lock().unwrap().push(LibraryGame {
        app_id: 99,
        name: "Game".to_string(),
        is_installed: true,
        install_path: Some("dir".to_string()),
        update_available: false,
        is_owned: true,
        is_family_shared: false,
        platform: None,
        active_branch: None,
        assets: None,
        store_url: None,
    });

    b.poll_game_refresh();

    let updated = b.items.iter().find(|x| x.id == 99).unwrap();
    assert!(
        updated.installed,
        "installed flag adopted from refreshed entry"
    );
    // The transient status was dropped, so the badge now derives from the
    // installed flag (a steady "installed" state, not "Installed!").
    assert_eq!(
        updated.get_status().map(|s| s.state).as_deref(),
        Some("installed")
    );
}

#[test]
fn install_control_action_roundtrips() {
    let c = InstallControl::new();
    assert_eq!(c.get(), InstallAction::Running);
    c.set(InstallAction::Paused);
    assert_eq!(c.get(), InstallAction::Paused);
    c.set(InstallAction::Stopped);
    assert_eq!(c.get(), InstallAction::Stopped);
}
