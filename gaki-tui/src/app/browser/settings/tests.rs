use super::*;

fn spec(key: &str) -> Spec {
    *SPECS.iter().find(|s| s.key == key).unwrap()
}

#[test]
fn every_config_key_has_a_spec_in_order() {
    let keys: Vec<&str> = SPECS.iter().map(|s| s.key).collect();
    let config: Vec<&str> = libgaki::ops::config::KEYS.iter().map(|(k, _)| *k).collect();
    assert_eq!(keys, config);
}

#[test]
fn a_toggle_flips_from_its_default() {
    let edit = spec("cloud_sync").edit;
    assert_eq!(next_value(edit, None, &[]), Some(Some("false".into())));
    assert_eq!(
        next_value(edit, Some("false"), &[]),
        Some(Some("true".into()))
    );
}

#[test]
fn a_choice_with_a_default_cycles_its_options() {
    let edit = spec("presence").edit;
    assert_eq!(next_value(edit, None, &[]), Some(Some("online".into())));
    assert_eq!(
        next_value(edit, Some("online"), &[]),
        Some(Some("offline".into()))
    );
}

#[test]
fn an_optional_choice_steps_through_unset() {
    let edit = spec("default_platform").edit;
    assert_eq!(next_value(edit, None, &[]), Some(Some("windows".into())));
    assert_eq!(
        next_value(edit, Some("windows"), &[]),
        Some(Some("linux".into()))
    );
    assert_eq!(next_value(edit, Some("linux"), &[]), Some(None));
}

#[test]
fn profiles_cycle_and_a_missing_one_starts_over() {
    let edit = spec("default_profile").edit;
    let profiles = vec!["a".to_string(), "b".to_string()];
    assert_eq!(next_value(edit, None, &profiles), Some(Some("a".into())));
    assert_eq!(next_value(edit, Some("b"), &profiles), Some(None));
    assert_eq!(
        next_value(edit, Some("gone"), &profiles),
        Some(None),
        "an unknown value goes to the first step"
    );
    assert_eq!(next_value(edit, None, &[]), Some(None), "no profiles");
}

#[test]
fn prompts_are_not_stepped() {
    assert_eq!(next_value(spec("proxy").edit, None, &[]), None);
}

#[test]
fn an_unchanged_value_is_not_saved_and_secret_store_asks() {
    let mut view = SettingsView::with_values(vec![None; SPECS.len()]);
    let presence = SPECS.iter().position(|s| s.key == "presence").unwrap();
    view.apply(presence, None);
    assert!(view.status.is_empty(), "nothing to save");

    let store = SPECS.iter().position(|s| s.key == "secret_store").unwrap();
    view.apply(store, Some("memory".into()));
    assert_eq!(
        view.confirm,
        Some(Pending {
            index: store,
            value: Some("memory".into())
        })
    );
    assert!(view.status.is_empty(), "not saved before the answer");
}

fn daemon(pid: u32) -> GakiProcess {
    GakiProcess {
        pid,
        is_daemon: true,
        command: "gaki-cli daemon".into(),
        config_dir: None,
        socket: None,
    }
}

fn browser_with_daemons(rows: Vec<GakiProcess>) -> Browser {
    let mut browser = Browser::new(vec![]);
    browser.settings.tab = Tab::Daemons;
    browser.settings.daemons = Some(DaemonsTab::with(rows));
    browser
}

#[test]
fn the_daemons_tab_follows_the_check_tab() {
    assert_eq!(Tab::Check.step(true), Tab::Daemons);
    assert_eq!(Tab::Daemons.step(true), Tab::Settings);
}

#[test]
fn stopping_a_daemon_asks_first_and_no_cancels() {
    // Fake PIDs, so a slip into the yes branch couldn't hit a real process
    // either: the answer here is always no.
    let mut browser = browser_with_daemons(vec![daemon(u32::MAX - 1), daemon(u32::MAX)]);
    browser.daemons_move(1);
    browser.begin_daemon_stop(false);
    assert!(browser.daemon_confirm_open());
    assert_eq!(
        browser.settings.daemons.as_ref().unwrap().confirm,
        Some(StopConfirm {
            pids: vec![u32::MAX],
            all: false
        })
    );
    browser.answer_daemon_stop(false);
    assert!(!browser.daemon_confirm_open());

    browser.begin_daemon_stop(true);
    assert_eq!(
        browser
            .settings
            .daemons
            .as_ref()
            .unwrap()
            .confirm
            .as_ref()
            .unwrap()
            .pids,
        vec![u32::MAX - 1, u32::MAX]
    );
    browser.answer_daemon_stop(false);
}

#[test]
fn stopping_with_no_daemons_says_so() {
    let mut browser = browser_with_daemons(Vec::new());
    browser.begin_daemon_stop(true);
    assert!(!browser.daemon_confirm_open());
    assert_eq!(browser.settings.status, "No daemon is running.");
}

#[test]
fn the_daemons_highlight_clamps() {
    let mut browser = browser_with_daemons(vec![daemon(u32::MAX - 1), daemon(u32::MAX)]);
    browser.daemons_move(5);
    assert_eq!(browser.settings.daemons.as_ref().unwrap().selected, 1);
    browser.daemons_move(-5);
    assert_eq!(browser.settings.daemons.as_ref().unwrap().selected, 0);
}
