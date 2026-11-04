//! Application state and the `libgaki` worker ([`worker`]).
//!
//! The splash and sign-in page builders below move to `ui/` with the rest of
//! the layout work in Phase 4.5.

pub mod worker;

use crate::config::Config;
use crate::theme;

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

const SPLASH: &str = include_str!("../../assets/splash.txt");

pub struct App {
    pub mode: Mode,
    pub user: String,
    /// In-progress password entry (classic login). Never persisted.
    pub password: String,
    /// In-progress Steam Guard code entry.
    pub guard_code: String,
    pub highlight: Color,
}

#[derive(PartialEq, Clone)]
pub enum Mode {
    Login,
    LoginQr,
    /// Typing the Steam username (classic login).
    LoginUser,
    /// Typing the Steam password (classic login).
    LoginPass,
    /// Classic login in progress (connecting / awaiting confirmation).
    LoginClassic,
    /// Typing a Steam Guard code prompted during classic login.
    LoginGuard,
    Loading,
    /// The main library browser (filter tabs, list, detail, status bar).
    Browse,
    Failed,
    Terminated(String),
}

impl App {
    pub fn new(config: &Config) -> App {
        let user = config.default_user.clone();
        let highlight = config.highlight;
        App {
            mode: if user.is_empty() {
                Mode::Login
            } else {
                Mode::Loading
            },
            user,
            password: String::new(),
            guard_code: String::new(),
            highlight,
        }
    }

