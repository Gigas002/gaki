//! Interface calls that now run in-process on `libgaki`, against an empty
//! config dir: nothing stored, so nothing reaches the network.

use gaki_tui::interface::aurelia;

/// One test: the config dir is set once per process.
#[test]
fn logged_out_session_is_reported_not_fetched() {
    let config = tempfile::tempdir().unwrap();
    assert!(libgaki::core::config::set_config_dir(
        config.path().to_path_buf()
    ));

    let health = aurelia::health().expect("health works offline");
    assert!(!health.logged_in);
    assert_eq!(health.account, None);
    assert!(!health.daemon);

    let err = aurelia::account().expect_err("no session, no account");
    assert!(
        err.to_string().contains("not logged in"),
        "unexpected error: {err}"
    );
}
