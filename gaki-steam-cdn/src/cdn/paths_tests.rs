// Part of gaki-steam-cdn (gaki's addition to the steam-cdn fork); see README.md.
use super::*;

#[test]
fn accepts_paths_inside() {
    assert_eq!(entry_path("bin/game").unwrap(), Path::new("bin/game"));
    assert_eq!(entry_path("./a/./b").unwrap(), Path::new("a/b"));
    assert_eq!(entry_path("a//b/").unwrap(), Path::new("a/b"));
}

#[test]
fn rejects_paths_outside() {
    for raw in ["/etc/passwd", "../x", "a/../../x", "a/..", "", ".", "/"] {
        assert!(
            matches!(entry_path(raw), Err(Error::UnsafePath(_))),
            "{raw:?}"
        );
    }
}

#[test]
fn link_targets() {
    let parent = Path::new("lib/x86_64");
    assert!(link_stays_inside(parent, "libfoo.so.1"));
    assert!(link_stays_inside(parent, "../../bin/tool"));
    assert!(link_stays_inside(parent, "./sub/libfoo.so.1"));
    assert!(!link_stays_inside(parent, "../../../outside"));
    assert!(!link_stays_inside(parent, "/usr/lib/libfoo.so"));
    // `..` after a name could follow a link first, so it never passes.
    assert!(!link_stays_inside(parent, "sub/../libfoo.so.1"));
    assert!(!link_stays_inside(Path::new(""), "b/.."));
    // At the top, a link can't climb at all.
    assert!(!link_stays_inside(Path::new(""), ".."));
    assert!(link_stays_inside(Path::new(""), "dir/file"));
    assert!(link_stays_inside(Path::new(""), "."));
}

#[cfg(unix)]
#[test]
fn on_disk_symlinks_are_followed_for_parents() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("root");
    let outside = tmp.path().join("outside");
    std::fs::create_dir_all(root.join("dir")).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();
    std::os::unix::fs::symlink("dir", root.join("inner")).unwrap();
    let root = root.canonicalize().unwrap();

    assert!(ensure_inside(&root, &root.join("dir/file")).is_ok());
    assert!(ensure_inside(&root, &root.join("new/deeper/file")).is_ok());
    assert!(ensure_inside(&root, &root.join("inner/file")).is_ok());
    assert!(matches!(
        ensure_inside(&root, &root.join("escape/file")),
        Err(Error::UnsafePath(_))
    ));
    assert!(matches!(
        ensure_inside(&root, &root.join("escape/new/file")),
        Err(Error::UnsafePath(_))
    ));
}

#[cfg(unix)]
#[test]
fn unlinks_only_symlinks() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("file");
    let link = tmp.path().join("link");
    std::fs::write(&file, b"x").unwrap();
    std::os::unix::fs::symlink(&file, &link).unwrap();

    unlink_symlink(&file).unwrap();
    assert!(file.exists());
    unlink_symlink(&link).unwrap();
    assert!(link.symlink_metadata().is_err());
    assert_eq!(std::fs::read(&file).unwrap(), b"x");
    unlink_symlink(&tmp.path().join("missing")).unwrap();
}

/// Plays a manifest's entries, in order, into a fresh install directory next
/// to an `outside` directory, and returns both.
#[cfg(unix)]
fn play(
    entries: Vec<(&str, Entry)>,
) -> (
    tempfile::TempDir,
    PathBuf,
    Vec<Result<Option<PathBuf>, Error>>,
) {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("root")).unwrap();
    std::fs::create_dir_all(tmp.path().join("outside")).unwrap();
    std::fs::write(tmp.path().join("outside/victim"), b"keep me").unwrap();
    let root = tmp.path().join("root").canonicalize().unwrap();
    let results = entries
        .into_iter()
        .map(|(raw, entry)| place(&root, raw, entry))
        .collect();
    (tmp, root, results)
}

#[cfg(unix)]
#[test]
fn places_an_ordinary_depot() {
    let (_tmp, root, results) = play(vec![
        ("bin", Entry::Dir),
        ("lib/libfoo.so.1", Entry::File),
        ("lib/libfoo.so", Entry::Link("libfoo.so.1".into())),
        ("bin/tool", Entry::Link("../lib/libfoo.so".into())),
    ]);
    assert!(results.iter().all(Result::is_ok));
    assert_eq!(
        results[1].as_ref().unwrap().as_deref(),
        Some(root.join("lib/libfoo.so.1").as_path())
    );
    assert_eq!(
        std::fs::read_link(root.join("bin/tool")).unwrap(),
        Path::new("../lib/libfoo.so")
    );
    assert!(root.join("bin").is_dir());
}

#[cfg(unix)]
#[test]
fn a_forged_manifest_stays_inside() {
    let (tmp, root, results) = play(vec![
        // Straight out.
        ("../outside/victim", Entry::Link("/etc/passwd".into())),
        ("/tmp/x", Entry::File),
        // A link that climbs, then entries through it.
        ("up", Entry::Link("..".into())),
        ("up/outside/victim", Entry::File),
        // A link to the root, then a link climbing out of it.
        ("here", Entry::Link(".".into())),
        ("here/escape", Entry::Link("..".into())),
        ("away", Entry::Link("here/..".into())),
        // An absolute link, then a directory through it.
        ("abs", Entry::Link("/".into())),
        ("abs/tmp/evil", Entry::Dir),
    ]);
    assert!(matches!(results[0], Err(Error::UnsafePath(_))));
    assert!(matches!(results[1], Err(Error::UnsafePath(_))));
    // The climbing link is skipped, so `up/...` became real directories here.
    let up = root.join("up").symlink_metadata().unwrap();
    assert!(up.is_dir() && !up.file_type().is_symlink());
    // `here -> .` is harmless and is created; the links climbing from it are not.
    assert!(root.join("here").symlink_metadata().is_ok());
    assert!(root.join("escape").symlink_metadata().is_err());
    assert!(root.join("away").symlink_metadata().is_err());
    // The absolute link is refused, so `abs/tmp/evil` is made right here.
    assert!(root.join("abs/tmp/evil").is_dir());
    assert!(
        !root
            .join("abs")
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink()
    );
    // Whatever was created is real directories and files under the root.
    assert_eq!(
        std::fs::read(tmp.path().join("outside/victim")).unwrap(),
        b"keep me"
    );
    for result in results.iter().flatten().flatten() {
        assert!(result.starts_with(&root), "{}", result.display());
    }
}

#[cfg(unix)]
#[test]
fn files_replace_planted_links() {
    let (tmp, root, _) = play(vec![]);
    let victim = tmp.path().join("outside/victim");
    std::os::unix::fs::symlink(&victim, root.join("game.bin")).unwrap();
    let path = place(&root, "game.bin", Entry::File).unwrap().unwrap();
    // The planted link is gone; the download creates a real file there.
    assert_eq!(path, root.join("game.bin"));
    assert!(path.symlink_metadata().is_err());
    assert_eq!(std::fs::read(&victim).unwrap(), b"keep me");
}

#[cfg(unix)]
#[test]
fn planted_directory_links_are_refused() {
    let (tmp, root, _) = play(vec![]);
    std::os::unix::fs::symlink(tmp.path().join("outside"), root.join("data")).unwrap();
    assert!(matches!(
        place(&root, "data/victim", Entry::File),
        Err(Error::UnsafePath(_))
    ));
    assert!(matches!(
        place(&root, "data/new", Entry::Dir),
        Err(Error::UnsafePath(_))
    ));
}
