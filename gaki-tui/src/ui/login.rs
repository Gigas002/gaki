//! The screens before the library: splash, sign-in (password, Steam Guard,
//! QR), loading, and the crash page. Each is a top pane plus a help box.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::client::{Client, State};
use crate::app::steam::LoginPhase;
use crate::app::{App, Mode};
use crate::ui::theme;

const SPLASH: &str = include_str!("../../assets/splash.txt");

/// Draw the screen for `app.mode`. The library (`Mode::Browse`) is drawn by
/// [`super::library`], so it gets a placeholder here.
pub fn draw(frame: &mut Frame, app: &App, client: &Client) {
    // Login / loading / terminated screens use the simple two-pane layout.
    let layout = layout();
    let placement = layout.split(frame.area());
    let help = match app.mode {
        Mode::Terminated(_) => terminated_help(),
        Mode::Login | Mode::Failed => login_help(),
        Mode::LoginQr => qr_help(),
        Mode::LoginUser => login_user(app.user.clone()),
        Mode::LoginPass => login_pass(app.password.len()),
        Mode::LoginClassic => login_classic_help(),
        Mode::LoginGuard => login_guard(
            match client.get_login_phase() {
                LoginPhase::GuardCode(kind) => kind,
                _ => String::new(),
            },
            app.guard_code.clone(),
        ),
        Mode::Loading => match client.get_state() {
            Ok(State::Loaded(count, of)) => loaded(count, of),
            _ => loading(),
        },
        Mode::Browse => loading(),
    };
    match &app.mode {
        Mode::Failed => frame.render_widget(no_session(), placement[0]),
        Mode::Login => frame.render_widget(login_landing(client.get_last_error()), placement[0]),
        Mode::LoginQr => frame.render_widget(qr(client.get_qr(), placement[0]), placement[0]),
        Mode::LoginUser | Mode::LoginPass => frame.render_widget(splash(), placement[0]),
        Mode::LoginClassic => {
            let status = match client.get_login_phase() {
                LoginPhase::AwaitingConfirmation => {
                    "Signing in — if prompted, approve this login in your Steam Mobile app..."
                        .to_string()
                }
                LoginPhase::DeviceConfirmation => {
                    "Approve this login in your Steam Mobile app, then wait...".to_string()
                }
                LoginPhase::Failed(msg) => format!("Login failed: {}", msg),
                _ => "Signing in...".to_string(),
            };
            frame.render_widget(login_classic(status), placement[0])
        }
        Mode::LoginGuard => {
            let kind = match client.get_login_phase() {
                LoginPhase::GuardCode(kind) => kind,
                _ => String::new(),
            };
            frame.render_widget(login_guard(kind, app.guard_code.clone()), placement[0])
        }
        Mode::Terminated(err) => frame.render_widget(terminated(err.clone()), placement[0]),
        Mode::Loading => frame.render_widget(splash(), placement[0]),
        Mode::Browse => {}
    }
    frame.render_widget(help, placement[1]);
}

/// The first frame, drawn while the session is probed.
pub fn startup(frame: &mut Frame) {
    let placement = layout().split(frame.area());
    frame.render_widget(splash(), placement[0]);
    frame.render_widget(patience(), placement[1]);
}

pub fn layout() -> Layout {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(2), Constraint::Length(3)])
}
pub fn terminated(err: String) -> Paragraph<'static> {
    infobox(
        "Oh dear...".to_string(),
        format!(
            "Something has crashed.. For more details please refer to the error below:\n\n{}",
            err
        ),
        Alignment::Left,
    )
}

pub fn no_session() -> Paragraph<'static> {
    infobox(
        "gaki".to_string(),
        format!(
            "{}\n Uhoh. Could not find a session. Have you run `gaki-cli login`?",
            SPLASH
        ),
        Alignment::Center,
    )
}

pub fn splash() -> Paragraph<'static> {
    infobox("gaki".to_string(), SPLASH.to_string(), Alignment::Center)
}

