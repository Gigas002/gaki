use super::*;

#[test]
fn builds_expected_urls() {
    assert_eq!(store_url(8850), "https://store.steampowered.com/app/8850");
    assert_eq!(
        header_url(8850),
        "https://cdn.cloudflare.steamstatic.com/steam/apps/8850/header.jpg"
    );
    assert_eq!(
        capsule_url(8850),
        "https://cdn.cloudflare.steamstatic.com/steam/apps/8850/library_600x900.jpg"
    );
    assert_eq!(
        small_capsule_url(8850),
        "https://cdn.cloudflare.steamstatic.com/steam/apps/8850/capsule_231x87.jpg"
    );
    assert_eq!(
        hero_url(8850),
        "https://cdn.cloudflare.steamstatic.com/steam/apps/8850/library_hero.jpg"
    );
    assert_eq!(
        logo_url(8850),
        "https://cdn.cloudflare.steamstatic.com/steam/apps/8850/logo.png"
    );
}

#[test]
fn builds_proton_compatibility_urls() {
    assert_eq!(protondb_url(8850), "https://www.protondb.com/app/8850");
    assert_eq!(
        proton_issues_url(8850),
        "https://github.com/ValveSoftware/Proton/issues?q=is%3Aissue+8850"
    );
}

#[test]
fn builds_steamdb_urls() {
    assert_eq!(steamdb_url(8850), "https://steamdb.info/app/8850/");
    assert_eq!(
        steamdb_depot_url(8851),
        "https://steamdb.info/depot/8851/manifests/"
    );
}
