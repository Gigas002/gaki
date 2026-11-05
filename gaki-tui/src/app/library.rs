//! The library window's run loop.

use std::time::{Duration, Instant};

use crate::app::artwork::Artwork;
use crate::app::keys::Flow;
use crate::app::marquee::Marquee;
use crate::app::terminal::{self, TerminalGuard};
use crate::app::{App, AppResult, Mode};
use crate::browse::Browser;
use crate::client::{Client, State};
use crate::config::Config;
use crate::interface::aurelia::LoginPhase;
use crate::ui;
use crate::utils::event::{Event, Events};

/// How long input must be idle before the art and lazy details load, so
/// scrolling stays smooth: no image decode or fetch per game scrolled past.
const ARTWORK_IDLE: Duration = Duration::from_millis(200);

/// Everything the library window's loop works on.
pub(super) struct Library {
    pub(super) app: App,
    pub(super) config: Config,
    pub(super) browser: Browser,
    pub(super) client: Client,
    /// The browser was seeded from the cached library, so the first session
    /// check can skip a reload.
    pub(super) cached: bool,
    /// Clear the terminal before the next frame (after a screen change).
    pub(super) clear_screen: bool,
    artwork: Artwork,
    marquee: Marquee,
    last_input_at: Instant,
}

/// Run the library window until the user quits.
pub fn run() -> AppResult {
    let (_guard, mut terminal) = TerminalGuard::enter(true)?;
    terminal::clear(&mut terminal)?;
    terminal.draw(ui::login::startup)?;

    let mut lib = Library::start()?;
    let events = Events::new();
    loop {
        lib.poll_workers();
        let marquee_tick = lib.marquee.tick(lib.browser.selected_index());
        terminal.draw(|frame| {
            ui::draw(
                frame,
                &lib.app,
                &mut lib.browser,
                &lib.client,
                lib.artwork.art(),
                marquee_tick,
            )
        })?;

        if let Event::Input(input) = events.next()? {
            // Art and details hold off until a burst of keys settles.
            lib.last_input_at = Instant::now();
            if lib.handle_key(input)? == Flow::Quit {
                break;
            }
        }
        lib.advance()?;
        if std::mem::take(&mut lib.clear_screen) {
            terminal::clear(&mut terminal)?;
        }
        // Adopt a market search or price result published off-thread.
        lib.browser.poll_market();
        lib.follow_selection();
    }

    terminal::clear(&mut terminal)?;
    Ok(())
}

impl Library {
    /// Load the config, probe the session, and seed the browser from the
    /// cached library if there is one.
    fn start() -> AppResult<Self> {
        let config = Config::new()?;
        let mut app = App::new(&config);
        let client = Client::new();
        // Always probe the session first (stored session or daemon). The
        // result decides between the library and the sign-in page.
        client.check_session()?;
        app.mode = Mode::Loading;
        let (browser, cached) = match client.games() {
            Ok(games) => (Browser::new(games), true),
            Err(_) => (Browser::new(Vec::new()), false),
        };
        Ok(Self {
            app,
            config,
            browser,
            client,
            cached,
            clear_screen: false,
            artwork: Artwork::default(),
            marquee: Marquee::default(),
            last_input_at: Instant::now()
                .checked_sub(Duration::from_secs(1))
                .unwrap_or_else(Instant::now),
        })
    }

    /// Adopt whatever the background workers finished since the last frame.
    /// Never blocks.
    fn poll_workers(&mut self) {
        let browser = &mut self.browser;
        // Adopt any async friend-management results (resolved search previews and
        // roster refreshes after an add/remove) before drawing this frame.
        browser.poll_friends_ops();

        // Drain any finished off-thread Workshop browse/search/subscribe worker
        // before rendering, so its result lands on the next frame. Runs on every
        // event (including ticks) and never blocks the UI thread.
        browser.poll_workshop();

        // Reconcile a game's install state once its install finishes, and adopt
        // any targeted single-game refresh that landed — so `installed` (which
        // gates uninstall) tracks reality without a full-screen reload.
        browser.poll_install_completions();
        browser.poll_game_refresh();

        // A just-finished Proton install flips a runtime to [installed]; pick
        // that up by refreshing the list once, then drop the status line. Done
        // before draw since the render closure only borrows `browser` immutably.
        if browser.show_proton && browser.proton_status_line().as_deref() == Some("Installed!") {
            browser.refresh_proton();
            browser.clear_proton_status();
        }
    }

    /// Move between screens on what the client reports: a finished login, a
    /// loaded library, a lost session, or a crash.
    fn advance(&mut self) -> AppResult {
        let Library {
            app,
            config,
            browser,
            client,
            cached,
            clear_screen,
            ..
        } = self;
        if app.mode == Mode::Loading {
            match client.get_state()? {
                State::Loaded(_, -2) => {
                    if *cached {
                        // Used the cached library; allow future reloads.
                        *cached = false;
                        app.mode = Mode::Browse;
                    } else {
                        client.load_games()?;
                    }
                }
                State::LoggedIn => {
                    // The games cache can be momentarily missing/empty mid-reload
                    // (right after a restart invalidates it). Treat an unreadable
                    // cache as "not ready yet" and stay in Loading to retry next
                    // iteration, rather than crashing the TUI on a parse error.
                    if let Ok(games) = client.games() {
                        config.save()?;
                        browser.set_items(games);
                        *clear_screen = true;
                        app.mode = Mode::Browse;
                    }
                }
                State::Failed => {
                    // No session — fall through to the sign-in landing page.
                    app.mode = Mode::Login;
                }
                _ => {}
            }
        }
        if app.mode == Mode::LoginQr {
            match client.get_state()? {
                State::Loaded(_, -2) => {
                    *cached = false;
                    app.mode = Mode::Loading;
                    *clear_screen = true;
                }
                State::Failed => {
                    app.mode = Mode::Login;
                }
                _ => {}
            }
        }
        if app.mode == Mode::LoginClassic || app.mode == Mode::LoginGuard {
            // A Steam Guard prompt mid-login switches us to the code entry page.
            if app.mode == Mode::LoginClassic
                && let LoginPhase::GuardCode(_) = client.get_login_phase()
            {
                app.guard_code.clear();
                app.mode = Mode::LoginGuard;
            }
            match client.get_state()? {
                State::Loaded(_, -2) => {
                    *cached = false;
                    app.mode = Mode::Loading;
                    *clear_screen = true;
                }
                State::Failed => {
                    app.mode = Mode::Login;
                }
                _ => {}
            }
        }
        if let State::Terminated(err) = client.get_state()? {
            app.mode = Mode::Terminated(err);
        }
        Ok(())
    }

    /// Keep the art and the lazy details (Proton tier, store metadata) on the
    /// highlighted game. Paused while an overlay covers the detail pane.
    fn follow_selection(&mut self) {
        if self.app.mode != Mode::Browse || self.browser.overlay_open() {
            return;
        }
        let selected = self.browser.selected();
        let settled = self.last_input_at.elapsed() >= ARTWORK_IDLE;
        self.artwork.update(selected.as_ref(), settled);
        if settled && let Some(game) = selected.as_ref() {
            game.query_proton();
            game.query_info();
        }
    }
}
