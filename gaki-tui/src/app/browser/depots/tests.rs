use super::*;

fn done<T>(result: Result<T, String>) -> Load<T> {
    Load::Loading(Arc::new(Mutex::new(Some(result))))
}

fn pending<T>() -> Load<T> {
    Load::Loading(Arc::new(Mutex::new(None)))
}

fn depot(id: u64) -> steam::Depot {
    steam::Depot {
        id,
        name: format!("Depot {id}"),
        size: 0,
        file_count: 0,
        config: String::new(),
        is_owned: None,
    }
}

#[test]
fn a_load_is_adopted_once() {
    let mut load = done(Ok(3));
    assert!(load.is_loading());
    assert!(load.poll());
    assert_eq!(load.ready(), Some(&3));
    assert!(!load.poll());

    let mut failed: Load<i32> = done(Err("offline".into()));
    assert!(failed.poll());
    assert_eq!(failed.error(), Some("offline"));

    let mut waiting: Load<i32> = pending();
    assert!(!waiting.poll());
    assert!(waiting.is_loading());
}

#[test]
fn spawned_jobs_land_in_their_slot() {
    let slot = spawn(|| Ok::<_, TuiError>(7));
    let result = loop {
        if let Some(r) = take(&slot) {
            break r;
        }
        std::thread::yield_now();
    };
    assert_eq!(result, Ok(7));
}

#[test]
fn depot_list_selection_clamps_after_loading() {
    let mut browser = Browser::new(vec![]);
    browser.show_depots = true;
    browser.depots.selected = 5;
    browser.depots.list = done(Ok(vec![depot(1), depot(2)]));
    browser.poll_depots();
    assert_eq!(browser.depots.selected, 1);
    browser.depots_next();
    assert_eq!(browser.depots.selected, 1);
    browser.depots_previous();
    browser.depots_previous();
    assert_eq!(browser.depots.selected, 0);
    assert_eq!(browser.depots.selected_depot().map(|d| d.id), Some(1));
}

#[test]
fn closing_drops_the_list() {
    let mut browser = Browser::new(vec![]);
    browser.show_depots = true;
    browser.depots.list = Load::Ready(vec![depot(1)]);
    browser.close_depots();
    assert!(!browser.show_depots);
    assert_eq!(browser.depots.list.ready().map(Vec::len), Some(0));
}

#[cfg(feature = "depot")]
mod file_view {
    use super::*;
    use crate::app::browser::depots::files::{
        FilesInput, FilesView, expand_dir, passes, xdg_download_dir,
    };
    use libgaki::ops::depot::{DepotFiles, EntryKind, ManifestFileEntry};
    use std::path::{Path, PathBuf};

    fn entry(path: &str, kind: EntryKind) -> ManifestFileEntry {
        ManifestFileEntry {
            path: path.to_string(),
            kind,
            size: 1,
            sha1: String::new(),
            chunks: 1,
            executable: false,
            link_target: None,
        }
    }

    fn browser_with_files() -> Browser {
        let mut browser = Browser::new(vec![]);
        browser.show_depots = true;
        browser.depots.app_id = 10;
        browser.depots.list = Load::Ready(vec![depot(11)]);
        browser.depots.files = Some(FilesView {
            depot_id: 11,
            depot_name: "Depot 11".into(),
            branch: "public".into(),
            listing: Load::Ready(DepotFiles {
                app_id: 10,
                depot_id: 11,
                manifest_id: 99,
                files: vec![
                    entry("bin", EntryKind::Directory),
                    entry("bin/Game.exe", EntryKind::File),
                    entry("bin/lib.so", EntryKind::Symlink),
                    entry("data/a.pak", EntryKind::File),
                ],
            }),
            filter: String::new(),
            selected: 0,
            input: None,
            status: String::new(),
        });
        browser
    }

    fn view(browser: &Browser) -> &FilesView {
        browser.depots.files.as_ref().unwrap()
    }

    #[test]
    fn filter_is_a_substring_or_a_glob() {
        assert!(passes("", "anything"));
        assert!(passes("game", "bin/Game.exe"));
        assert!(!passes("game", "data/a.pak"));
        assert!(passes("*.EXE", "bin/Game.exe"));
        assert!(!passes("*.exe", "bin/Game.exe.bak"));
        assert!(passes("bin/?ame*", "bin/Game.exe"));
    }

