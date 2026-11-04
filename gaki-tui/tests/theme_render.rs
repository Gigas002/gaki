//! Renders themed widgets into tui's off-screen `TestBackend` and asserts the
//! Steam palette is actually painted into the cell buffer (not just that the
//! code compiles).

use gaki_tui::app::App;
use gaki_tui::theme;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::widgets::Paragraph;

#[test]
fn panel_paints_steam_palette() {
    let backend = TestBackend::new(24, 6);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| {
            let p = Paragraph::new("hello")
                .block(theme::panel("Title".to_string()))
                .style(theme::base());
            f.render_widget(p, f.area());
        })
        .unwrap();

    let buffer = terminal.backend().buffer();

    // An interior cell is the navy panel background.
    assert_eq!(
        buffer.cell((3, 2)).unwrap().bg,
        theme::BG,
        "panel interior should be navy"
    );

    // The title text (row 0) is painted in the accent colour somewhere.
    let title_accent = (1..23).any(|x| buffer.cell((x, 0)).unwrap().fg == theme::ACCENT);
    assert!(title_accent, "panel title should use the accent colour");
}

#[test]
fn infobox_is_themed() {
    let backend = TestBackend::new(30, 6);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| {
            f.render_widget(App::build_loading(), f.area());
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    assert_eq!(
        buffer.cell((2, 2)).unwrap().bg,
        theme::BG,
        "info boxes should sit on the navy panel background"
    );
}
