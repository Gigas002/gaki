use std::{fs, os::unix::fs::PermissionsExt};

use super::*;

/// A config dir and home under one tempdir, with `bin/` as `PATH`.
struct Fixture {
    root: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("config/profiles")).unwrap();
        fs::create_dir_all(root.path().join("home")).unwrap();
        fs::create_dir_all(root.path().join("bin")).unwrap();
        Self { root }
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.root.path().join(rel)
    }

    /// A profile file gets its `name` from the file name.
    fn write(&self, rel: &str, text: &str) -> &Self {
        let path = self.path(rel);
        let text = match rel.strip_prefix("config/profiles/") {
            Some(file) => format!("name = {:?}\n{text}", file.trim_end_matches(".toml")),
            None => text.to_owned(),
        };
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
        self
    }

    fn executable(&self, rel: &str) -> &Self {
        self.write(rel, "#!/bin/sh\n");
        fs::set_permissions(self.path(rel), fs::Permissions::from_mode(0o755)).unwrap();
        self
    }

    fn catalog(&self) -> Catalog {
        let catalog = Catalog::load(&self.path("config"), &self.path("home"));
        assert!(catalog.load_errors.is_empty(), "{:?}", catalog.load_errors);
        catalog
    }

    /// Runs [`check`]; Proton names resolve to `<root>/compat/<name>`.
    fn check(&self) -> Vec<Finding> {
        let home = self.path("home");
        let bin = self.path("bin");
        let compat = self.path("compat");
        let find = |name: &str| Some(compat.join(name)).filter(|p| p.exists());
        check(
            &self.catalog(),
            &Host {
                home: &home,
                path: Some(bin.as_os_str()),
                find_proton: &find,
            },
        )
    }
}

fn of<'a>(findings: &'a [Finding], check: &str) -> Vec<&'a Finding> {
    findings.iter().filter(|f| f.check == check).collect()
}

#[test]
fn empty_config_checks_library_and_default_proton_only() {
    let fx = Fixture::new();
    let library = fx.path("library");
    fx.write(
        "config/config.toml",
        &format!("steam_library = {:?}\n", library.to_str().unwrap()),
    );
    let findings = fx.check();
    let checks: Vec<_> = findings.iter().map(|f| f.check).collect();
    assert_eq!(checks, ["library", "proton"], "{findings:#?}");
    assert_eq!(findings[0].level, Level::Warn, "missing library");
    assert_eq!(findings[1].level, Level::Warn, "no default build");
    assert_eq!(worst(&findings), Level::Warn);
}

#[test]
fn library_must_be_a_writable_directory() {
    let fx = Fixture::new();
    fx.write("config/config.toml", "steam_library = \"~/lib\"\n");
    fs::create_dir_all(fx.path("home/lib")).unwrap();
    assert_eq!(of(&fx.check(), "library")[0].level, Level::Ok);

    fs::set_permissions(fx.path("home/lib"), fs::Permissions::from_mode(0o555)).unwrap();
    let findings = fx.check();
    // root writes anywhere; the check is the kernel's.
    if !rustix::process::geteuid().is_root() {
        assert_eq!(of(&findings, "library")[0].level, Level::Error);
    }
    fs::set_permissions(fx.path("home/lib"), fs::Permissions::from_mode(0o755)).unwrap();

    fs::remove_dir(fx.path("home/lib")).unwrap();
    fx.write("home/lib", "");
    let findings = fx.check();
    let library = of(&findings, "library")[0];
    assert_eq!(library.level, Level::Error);
    assert!(library.message.contains("isn't a directory"), "{library:?}");
}

#[test]
fn proton_builds_need_the_proton_script() {
    let fx = Fixture::new();
    fx.write("config/config.toml", "default_proton = \"named\"\n")
        .write("compat/named/proton", "")
        .write("home/builds/good/proton", "")
        .write("home/builds/empty/version", "")
        .write(
            "config/profiles/good.toml",
            "runner = \"proton\"\n[proton]\nbuild = \"~/builds/good\"\n",
        )
        .write(
            "config/profiles/empty.toml",
            "runner = \"proton\"\n[proton]\nbuild = \"~/builds/empty\"\n",
        )
        .write(
            "config/profiles/gone.toml",
            "runner = \"proton\"\n[proton]\nbuild = \"GE-Proton1-1\"\n",
        )
        .write("config/profiles/default.toml", "runner = \"proton\"\n");
    let findings = fx.check();
    let proton = of(&findings, "proton");
    let levels: Vec<_> = proton
        .iter()
        .map(|f| (f.level, f.message.as_str()))
        .collect();
    assert_eq!(
        proton.len(),
        4,
        "default + 3 profiles with a build: {levels:#?}"
    );
    assert_eq!(proton[0].level, Level::Ok, "{levels:#?}");
    let line = |name: &str| {
        proton
            .iter()
            .find(|f| f.message.starts_with(&format!("profile `{name}`")))
            .unwrap()
    };
    assert_eq!(line("good").level, Level::Ok);
    assert_eq!(line("empty").level, Level::Error);
    assert!(line("empty").message.contains("no `proton` script"));
    assert_eq!(line("gone").level, Level::Error);
    assert!(line("gone").message.contains("isn't installed"));
}