    pub fn build_layout() -> Layout {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(2), Constraint::Length(3)])
    }
    pub fn build_splash_terminated(err: String) -> Paragraph<'static> {
        App::build_infobox(
            "Oh dear...".to_string(),
            format!(
                "Something has crashed.. For more details please refer to the error below:\n\n{}",
                err
            ),
            Alignment::Left,
        )
    }

    pub fn build_splash_err() -> Paragraph<'static> {
        App::build_infobox(
            "gaki".to_string(),
            format!(
                "{}\n Uhoh. Could not find a session. Have you run `gaki-cli login`?",
                SPLASH
            ),
            Alignment::Center,
        )
    }

    pub fn build_splash() -> Paragraph<'static> {
        App::build_infobox("gaki".to_string(), SPLASH.to_string(), Alignment::Center)
    }

    pub fn build_patience() -> Paragraph<'static> {
        App::build_infobox(
            "Welcome".to_string(),
            "Checking cache (on load, you can press 'r' to invalidate cache)".to_string(),
            Alignment::Left,
        )
    }

    fn build_infobox(title: String, content: String, alignment: Alignment) -> Paragraph<'static> {
        Paragraph::new(content)
            .style(theme::base())
            .alignment(alignment)
            .block(theme::panel(title))
    }
    pub fn build_loaded(count: i32, of: i32) -> Paragraph<'static> {
        let p = {
            if of < 0 {
                "Calculating...".to_string()
            } else {
                let p = 100. * (count as f32) / (of as f32);
                format!("Loading %{:.1}", p)
            }
        };
        App::build_infobox("Please wait".to_string(), p, Alignment::Left)
    }
    pub fn build_loading() -> Paragraph<'static> {
        App::build_infobox(
            "Please wait".to_string(),
            "Logging in and updating...".to_string(),
            Alignment::Left,
        )
    }
    pub fn build_login(username: String) -> Paragraph<'static> {
        App::build_infobox(
            "Login (Enter to submit)".to_string(),
            username,
            Alignment::Left,
        )
    }

    /// Landing page shown when no Steam session is available. Offers classic
    /// (username/password) login and a QR login.
    pub fn build_login_landing(error: Option<String>) -> Paragraph<'static> {
        // Instructions come first so they stay visible even when the (tall)
        // splash overflows the panel.
        let problem = match error {
            Some(err) => format!("\n {}\n", err),
            None => String::new(),
        };
        App::build_infobox(
            "Sign in".to_string(),
            format!(
                " No Steam session found.{}\n [Enter]  sign in with username & password\n [y]      sign in by QR code (Steam Mobile app)\n [r]      re-check session (after `gaki-cli login`)\n [q]      quit\n\n{}",
                problem, SPLASH
            ),
            Alignment::Center,
        )
    }

    /// Classic-login username entry.
    pub fn build_login_user(user: String) -> Paragraph<'static> {
        App::build_infobox(
            "Steam username (Enter to continue, Esc to cancel)".to_string(),
            user,
            Alignment::Left,
        )
    }

    /// Classic-login password entry (masked).
    pub fn build_login_pass(len: usize) -> Paragraph<'static> {
        App::build_infobox(
            "Steam password (Enter to sign in, Esc to go back)".to_string(),
            "*".repeat(len),
            Alignment::Left,
        )
    }

    /// Status page while a classic login is connecting / awaiting confirmation.
    pub fn build_login_classic(status: String) -> Paragraph<'static> {
        App::build_infobox(
            "Signing in ([Esc] cancel, [q] quit)".to_string(),
            format!("{}\n\n{}", SPLASH, status),
            Alignment::Center,
        )
    }

    /// Steam Guard code entry, prompted mid-login.
    pub fn build_login_guard(kind: String, code: String) -> Paragraph<'static> {
        let label = match kind.as_str() {
            "email" => "Enter the Steam Guard code emailed to you",
            "device" => "Enter the Steam Guard code from your authenticator",
            _ => "Enter your Steam Guard code",
        };
        App::build_infobox(
            format!("{} (Enter to submit, Esc to cancel)", label),
            code,
            Alignment::Left,
        )
    }

    pub fn build_login_classic_help() -> Paragraph<'static> {
        App::build_infobox(
            "Signing in".to_string(),
            "Please wait — approve on your Steam Mobile app if prompted | [Esc] cancel | [q] quit"
                .to_string(),
            Alignment::Left,
        )
    }

    /// The QR-login page for a pane of size `area`: the current challenge as
    /// scannable blocks (black modules on white, quiet zone included),
    /// centred. When the pane is too small to hold the whole code, it says how
    /// large the terminal must be instead of drawing a clipped, unscannable
    /// code.
    pub fn build_qr(qr: Option<String>, area: Rect) -> Paragraph<'static> {
        let panel = theme::panel("Scan with the Steam Mobile app ([Esc] cancel, [q] quit)".into());
        let Some(url) = qr else {
            return Paragraph::new("Generating QR code...")
                .alignment(Alignment::Center)
                .block(panel);
        };
        let Some(code) = App::render_qr_string(&url) else {
            return Paragraph::new(format!("(could not render the QR code)\n{url}"))
                .alignment(Alignment::Center)
                .block(panel);
        };
        let lines: Vec<&str> = code.lines().collect();
        let width = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as u16;
        let height = lines.len() as u16;
        // The panel border takes one cell on every side.
        let (inner_w, inner_h) = (area.width.saturating_sub(2), area.height.saturating_sub(2));
        if width > inner_w || height > inner_h {
            // The help box below the pane takes 3 rows.
            return Paragraph::new(format!(
                "The QR code needs a terminal of at least {}×{}.\nEnlarge the window, or press [Esc] and sign in with a password.",
                width + 2,
                height + 2 + 3
            ))
            .alignment(Alignment::Center)
            .block(panel);
        }
        let qr_style = Style::default().fg(Color::Black).bg(Color::White);
        let top = (inner_h - height) / 2;
        let text: Vec<Line<'static>> = std::iter::repeat_n(Line::raw(""), top as usize)
            .chain(
                lines
                    .into_iter()
                    .map(|l| Line::from(Span::styled(l.to_string(), qr_style))),
            )
            .collect();
        Paragraph::new(text)
            .alignment(Alignment::Center)
            .block(panel)
    }

    /// Render a challenge URL as a terminal QR code, two modules per cell
    /// vertically, with the standard quiet zone. Dark modules are block glyphs,
    /// so it must be drawn dark-on-light.
    fn render_qr_string(url: &str) -> Option<String> {
        let code = qrcode::QrCode::new(url.as_bytes()).ok()?;
        Some(
            code.render::<qrcode::render::unicode::Dense1x2>()
                .quiet_zone(true)
                .build(),
        )
    }
    pub fn build_login_help() -> Paragraph<'static> {
        App::build_infobox(
            "Sign in".to_string(),
            "[Enter] username/password  |  [y] QR code  |  [r] re-check  |  [q] quit".to_string(),
            Alignment::Left,
        )
    }

    pub fn build_qr_help() -> Paragraph<'static> {
        App::build_infobox(
            "Waiting for scan".to_string(),
            "Scan the code with the Steam Mobile app | [Esc] cancel | [q] quit".to_string(),
            Alignment::Left,
        )
    }

    pub fn build_terminated_help() -> Paragraph<'static> {
        App::build_infobox(
            "Woops.".to_string(),
            "Press q to quit.".to_string(),
            Alignment::Left,
        )
    }
}
