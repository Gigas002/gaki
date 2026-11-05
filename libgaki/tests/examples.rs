//! Every file in `examples/` loads and validates cleanly.

use std::path::Path;

use libgaki::config::{Catalog, Platform, Runner, Severity, ValidationContext};

#[test]
fn examples_load_and_validate() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples");
    let home = Path::new("/home/user");
    let catalog = Catalog::load(&dir, home);
    assert!(
        catalog.load_errors.is_empty(),
        "load errors: {:?}",
        catalog.load_errors
    );

    let names: Vec<&str> = catalog.profiles.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["native", "proton", "proton-cachyos"]);
    assert_eq!(catalog.profile("native").unwrap().runner, Runner::Native);
    assert_eq!(catalog.games.len(), 2);
    assert_eq!(catalog.config.default_platform, Some(Platform::Linux));
    assert_eq!(catalog.game_by_alias("witcher3").unwrap().app_id, 292030);

    let issues = catalog.validate(&ValidationContext { home });
    assert!(
        issues.iter().all(|issue| issue.severity != Severity::Error),
        "{issues:#?}"
    );
    assert!(issues.is_empty(), "examples should not warn: {issues:#?}");
}
