//! The scroll position of a list name too long for its row.

use std::time::{Duration, Instant};

/// Milliseconds per one-column step (about four columns a second).
const STEP_MS: u128 = 250;

/// Scrolls the highlighted row's name. The step comes from wall-clock time
/// rather than a frame count, so the pace stays steady whatever triggered
/// each redraw (a tick or a keypress). It restarts from the first character
/// each time the highlight moves, so a name never opens mid-scroll.
pub struct Marquee {
    start: Instant,
    row: Option<usize>,
    base: usize,
}

impl Default for Marquee {
    fn default() -> Self {
        Self {
            start: Instant::now(),
            row: None,
            base: 0,
        }
    }
}

impl Marquee {
    /// The scroll offset for this frame, with `row` highlighted.
    pub fn tick(&mut self, row: Option<usize>) -> usize {
        self.tick_at(row, self.start.elapsed())
    }

    fn tick_at(&mut self, row: Option<usize>, elapsed: Duration) -> usize {
        let global = (elapsed.as_millis() / STEP_MS) as usize;
        if row != self.row {
            self.row = row;
            self.base = global;
        }
        global.saturating_sub(self.base)
    }
}

#[cfg(test)]
mod tests;
