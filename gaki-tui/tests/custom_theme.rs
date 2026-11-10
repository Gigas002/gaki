//! A theme file reaches what's drawn: install one (this test binary is its own
//! process, so the global theme doesn't leak into other tests) and check the
//! cells.

use gaki_tui::ui::theme::{self, Theme};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;
use ratatui::widgets::Paragraph;

#[test]
fn an_installed_theme_paints_the_panels() {
    let theme = Theme::parse(
        "[palette]\nbg = \"ff000010\"\naccent = \"#ff0000\"\n\
         [styles]\nborder = { fg = \"ff00ff00\" }\n\
         [widgets]\nborder_type = \"double\"\n",
    )
    .unwrap();
    theme::install(theme, None);

    let mut terminal = Terminal::new(TestBackend::new(20, 5)).unwrap();
    terminal
        .draw(|f| {
            let p = Paragraph::new("hi").block(theme::panel("T".to_string()));
            f.render_widget(p, f.area());
        })
        .unwrap();
    let buffer = terminal.backend().buffer();

    let corner = buffer.cell((0, 0)).unwrap();
    assert_eq!(corner.symbol(), "╔", "double border");
    assert_eq!(corner.fg, Color::Rgb(0, 255, 0), "the border style");
    assert_eq!(
        corner.bg,
        Color::Rgb(0, 0, 0x10),
        "border bg follows the palette"
    );
    assert_eq!(buffer.cell((3, 2)).unwrap().bg, Color::Rgb(0, 0, 0x10));
    let title = (1..19).any(|x| buffer.cell((x, 0)).unwrap().fg == Color::Rgb(255, 0, 0));
    assert!(title, "the title takes the palette's accent");
}
