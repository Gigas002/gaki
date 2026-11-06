//! Keys on the sign-in screens.

use ratatui::crossterm::event::KeyCode;

use super::Flow;
use crate::app::library::Library;
use crate::app::{AppResult, Mode};

pub(super) fn key(lib: &mut Library, input: KeyCode) -> AppResult<Flow> {
    let Library {
        app,
        client,
        cached,
        ..
    } = lib;
    match app.mode {
        Mode::Login | Mode::Failed => match input {
            KeyCode::Char('q') => {
                return Ok(Flow::Quit);
            }
            // Skip the fresh sign-in a sensitive view asked for.
            #[cfg(feature = "inventory")]
            KeyCode::Esc if app.reauth.take().is_some() => {
                app.mode = Mode::Browse;
            }
            KeyCode::Char('r') => {
                // A session may have been established out-of-band via
                // `gaki-cli login`; re-probe it.
                *cached = false;
                app.mode = Mode::Loading;
                client.check_session()?;
            }
            KeyCode::Char('y') => {
                client.login_qr()?;
                app.mode = Mode::LoginQr;
            }
            KeyCode::Char('\n') | KeyCode::Enter => {
                app.user.clear();
                app.password.clear();
                app.mode = Mode::LoginUser;
            }
            _ => {}
        },
        Mode::LoginQr => match input {
            KeyCode::Char('q') => {
                return Ok(Flow::Quit);
            }
            KeyCode::Esc => {
                // Abandon this attempt; it stops and its outcome is
                // dropped.
                client.cancel_qr()?;
                app.mode = Mode::Login;
            }
            _ => {}
        },
        Mode::LoginUser => match input {
            KeyCode::Esc => {
                app.mode = Mode::Login;
            }
            KeyCode::Char('\n') | KeyCode::Enter => {
                let mut user = app.user.clone();
                user.retain(|c| !c.is_whitespace());
                if !user.is_empty() {
                    app.user = user;
                    app.password.clear();
                    app.mode = Mode::LoginPass;
                }
            }
            KeyCode::Backspace => {
                app.user.pop();
            }
            KeyCode::Char(c) => {
                app.user.push(c);
            }
            _ => {}
        },
        Mode::LoginPass => match input {
            KeyCode::Esc => {
                app.password.clear();
                app.mode = Mode::LoginUser;
            }
            KeyCode::Char('\n') | KeyCode::Enter => {
                if !app.password.is_empty() {
                    client.login_classic(&app.user, &app.password)?;
                    // Don't keep the password in memory any longer.
                    app.password.clear();
                    app.mode = Mode::LoginClassic;
                }
            }
            KeyCode::Backspace => {
                app.password.pop();
            }
            KeyCode::Char(c) => {
                app.password.push(c);
            }
            _ => {}
        },
        Mode::LoginClassic => match input {
            KeyCode::Char('q') => {
                return Ok(Flow::Quit);
            }
            KeyCode::Esc => {
                app.mode = Mode::Login;
            }
            _ => {}
        },
        Mode::LoginGuard => match input {
            KeyCode::Esc => {
                app.mode = Mode::Login;
            }
            KeyCode::Char('\n') | KeyCode::Enter => {
                let mut code = app.guard_code.clone();
                code.retain(|c| !c.is_whitespace());
                if !code.is_empty() {
                    client.submit_guard_code(&code)?;
                    app.guard_code.clear();
                    app.mode = Mode::LoginClassic;
                }
            }
            KeyCode::Backspace => {
                app.guard_code.pop();
            }
            KeyCode::Char(c) => {
                app.guard_code.push(c);
            }
            _ => {}
        },
        _ => {}
    }
    Ok(Flow::Continue)
}
