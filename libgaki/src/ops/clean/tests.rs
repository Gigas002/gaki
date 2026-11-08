use super::*;

struct Fixture {
    _dir: tempfile::TempDir,
    layout: Layout,
}

fn write(path: &Path, bytes: usize) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, vec![b'x'; bytes]).unwrap();
}

fn summary(dir: &Path, app_id: u32, timestamp: u64) {
    write(&dir.join("stdout.log"), 10);
    let text = format!(
        "session_id = \"s\"\napp_id = {app_id}\nresult = \"Success\"\n\
         total_duration_ms = 1\nstage_durations_ms = {{}}\ntimestamp = {timestamp}\n"
    );
    std::fs::write(dir.join("summary.toml"), text).unwrap();
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let layout = Layout {
        images: root.join("config/images"),
        data: root.join("config"),
        library_cache: root.join("config/library_cache.toml"),
        tui_cache: root.join("cache/tui"),
        launch_logs: root.join("config/logs"),
        locks: root.join("state/installs"),
        shader_roots: vec![root.join("cache/shadercache"), root.join("fast")],
    };
    write(&layout.images.join("570.jpg"), 100);
    write(&layout.data.join("info_cache/570.us.toml"), 50);
    write(&layout.data.join("tag_names.toml"), 5);
    write(&layout.library_cache, 20);
    write(&layout.tui_cache.join("games.toml"), 30);
    // 570: two launches, keep the newer; 10: one launch, kept.
    summary(&layout.launch_logs.join("a"), 570, 1);
    summary(&layout.launch_logs.join("b"), 570, 2);
    summary(&layout.launch_logs.join("c"), 10, 1);
    write(&layout.locks.join("570.lock"), 0);
    write(&layout.locks.join("10.lock"), 0);
    // Shader caches: 570 installed, 440 not; 999 is a user's own folder.
    let shaders = &layout.shader_roots[0];
    write(&shaders.join("570/DXVK_state_cache/x"), 7);
    write(&shaders.join("440/mesa_shader_cache/x"), 9);
    write(&layout.shader_roots[1].join("999/notes.txt"), 3);
    // A config file next to the caches is never a candidate.
    write(&layout.data.join("config.toml"), 1);
    Fixture { _dir: dir, layout }
}

fn kinds(items: &[Item]) -> Vec<(Kind, String)> {
    items
        .iter()
        .map(|i| {
            let name = i.path.file_name().unwrap().to_string_lossy().into_owned();
            (i.kind, name)
        })
        .collect()
}

#[test]
fn everything_safe_by_default() {
    let fx = fixture();
    let installed = HashSet::from([570]);
    let items = plan_in(&fx.layout, &Selection::default(), &installed, &|id| {
        id == 10
    });
    assert_eq!(
        kinds(&items),
        [
            (Kind::Images, "images".into()),
            (Kind::Info, "info_cache".into()),
            (Kind::Info, "tag_names.toml".into()),
            (Kind::Library, "library_cache.toml".into()),
            (Kind::Tui, "tui".into()),
            (Kind::Logs, "a".into()),
            (Kind::Locks, "570.lock".into()),
            (Kind::Shaders, "440".into()),
        ]
    );
    assert_eq!(items[0].bytes, 100);
    assert_eq!(totals(&items)[&Kind::Info], (2, 55));
}

#[test]
fn all_shaders_and_subsets() {
    let fx = fixture();
    let installed = HashSet::from([570]);
    let selection = Selection {
        kinds: vec![Kind::Shaders],
        all_shaders: true,
    };
    let items = plan_in(&fx.layout, &selection, &installed, &|_| false);
    assert_eq!(
        kinds(&items),
        [(Kind::Shaders, "440".into()), (Kind::Shaders, "570".into())]
    );
}

#[test]
fn delete_empties_cache_dirs_and_removes_the_rest() {
    let fx = fixture();
    let items = plan_in(&fx.layout, &Selection::default(), &HashSet::new(), &|_| {
        false
    });
    let report = delete(&items);
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(report.deleted.len(), items.len());
    assert_eq!(
        report.freed_bytes,
        items.iter().map(|i| i.bytes).sum::<u64>()
    );

    let l = &fx.layout;
    assert!(l.images.is_dir() && is_empty_dir(&l.images));
    assert!(l.tui_cache.is_dir() && is_empty_dir(&l.tui_cache));
    assert!(!l.library_cache.exists());
    assert!(!l.launch_logs.join("a").exists());
    assert!(l.launch_logs.join("b").exists() && l.launch_logs.join("c").exists());
    assert!(!l.shader_roots[0].join("570").exists());
    assert!(l.shader_roots[1].join("999/notes.txt").exists());
    assert!(l.data.join("config.toml").exists());

    // Emptied directories aren't listed again.
    let again = plan_in(l, &Selection::default(), &HashSet::new(), &|_| false);
    assert!(
        again
            .iter()
            .all(|i| i.kind != Kind::Images && i.kind != Kind::Tui),
        "{again:?}"
    );
}
