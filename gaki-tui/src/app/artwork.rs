//! The art for the highlighted game, loaded off the UI thread. Without the
//! `art` feature there is none.

use crate::app::game::Game;
use crate::ui;
#[cfg(feature = "art")]
use crate::utils::image::{self as artwork, ImageSlot, ImgBuf};

/// The background and cover art of the highlighted game. Background threads
/// publish downloads into the slots; the requested ids make stale loads drop
/// and an unchanged selection a no-op.
#[derive(Default)]
#[cfg(feature = "art")]
pub struct Artwork {
    background: Option<ImgBuf>,
    cover: Option<ImgBuf>,
    background_slot: ImageSlot,
    cover_slot: ImageSlot,
    requested_background: Option<i32>,
    requested_cover: Option<i32>,
    /// The game the art on screen belongs to.
    shown_for: Option<i32>,
}

/// Without the `art` feature: nothing to load or draw.
#[derive(Default)]
#[cfg(not(feature = "art"))]
pub struct Artwork {}

#[cfg(not(feature = "art"))]
impl Artwork {
    pub fn art(&self) -> ui::Art<'_> {
        ui::Art::default()
    }

    pub fn update(&mut self, _selected: Option<&Game>, _settled: bool) {}
}

#[cfg(feature = "art")]
impl Artwork {
    /// The art to draw this frame.
    pub fn art(&self) -> ui::Art<'_> {
        ui::Art {
            background: self.background.as_ref(),
            cover: self.cover.as_ref(),
        }
    }

    /// Follow the highlighted game. A new selection drops the old art at
    /// once; loading the new art waits until `settled` (input has been idle a
    /// moment), so scrolling past a game never downloads its art.
    pub fn update(&mut self, selected: Option<&Game>, settled: bool) {
        let cur_sel = selected.map(|g| g.id);
        if cur_sel != self.shown_for {
            // Selection changed: drop the stale art and forget the requested
            // id so the new art reloads once input settles — nothing shows
            // (or loads) for games merely scrolled past.
            self.shown_for = cur_sel;
            self.background = None;
            self.cover = None;
            self.requested_background = None;
            self.requested_cover = None;
        }
        // Always adopt any in-flight (off-thread) loads.
        artwork::poll(
            self.requested_background,
            &mut self.background,
            &self.background_slot,
        );
        artwork::poll(self.requested_cover, &mut self.cover, &self.cover_slot);
        if settled {
            artwork::select(
                selected,
                artwork::ImageKind::Background,
                &mut self.requested_background,
                &mut self.background,
                &self.background_slot,
            );
            artwork::select(
                selected,
                artwork::ImageKind::Cover,
                &mut self.requested_cover,
                &mut self.cover,
                &self.cover_slot,
            );
        }
    }
}
