//! Renders the depots overlay and its file view into an off-screen
//! `TestBackend` and checks what it paints.

use gaki_tui::app::browser::Browser;
use gaki_tui::app::browser::depots::Load;
use gaki_tui::app::steam::Depot;
use gaki_tui::ui;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn render(browser: &Browser, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| ui::depots::draw(frame, frame.area(), browser))
        .unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer.cell((x, y)).unwrap().symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn depot(id: u64, name: &str) -> Depot {
    Depot {
        id,
        name: name.to_string(),
        size: 1024 * 1024,
        file_count: 0,
        config: String::new(),
        is_owned: None,
    }
}

#[test]
fn the_depot_list_shows_each_depot_and_the_highlight() {
    let mut browser = Browser::new(vec![]);
    browser.show_depots = true;
    browser.depots.list = Load::Ready(vec![depot(11, "Content"), depot(12, "Linux")]);
    browser.depots.selected = 1;
    let screen = render(&browser, 70, 12);
    assert!(screen.contains("Depots (2)"), "{screen}");
    assert!(screen.contains("Content (11)"), "{screen}");
    assert!(screen.contains("▶ Linux (12)"), "{screen}");
}

#[test]
fn a_failed_depot_list_says_why() {
    let mut browser = Browser::new(vec![]);
    browser.show_depots = true;
    browser.depots.list = Load::Failed("offline".into());
    let screen = render(&browser, 70, 8);
    assert!(screen.contains("Couldn't load depots: offline"), "{screen}");
}

#[cfg(feature = "depot")]
#[test]
fn the_file_view_scrolls_to_the_highlight_and_shows_the_prompt() {
    use gaki_tui::app::browser::depots::files::{FilesInput, FilesView};
    use libgaki::ops::depot::{DepotFiles, EntryKind, ManifestFileEntry};

    let files = (0..50)
        .map(|i| ManifestFileEntry {
            path: format!("data/file{i:02}.pak"),
            kind: EntryKind::File,
            size: 2048,
            sha1: String::new(),
            chunks: 1,
            executable: false,
            link_target: None,
        })
        .collect();
    let mut browser = Browser::new(vec![]);
    browser.show_depots = true;
    browser.depots.files = Some(FilesView {
        depot_id: 11,
        depot_name: "Content".into(),
        branch: "public".into(),
        listing: Load::Ready(DepotFiles {
            app_id: 10,
            depot_id: 11,
            manifest_id: 99,
            files,
        }),
        filter: String::new(),
        selected: 40,
        input: Some(FilesInput::SaveTo("/home/me/Downloads".into())),
        status: String::new(),
    });
    let screen = render(&browser, 90, 16);
    assert!(screen.contains("manifest 99"), "{screen}");
    assert!(screen.contains("50 of 50"), "{screen}");
    assert!(screen.contains("▶"), "{screen}");
    assert!(screen.contains("data/file40.pak"), "{screen}");
    assert!(!screen.contains("data/file00.pak"), "{screen}");
    assert!(
        screen.contains("Save to directory: /home/me/Downloads"),
        "{screen}"
    );
}
