//! An overlay drawn over a wide character (CJK, emoji) that its left edge cuts
//! in half keeps its border: the cut glyph's first half is blanked.

use gaki_tui::ui::overlays::Cover;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Widget};

#[test]
fn a_cut_wide_character_is_blanked() {
    let mut buf = Buffer::empty(Rect::new(0, 0, 10, 3));
    // "ウマ" takes columns 2-5; the overlay starts at column 3, inside "ウ".
    buf.set_string(2, 1, "ウマ", ratatui::style::Style::default());
    let area = Rect::new(3, 0, 5, 3);
    Cover.render(area, &mut buf);
    Block::default()
        .borders(Borders::ALL)
        .render(area, &mut buf);
    assert_eq!(buf[(2, 1)].symbol(), " ", "the cut glyph is gone");
    assert_eq!(buf[(3, 1)].symbol(), "│", "the border is whole");
}

#[test]
fn an_uncut_character_stays() {
    let mut buf = Buffer::empty(Rect::new(0, 0, 10, 3));
    buf.set_string(1, 1, "ウ", ratatui::style::Style::default());
    Cover.render(Rect::new(3, 0, 5, 3), &mut buf);
    assert_eq!(buf[(1, 1)].symbol(), "ウ");
}
