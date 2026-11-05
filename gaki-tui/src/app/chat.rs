//! The stand-alone chat window: `gaki --chat <steamid> --name <name>`. The
//! Friends panel's "open in new terminal" action (`t`) starts it in a new
//! console window. It reuses the chat widget and the `Browser` chat state.

use ratatui::crossterm::event::KeyCode;

use crate::app::AppResult;
use crate::app::terminal::{self, TerminalGuard};
use crate::browse::Browser;
use crate::ui;
use crate::utils::event::{Event, Events};

/// Run a full-screen chat with one friend until Esc.
pub fn run(steamid: u64, name: String) -> AppResult {
    let (_guard, mut terminal) = TerminalGuard::enter(false)?;
    terminal::clear(&mut terminal)?;

    let mut browser = Browser::new(Vec::new());
    browser.chat_steamid = steamid;
    browser.chat_partner = name;
    browser.refresh_chat();

    let events = Events::new();
    // Re-fetch history every ~3s (12 ticks at the 250ms tick rate) so incoming
    // messages show up without a fetch on every frame.
    let mut ticks: u32 = 0;
    loop {
        terminal.draw(|frame| ui::chat::window(frame, &browser))?;

        match events.next()? {
            Event::Input(input) => match input {
                KeyCode::Esc => break,
                KeyCode::Char('\n') | KeyCode::Enter => browser.chat_send(),
                KeyCode::Backspace => browser.chat_pop(),
                KeyCode::Char(c) => browser.chat_push(c),
                _ => {}
            },
            Event::Tick => {
                ticks = ticks.wrapping_add(1);
                if ticks.is_multiple_of(12) {
                    browser.refresh_chat();
                }
            }
        }
    }

    terminal::clear(&mut terminal)?;
    Ok(())
}
