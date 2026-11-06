//! The library window: filter tabs, the game list (or the Friends panel),
//! the detail pane over the game's art, the status bar, and any open overlay.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::app::browser::{Browser, View};
use crate::ui;
use crate::utils::image::ImgBuf;

/// Draw the library. `account` is the signed-in name for the status bar, and
/// `marquee_tick` scrolls the highlighted row's name when it doesn't fit.
pub fn draw(
    frame: &mut Frame,
    browser: &mut Browser,
    account: Option<&str>,
    background: Option<&ImgBuf>,
    cover: Option<&ImgBuf>,
    marquee_tick: usize,
) {
    // Library browser: tabs / [list | detail+art] / status bar.
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(2),
            Constraint::Length(2),
        ])
        .split(frame.area());

    // Top bar split into two sections: the library filters on the
    // left and the Friends & Chat tab in its own box on the right,
    // each with its own headline. Without `social` the filters take it all.
    #[cfg(feature = "social")]
    {
        let tab_areas = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(20), Constraint::Length(19)])
            .split(chunks[0]);
        frame.render_widget(ui::tabs::library_tabs(browser), tab_areas[0]);
        frame.render_widget(ui::tabs::friends_tabs(browser), tab_areas[1]);
    }
    #[cfg(not(feature = "social"))]
    frame.render_widget(ui::tabs::library_tabs(browser), chunks[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(chunks[1]);

    // Left pane: the game list under a library filter, or — when the
    // Friends tab is selected — the Friends & Chat panel in its
    // place (the "All → Friends" transition). The panel sizes its
    // whole-list scroll to the full-height left column.
    match browser.view {
        #[cfg(feature = "social")]
        View::Friends => {
            let friends_rows = body[0].height.saturating_sub(2) as usize;
            frame.render_widget(ui::friends::friends(browser, friends_rows), body[0]);
        }
        View::Library => {
            let list_widget = ui::list::list(browser, body[0].width, marquee_tick);
            frame.render_stateful_widget(list_widget, body[0], &mut browser.state);
        }
    }

    let selected = browser.selected();
    // Game Details fill the whole right column; the Friends & Chat
    // panel is reached via its own tab, so there is no bottom-right
    // sidebar anymore.
    let detail_area = body[1];

    // Detail panel: the cover art is painted as its background and
    // the details — including the description as its own row — are
    // listed on top.
    let inner = Rect {
        x: detail_area.x + 1,
        y: detail_area.y + 1,
        width: detail_area.width.saturating_sub(2),
        height: detail_area.height.saturating_sub(2),
    };

    let desc_width = inner.width.saturating_mul(82) / 100;

    // The art renders whenever it is loaded. While scrolling it is cleared (the
    // app drops it on a selection change), so nothing paints or resizes
    // mid-scroll. The faded background covers the whole pane, and the portrait
    // cover fills the region above `ID | Name`.
    let content_h = ui::detail::content_height(
        selected.as_ref(),
        browser.expand_description,
        desc_width.saturating_sub(1),
    )
    .min(inner.height);
    ui::artwork::paint(frame, inner, background, cover, content_h);

    // The (transparent) detail table draws its frame and bottom-
    // aligned rows over the art.
    frame.render_widget(
        ui::detail::detail(
            selected.as_ref(),
            browser.expand_description,
            desc_width.saturating_sub(1),
            inner.height,
        ),
        detail_area,
    );

    frame.render_widget(ui::status::status_bar(browser, account), chunks[2]);
    // Bottom-right corner, right of the key hints.
    frame.render_widget(
        ui::status::version_badge(),
        ui::status::version_badge_area(chunks[2]),
    );

    ui::overlays::draw(frame, browser);
}
