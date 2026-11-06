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

#[test]
fn examples_resolve() {
    use libgaki::settings::{Overrides, ProtonBuild, resolve};

    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples");
    let home = Path::new("/home/user");
    let catalog = Catalog::load(&dir, home);

    let witcher = resolve(&catalog, 292030, &Overrides::default(), home).unwrap();
    assert_eq!(witcher.profile.as_deref(), Some("proton-cachyos"));
    assert_eq!(witcher.platform, Some(Platform::Windows));
    let proton = witcher.proton.unwrap();
    assert_eq!(
        proton.build,
        ProtonBuild::Path("/usr/share/steam/compatibilitytools.d/proton-cachyos".into())
    );
    assert_eq!(witcher.env["DXVK_HUD"], "fps");

    let dota = resolve(&catalog, 570, &Overrides::default(), home).unwrap();
    assert_eq!(dota.platform, Some(Platform::Linux));
    assert!(dota.proton.is_none());

    // Not in games.toml: `default_platform = "linux"` drops the proton default.
    let other = resolve(&catalog, 10, &Overrides::default(), home).unwrap();
    assert_eq!(other.profile, None);
    assert_eq!(other.platform, Some(Platform::Linux));
}

#[test]
fn the_library_example_parses() {
    use libgaki::config::Platform;
    use libgaki::library::transfer;

    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/library.toml");
    let file = transfer::read(&path).unwrap();
    let ids: Vec<u32> = file.games.iter().map(|g| g.app_id).collect();
    assert_eq!(ids, [1245620, 570, 620]);
    assert_eq!(file.games[0].platform, Some(Platform::Windows));
    assert_eq!(file.games[1].branch.as_deref(), Some("beta"));
    assert_eq!(file.games[2].name, "");
}
