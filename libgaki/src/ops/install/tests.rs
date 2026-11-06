use super::*;

// Each test uses its own app id: the registry is process-wide.

#[test]
fn second_registration_for_the_same_app_fails() {
    let (_guard, _) = InstallGuard::begin(9_000_001).unwrap();
    let err = InstallGuard::begin(9_000_001).err().unwrap();
    assert!(err.to_string().contains("already running"));
}

#[test]
fn drop_unregisters_and_raises_abort() {
    let (guard, state) = InstallGuard::begin(9_000_002).unwrap();
    assert!(active_installs().lock().unwrap().contains_key(&9_000_002));
    drop(guard);
    assert!(!active_installs().lock().unwrap().contains_key(&9_000_002));
    assert!(state.read().unwrap().abort_signal.load(Ordering::SeqCst));
    assert!(InstallGuard::begin(9_000_002).is_ok());
}

// --- Manifest overrides (downgrade) ---

#[test]
fn parallel_lists_pair_by_position() {
    let map = parse_manifest_overrides(&[10, 20], &["111".into(), "222".into()]).unwrap();
    assert_eq!(map.get(&10), Some(&111));
    assert_eq!(map.get(&20), Some(&222));
    assert_eq!(map.len(), 2);
}

#[test]
fn combined_form_carries_its_own_depot() {
    let map = parse_manifest_overrides(&[], &["10:111".into(), "20:222".into()]).unwrap();
    assert_eq!(map.get(&10), Some(&111));
    assert_eq!(map.get(&20), Some(&222));
}

#[test]
fn mixing_bare_and_combined_is_supported() {
    // One combined entry plus one parallel pair.
    let map = parse_manifest_overrides(&[20], &["10:111".into(), "222".into()]).unwrap();
    assert_eq!(map.get(&10), Some(&111));
    assert_eq!(map.get(&20), Some(&222));
}

#[test]
fn unequal_parallel_lists_are_rejected() {
    let err = parse_manifest_overrides(&[10, 20], &["111".into()]).unwrap_err();
    assert!(err.to_string().contains("equal numbers"), "got: {err}");
}

#[test]
fn empty_input_is_rejected() {
    let err = parse_manifest_overrides(&[], &[]).unwrap_err();
    assert!(err.to_string().contains("at least one"), "got: {err}");
}

#[test]
fn duplicate_depot_is_rejected() {
    let err = parse_manifest_overrides(&[10], &["10:111".into(), "222".into()]).unwrap_err();
    assert!(err.to_string().contains("more than once"), "got: {err}");
}

#[test]
fn non_numeric_ids_are_rejected() {
    assert!(parse_manifest_overrides(&[], &["abc".into()]).is_err());
    assert!(parse_manifest_overrides(&[], &["10:xyz".into()]).is_err());
}

// --- Platform, space and outcome ---

#[test]
fn windows_is_preferred_when_offered() {
    let both = [DepotPlatform::Linux, DepotPlatform::Windows];
    assert_eq!(auto_select_platform(&both), DepotPlatform::Windows);
    assert_eq!(
        auto_select_platform(&[DepotPlatform::Linux]),
        DepotPlatform::Linux
    );
    assert_eq!(auto_select_platform(&[]), DepotPlatform::Windows);
}

#[test]
fn too_little_space_is_disk_full() {
    assert!(check_space("/lib", 10, 10).is_ok());
    let err = check_space("/lib", 11, 10).unwrap_err();
    assert_eq!(classify(&err).kind, ErrorKind::DiskFull);
}

#[test]
fn a_paused_install_lands_paused() {
    let cancelled = TypedError::new(ErrorKind::Cancelled, "download stopped").into();
    let (state, err) = classify_outcome(5, cancelled, true);
    assert_eq!(state, BacklogState::Paused);
    assert_eq!(classify(&err).kind, ErrorKind::Cancelled);
}

#[test]
fn a_stopped_install_lands_aborted_and_a_failure_interrupted() {
    let cancelled = TypedError::new(ErrorKind::Cancelled, "download stopped").into();
    assert_eq!(
        classify_outcome(5, cancelled, false).0,
        BacklogState::Aborted
    );
    let failed = anyhow::anyhow!("network went away");
    assert_eq!(
        classify_outcome(5, failed, false).0,
        BacklogState::Interrupted
    );
}

#[test]
fn an_import_queues_new_entries_and_keeps_existing_ones() {
    use crate::config::Platform;
    use crate::library::transfer::LibraryEntry;

    let entry = |app_id, platform| LibraryEntry {
        app_id,
        name: format!("game {app_id}"),
        platform,
        branch: None,
    };
    let mut list = Vec::new();
    queue_in(&mut list, &[entry(10, Some(Platform::Linux))], "/old", 1);
    list[0].state = BacklogState::Paused;
    list[0].downloaded_bytes = 500;

    queue_in(&mut list, &[entry(10, None), entry(20, None)], "/games", 2);
    assert_eq!(list.len(), 2);
    let (kept, new) = (&list[0], &list[1]);
    assert_eq!(
        kept.state,
        BacklogState::Paused,
        "a backlog entry is left alone"
    );
    assert_eq!(
        (kept.downloaded_bytes, kept.library.as_str()),
        (500, "/old")
    );
    assert_eq!(kept.platform, "linux");
    assert_eq!(new.state, BacklogState::Queued);
    assert_eq!(
        (new.name.as_str(), new.library.as_str()),
        ("game 20", "/games")
    );
    assert_eq!(new.platform, "", "no platform: picked when it installs");
    assert_eq!((new.added_at, new.auto_attempts), (2, 0));
    assert!(
        backlog::auto_resumable(&list).is_empty(),
        "queued entries wait for a resume"
    );
}
