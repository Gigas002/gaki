use super::*;

#[test]
fn shows_the_steam_rules() {
    // Tall enough for every row, so the block at the end is drawn.
    let backend = ratatui::backend::TestBackend::new(100, row_count() + 2);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|f| f.render_widget(help(0, f.area().width), f.area()))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let screen: String = buffer.content().iter().map(|c| c.symbol()).collect();
    assert!(screen.contains("Steam rules"));
    assert!(screen.contains("docs/STEAM-RULES.md"));
}

fn screen(width: u16) -> Vec<String> {
    let backend = ratatui::backend::TestBackend::new(width, row_count() + 2);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|f| f.render_widget(help(0, width), f.area()))
        .unwrap();
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect()
        })
        .collect()
}

#[test]
fn keys_never_run_into_their_descriptions() {
    for width in [60, 90, 126] {
        let lines = screen(width);
        let line = lines
            .iter()
            .find(|l| l.contains("page up / down"))
            .unwrap_or_else(|| panic!("no paging row at {width}"));
        assert!(
            !line.contains("PageDownpage"),
            "{width}: key runs into its description: {line}"
        );
    }
}

#[test]
fn a_wide_overlay_shows_whole_descriptions_and_a_narrow_one_cuts_them() {
    let wide = screen(200);
    let cut: Vec<&String> = wide.iter().filter(|l| l.contains('\u{2026}')).collect();
    assert!(cut.is_empty(), "nothing cut at 200 columns: {cut:#?}");
    assert!(
        wide.iter()
            .any(|l| l.contains("game info (details, checks, artwork)"))
    );
    let narrow = screen(60).join("\n");
    assert!(narrow.contains('\u{2026}'), "a cut description ends in …");
}

#[test]
fn fit_pads_or_cuts_to_the_width() {
    assert_eq!(fit("ab", 4), "ab  ");
    assert_eq!(fit("abcdef", 4), "abc\u{2026}");
    assert_eq!(fit("abc", 0), "");
}
