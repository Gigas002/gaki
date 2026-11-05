//! Terminal setup for the run loops, and its restore.

use std::io::{self, Stdout};

use ratatui::Terminal;
use ratatui::backend::{Backend, ClearType, CrosstermBackend};
use ratatui::crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{disable_raw_mode, enable_raw_mode};

pub type Tui = Terminal<CrosstermBackend<Stdout>>;

/// Holds the terminal in raw mode, with the mouse captured if asked, for as
/// long as it lives. Dropping it restores the terminal, so an error leaving a
/// run loop prints to a normal console.
pub struct TerminalGuard {
    mouse: bool,
}

impl TerminalGuard {
    /// Enter raw mode and return the guard with a terminal to draw on.
    ///
    /// Capturing the mouse lets the wheel scroll the lists. Text selection
    /// still works: terminals keep Shift+drag for their own selection before
    /// the app sees those events.
    pub fn enter(mouse: bool) -> io::Result<(Self, Tui)> {
        enable_raw_mode()?;
        // From here on, dropping the guard undoes the raw mode.
        let guard = Self { mouse };
        let mut stdout = io::stdout();
        if mouse {
            execute!(stdout, EnableMouseCapture)?;
        }
        let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
        Ok((guard, terminal))
    }
}

/// Clear the screen, and repaint every cell on the next draw.
///
/// Use this, not `Terminal::clear`: that one asks the terminal for the cursor
/// position, but the input thread is blocked reading stdin and takes the
/// reply, so the query times out after two seconds and fails the loop (that
/// is what made every quit exit with an error).
pub fn clear(terminal: &mut Tui) -> io::Result<()> {
    terminal.backend_mut().clear_region(ClearType::All)?;
    // Leaves both buffers empty, so the next frame is diffed against nothing.
    terminal.swap_buffers();
    Ok(())
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        if self.mouse {
            let _ = execute!(io::stdout(), DisableMouseCapture);
        }
    }
}
