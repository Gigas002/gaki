use super::*;

const MANIFEST: &str = r#""AppState"
{
	"appid"		"2835570"
	"name"		"Buckshot Roulette"
	"StateFlags"		"4"
	"installdir"		"Buckshot Roulette"
	"LastUpdated"		"1794318443"
	"SizeOnDisk"		"1330370018"
	"buildid"		"16736575"
	"LastOwner"		"0"
	"InstalledDepots"
	{
		"2835572"
		{
			"manifest"		"7472777116297885824"
			"size"		"1330370018"
		}
	}
	"UserConfig"
	{
		"BetaKey"		"beta"
	}
}
"#;

#[test]
fn top_level_fields_parse() {
    let m = parse_app_manifest(MANIFEST);
    assert_eq!(m.app_id, Some(2835570));
    assert_eq!(m.install_dir.as_deref(), Some("Buckshot Roulette"));
    assert_eq!(m.display_name().as_deref(), Some("Buckshot Roulette"));
    assert_eq!(m.size_on_disk, Some(1_330_370_018));
    assert_eq!(m.build_id, 16736575);
    assert_eq!(m.last_owner, None, "0 is no owner");
    assert_eq!(m.active_branch, "beta");
    assert!(m.fully_installed() && !m.update_pending());
}

#[test]
fn missing_fields_are_none() {
    let m = parse_app_manifest("\"AppState\"\n{\n\t\"appid\"\t\t\"1\"\n}\n");
    assert_eq!(m.size_on_disk, None);
    assert_eq!(m.active_branch, "public");
    assert!(!m.fully_installed());
}
