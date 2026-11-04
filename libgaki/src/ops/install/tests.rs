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
