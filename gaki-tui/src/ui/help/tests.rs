use super::*;

#[test]
fn shows_the_steam_rules() {
    // Tall enough for every row, so the block at the end is drawn.
    let backend = ratatui::backend::TestBackend::new(100, row_count() + 2);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|f| f.render_widget(help(0), f.area()))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let screen: String = buffer.content().iter().map(|c| c.symbol()).collect();
    assert!(screen.contains("Steam rules"));
    assert!(screen.contains("docs/STEAM-RULES.md"));
}