#[test]
fn wrappers_resolve_like_a_launch() {
    let fx = Fixture::new();
    fx.executable("bin/mangohud")
        .executable("home/run.sh")
        .write(
            "config/profiles/p.toml",
            "runner = \"native\"\nwrappers = [\"mangohud --dlsym\", \"gamemoderun\"]\n",
        )
        .write(
            "config/profiles/slr.toml",
            "runner = \"slr\"\nwrappers = [\"left-to-sruman\"]\n[slr]\nruntime = \"steamrt4\"\n",
        )
        .write(
            "config/games.toml",
            "[[game]]\napp_id = 10\nname = \"cs\"\nwrappers = [\"~/run.sh\", \"~/gone.sh\"]\n",
        )
        .executable("bin/sruman");
    let findings = fx.check();
    let wrappers: Vec<_> = of(&findings, "wrappers")
        .into_iter()
        .map(|f| (f.level, f.message.clone()))
        .collect();
    assert_eq!(
        wrappers,
        [
            (
                Level::Error,
                "profile `p`: `gamemoderun` not found".to_owned()
            ),
            (Level::Error, "game `cs`: `~/gone.sh` not found".to_owned()),
        ]
    );

    fx.executable("bin/gamemoderun").executable("home/gone.sh");
    let findings = fx.check();
    let wrappers = of(&findings, "wrappers");
    assert_eq!(wrappers.len(), 1);
    assert_eq!(
        (wrappers[0].level, wrappers[0].message.as_str()),
        (Level::Ok, "4 wrappers found")
    );
}

#[test]
fn sruman_is_checked_only_when_needed() {
    let fx = Fixture::new();
    assert!(of(&fx.check(), "sruman").is_empty());

    fx.write("config/config.toml", "sruman_steam = true\n");
    assert_eq!(of(&fx.check(), "sruman")[0].level, Level::Warn);

    fx.write(
        "config/profiles/rt.toml",
        "runner = \"slr\"\n[slr]\nruntime = \"steamrt4\"\n",
    );
    let findings = fx.check();
    let sruman = of(&findings, "sruman")[0];
    assert_eq!(sruman.level, Level::Error);
    assert!(sruman.message.ends_with("`rt`"), "{sruman:?}");

    fx.executable("bin/sruman");
    assert_eq!(of(&fx.check(), "sruman")[0].level, Level::Ok);
}

#[test]
fn host_steam_needs_the_client() {
    let fx = Fixture::new();
    fx.write(
        "config/games.toml",
        "[[game]]\napp_id = 20\nsteam = \"auto\"\n",
    );
    assert!(of(&fx.check(), "steam").is_empty());

    fx.write(
        "config/games.toml",
        "[[game]]\napp_id = 20\nsteam = \"host\"\n",
    );
    let findings = fx.check();
    let steam = of(&findings, "steam")[0];
    assert_eq!(steam.level, Level::Error);
    assert!(steam.message.ends_with("game `20`"), "{steam:?}");

    fx.executable("bin/steam");
    assert_eq!(of(&fx.check(), "steam")[0].level, Level::Ok);
}

#[test]
fn config_counts_errors_and_warnings() {
    use crate::config::{Issue, Subject};
    let issue = |severity| Issue {
        severity,
        subject: Subject::Config,
        message: String::new(),
    };
    let report = |load_errors: usize, issues: Vec<Issue>| crate::ops::config::CheckReport {
        load_errors: vec![String::new(); load_errors],
        issues,
    };
    assert_eq!(config(&report(0, vec![])).level, Level::Ok);
    let warned = config(&report(0, vec![issue(Severity::Warning)]));
    assert_eq!(
        (warned.level, warned.message.as_str()),
        (Level::Warn, "1 warning in the config files")
    );
    let both = config(&report(
        1,
        vec![issue(Severity::Error), issue(Severity::Warning)],
    ));
    assert_eq!(
        (both.level, both.message.as_str()),
        (Level::Error, "2 errors and 1 warning in the config files")
    );
}

#[cfg(feature = "install")]
#[test]
fn fossilize_only_matters_with_precache() {
    let fx = Fixture::new();
    fx.write("config/config.toml", "shader_precache = false\n");
    assert!(fossilize(&fx.catalog(), None).is_none());
    fx.write("config/config.toml", "shader_precache = true\n");
    assert_eq!(fossilize(&fx.catalog(), None).unwrap().level, Level::Warn);
    let found = fossilize(&fx.catalog(), Some(Path::new("/x/fossilize_replay"))).unwrap();
    assert_eq!(found.level, Level::Ok);
}

#[cfg(feature = "authentication")]
#[test]
fn login_reads_the_stored_account() {
    use crate::auth::StoredAccount;
    assert_eq!(login(&StoredAccount::None).level, Level::Warn);
    assert_eq!(login(&StoredAccount::Unknown).level, Level::Ok);
    let named = login(&StoredAccount::Account("me".into()));
    assert_eq!(named.message, "signed in as me");
}