pub fn patience() -> Paragraph<'static> {
    infobox(
        "Welcome".to_string(),
        "Checking cache (on load, you can press 'r' to invalidate cache)".to_string(),
        Alignment::Left,
    )
}

fn infobox(title: String, content: String, alignment: Alignment) -> Paragraph<'static> {
    Paragraph::new(content)
        .style(theme::base())
        .alignment(alignment)
        .block(theme::panel(title))
}
pub fn loaded(count: i32, of: i32) -> Paragraph<'static> {
    let p = {
        if of < 0 {
            "Calculating...".to_string()
        } else {
            let p = 100. * (count as f32) / (of as f32);
            format!("Loading %{:.1}", p)
        }
    };
    infobox("Please wait".to_string(), p, Alignment::Left)
}
pub fn loading() -> Paragraph<'static> {
    infobox(
        "Please wait".to_string(),
        "Logging in and updating...".to_string(),
        Alignment::Left,
    )
}
/// Landing page shown when no Steam session is available. Offers classic
/// (username/password) login and a QR login.
pub fn login_landing(error: Option<String>) -> Paragraph<'static> {
    // Instructions come first so they stay visible even when the (tall)
    // splash overflows the panel.
    let problem = match error {
        Some(err) => format!("\n {}\n", err),
        None => String::new(),
    };
    infobox(
        "Sign in".to_string(),
        format!(
            " No Steam session found.{}\n [Enter]  sign in with username & password\n [y]      sign in by QR code (Steam Mobile app)\n [r]      re-check session (after `gaki-cli login`)\n [q]      quit\n\n{}",
            problem, SPLASH
        ),
        Alignment::Center,
    )
}

/// Classic-login username entry.
pub fn login_user(user: String) -> Paragraph<'static> {
    infobox(
        "Steam username (Enter to continue, Esc to cancel)".to_string(),
        user,
        Alignment::Left,
    )
}

/// Classic-login password entry (masked).
pub fn login_pass(len: usize) -> Paragraph<'static> {
    infobox(
        "Steam password (Enter to sign in, Esc to go back)".to_string(),
        "*".repeat(len),
        Alignment::Left,
    )
}

/// Status page while a classic login is connecting / awaiting confirmation.
pub fn login_classic(status: String) -> Paragraph<'static> {
    infobox(
        "Signing in ([Esc] cancel, [q] quit)".to_string(),
        format!("{}\n\n{}", SPLASH, status),
        Alignment::Center,
    )
}

/// Steam Guard code entry, prompted mid-login.
pub fn login_guard(kind: String, code: String) -> Paragraph<'static> {
    let label = match kind.as_str() {
        "email" => "Enter the Steam Guard code emailed to you",
        "device" => "Enter the Steam Guard code from your authenticator",
        _ => "Enter your Steam Guard code",
    };
    infobox(
        format!("{} (Enter to submit, Esc to cancel)", label),
        code,
        Alignment::Left,
    )
}

pub fn login_classic_help() -> Paragraph<'static> {
    infobox(
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
pub fn qr(qr: Option<String>, area: Rect) -> Paragraph<'static> {
    let panel = theme::panel("Scan with the Steam Mobile app ([Esc] cancel, [q] quit)".into());
    let Some(url) = qr else {
        return Paragraph::new("Generating QR code...")
            .alignment(Alignment::Center)
            .block(panel);
    };
    let Some(code) = render_qr_string(&url) else {
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
pub fn login_help() -> Paragraph<'static> {
    infobox(
        "Sign in".to_string(),
        "[Enter] username/password  |  [y] QR code  |  [r] re-check  |  [q] quit".to_string(),
        Alignment::Left,
    )
}

pub fn qr_help() -> Paragraph<'static> {
    infobox(
        "Waiting for scan".to_string(),
        "Scan the code with the Steam Mobile app | [Esc] cancel | [q] quit".to_string(),
        Alignment::Left,
    )
}

pub fn terminated_help() -> Paragraph<'static> {
    infobox(
        "Woops.".to_string(),
        "Press q to quit.".to_string(),
        Alignment::Left,
    )
}
