// Based on steam-cdn (Apache-2.0), modified by Aurelia and gaki; see README.md.
use super::*;

const APPINFO: &str = r#"
"appinfo"
{
    "common" { "name" "Some Game" }
    "depots"
    {
        "1001"
        {
            "manifests"
            {
                "public" { "gid" "123" "size" "456" "download" "78" }
            }
            "encryptedmanifests"
            {
                "beta" { "gid" "999" }
            }
        }
        "branches" { "public" { "buildid" "1" } }
    }
}
"#;

#[test]
fn parses_depots_and_manifests() {
    let mut app = AppDepots::new(10);
    app.vdf_parse(APPINFO.as_bytes()).unwrap();
    assert_eq!(app.app_name.as_deref(), Some("Some Game"));
    assert_eq!(app.depots.len(), 1);
    let depot = &app.depots[0];
    assert_eq!(depot.depot_id, 1001);
    let public = depot
        .manifests
        .iter()
        .find(|m| m.branch == "public")
        .unwrap();
    assert_eq!(public.gid(), Some(123));
    assert_eq!(public.size, "456");
    assert!(!public.encrypted);
    let beta = depot.manifests.iter().find(|m| m.branch == "beta").unwrap();
    assert!(beta.encrypted);
    assert_eq!(beta.size, "");
}

#[test]
fn missing_depots_is_an_error() {
    let mut app = AppDepots::new(10);
    let vdf = r#""appinfo" { "common" { "name" "x" } }"#;
    assert!(app.vdf_parse(vdf.as_bytes()).is_err());
}
