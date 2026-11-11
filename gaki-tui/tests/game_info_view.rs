//! Renders the Game info overlay (`n`) into an off-screen `TestBackend`.

use gaki_tui::app::browser::game_info::{GameInfoView, InfoTab};
use gaki_tui::app::browser::job::Load;
use gaki_tui::app::steam::{CheckLine, DrmResult, GameInfo, SavedImage};
use gaki_tui::ui;
use libgaki::steam_client::{StoreAppInfo, StoreLink, StoreTag};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn render(view: &GameInfoView, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| ui::game_info::draw(frame, frame.area(), view))
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

fn info() -> GameInfo {
    let mut details = StoreAppInfo {
        app_id: 620,
        name: "Portal 2".into(),
        app_type: "game".into(),
        developers: vec!["Valve".into()],
        platforms: vec!["windows".into(), "linux".into()],
        price: Some("$9.99".into()),
        country: "US".into(),
        tags: vec![StoreTag {
            id: 1,
            name: Some("Puzzle".into()),
            weight: 1,
        }],
        links: vec![StoreLink {
            kind: "website".into(),
            url: "https://thinkwithportals.com".into(),
        }],
        ..Default::default()
    };
    details.assets.header = Some("https://cdn/620/header.jpg".into());
    GameInfo {
        details,
        extended: None,
        dlc_count: 2,
    }
}

#[test]
fn details_show_the_store_record() {
    let view = GameInfoView::with_details(620, "Portal 2", info());
    let screen = render(&view, 100, 30);
    assert!(screen.contains("Game info · Portal 2"), "{screen}");
    assert!(screen.contains("Valve"), "{screen}");
    assert!(screen.contains("windows, linux"), "{screen}");
    assert!(screen.contains("$9.99 [US]"), "{screen}");
    assert!(screen.contains("Puzzle"), "{screen}");
    assert!(screen.contains("thinkwithportals.com"), "{screen}");
    assert!(screen.contains("DLC"), "{screen}");
}

#[test]
fn checks_show_both_tickets() {
    let mut view = GameInfoView::with_details(620, "Portal 2", info());
    view.tab = InfoTab::Checks;
    view.drm = Some(Load::Ready(DrmResult {
        ownership: CheckLine {
            ok: true,
            text: "issued (170 bytes): the account holds a license".into(),
        },
        encrypted: CheckLine {
            ok: false,
            text: "refused (eresult 2): the app likely has no encrypted-ticket key".into(),
        },
    }));
    let screen = render(&view, 110, 20);
    assert!(screen.contains("✓ Ownership ticket"), "{screen}");
    assert!(screen.contains("✗ Encrypted app ticket"), "{screen}");
    assert!(screen.contains("[r] check again"), "{screen}");
}

#[test]
fn artwork_lists_urls_folder_and_results() {
    let mut view = GameInfoView::with_details(620, "Portal 2", info());
    view.tab = InfoTab::Artwork;
    view.save_dir = "/home/me/Pictures".into();
    view.saving = Some(Load::Ready(vec![
        SavedImage {
            kind: "header",
            result: Ok("/home/me/Pictures/620_header.jpg".into()),
        },
        SavedImage {
            kind: "logo",
            result: Err("/home/me/Pictures/620_logo.png already exists".into()),
        },
    ]));
    let screen = render(&view, 110, 24);
    assert!(
        screen.contains("Save to           /home/me/Pictures"),
        "{screen}"
    );
    assert!(screen.contains("https://cdn/620/header.jpg"), "{screen}");
    assert!(screen.contains("✓ header"), "{screen}");
    assert!(screen.contains("already exists"), "{screen}");

    view.dir_input = Some("~/art".into());
    let screen = render(&view, 110, 24);
    assert!(screen.contains("Save to: ~/art_"), "{screen}");
}

#[test]
fn a_long_field_wraps_under_its_value() {
    let mut game = info();
    game.details.tags = (0..12)
        .map(|i| StoreTag {
            id: i,
            name: Some(format!("Tag{i:02}")),
            weight: 1,
        })
        .collect();
    let view = GameInfoView::with_details(620, "Portal 2", game);
    let screen = render(&view, 60, 30);
    let lines: Vec<&str> = screen.lines().collect();
    let first = lines
        .iter()
        .position(|l| l.contains("Tags"))
        .expect("a Tags row");
    // The value starts after the 18-column label (plus the border).
    let column = lines[first].find("Tag00").unwrap();
    let next = lines[first + 1];
    let cont = next
        .find("Tag")
        .expect("the tags continue on the next line");
    assert_eq!(cont, column, "continuation under the value:\n{screen}");
    assert!(
        !next[..cont]
            .trim_start_matches('│')
            .trim()
            .contains(char::is_alphanumeric),
        "nothing under the label:\n{screen}"
    );
}