    #[test]
    fn typing_a_filter_narrows_the_list_live() {
        let mut browser = browser_with_files();
        browser.depot_files_move(3);
        browser.begin_depot_filter();
        assert!(browser.depot_input_open());
        for c in "pak".chars() {
            browser.depot_input_push(c);
        }
        let paths: Vec<_> = view(&browser)
            .visible()
            .iter()
            .map(|e| e.path.clone())
            .collect();
        assert_eq!(paths, ["data/a.pak"]);
        assert_eq!(view(&browser).selected, 0);
        browser.submit_depot_input();
        assert!(!browser.depot_input_open());
        assert_eq!(view(&browser).filter, "pak");

        // Esc in the filter clears it.
        browser.begin_depot_filter();
        browser.cancel_depot_input();
        assert_eq!(view(&browser).visible().len(), 4);
    }

    #[test]
    fn moving_clamps_to_the_visible_rows() {
        let mut browser = browser_with_files();
        browser.depot_files_move(10);
        assert_eq!(view(&browser).selected, 3);
        browser.depot_files_move(-10);
        assert_eq!(view(&browser).selected, 0);
    }

    #[test]
    fn only_files_can_be_downloaded() {
        let mut browser = browser_with_files();
        // Row 0 is the directory.
        browser.begin_depot_download();
        assert!(!browser.depot_input_open());
        assert!(view(&browser).status.contains("Only files"));

        browser.depot_files_move(1);
        browser.depots.save_dir = Some("/tmp/last".into());
        browser.begin_depot_download();
        assert_eq!(
            view(&browser).input,
            Some(FilesInput::SaveTo("/tmp/last".into()))
        );
    }

    #[test]
    fn one_download_at_a_time() {
        let mut browser = browser_with_files();
        browser.depot_files_move(1);
        browser.depots.download = Some(("x".into(), Arc::new(Mutex::new(None))));
        browser.begin_depot_download();
        assert!(!browser.depot_input_open());
        assert!(view(&browser).status.contains("already running"));
    }

    #[test]
    fn a_finished_download_reports_in_the_view_and_status_bar() {
        let mut browser = browser_with_files();
        browser.depots.download = Some((
            "Game.exe".into(),
            Arc::new(Mutex::new(Some(Ok(PathBuf::from("/d/Game.exe"))))),
        ));
        browser.poll_depots();
        assert!(browser.depots.download.is_none());
        assert_eq!(view(&browser).status, "Saved /d/Game.exe");
        assert_eq!(browser.notice.as_deref(), Some("Saved /d/Game.exe"));

        browser.depots.download = Some((
            "Game.exe".into(),
            Arc::new(Mutex::new(Some(Err("exists".into())))),
        ));
        browser.poll_depots();
        assert_eq!(
            browser.notice.as_deref(),
            Some("Couldn't download Game.exe: exists")
        );
    }

    #[test]
    fn branch_prompt_starts_from_the_current_branch() {
        let mut browser = browser_with_files();
        browser.begin_depot_branch();
        assert_eq!(
            view(&browser).input,
            Some(FilesInput::Branch("public".into()))
        );
        // Submitting the same branch keeps the listing.
        browser.submit_depot_input();
        assert!(view(&browser).listing.ready().is_some());
    }

    #[test]
    fn back_and_close() {
        let mut browser = browser_with_files();
        browser.close_depot_files();
        assert!(browser.depots.files.is_none());
        assert!(browser.show_depots);
    }

    #[test]
    fn xdg_download_dir_parsing() {
        let home = Path::new("/home/me");
        let text =
            "# comment\nXDG_DESKTOP_DIR=\"$HOME/Desktop\"\nXDG_DOWNLOAD_DIR=\"$HOME/Загрузки\"\n";
        assert_eq!(
            xdg_download_dir(text, home),
            Some(PathBuf::from("/home/me/Загрузки"))
        );
        assert_eq!(
            xdg_download_dir("XDG_DOWNLOAD_DIR=\"/data/dl\"", home),
            Some(PathBuf::from("/data/dl"))
        );
        assert_eq!(
            xdg_download_dir("XDG_DOWNLOAD_DIR=\"relative\"", home),
            None
        );
        assert_eq!(xdg_download_dir("", home), None);
    }

    #[test]
    fn expand_dir_handles_tilde() {
        let home = Path::new("/home/me");
        assert_eq!(expand_dir("~", home), PathBuf::from("/home/me"));
        assert_eq!(expand_dir(" ~/dl ", home), PathBuf::from("/home/me/dl"));
        assert_eq!(expand_dir("/abs", home), PathBuf::from("/abs"));
        assert_eq!(expand_dir("~user/x", home), PathBuf::from("~user/x"));
    }
}
