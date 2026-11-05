//! The session client. A background thread runs the long operations (session
//! check, library load, logins, installs) through [`crate::app::steam`], and
//! the run loop polls their progress as a [`State`].

use crate::app::game::status::GameStatus;
use crate::app::game::{self, Game};
use crate::app::steam::{self, LoginPhase};

use crate::utils::error::STError;
use crate::utils::paths::{cache_location, invalidate_cache};

use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(PartialEq, Clone)]
pub enum State {
    LoggedOut,
    LoggedIn,
    Failed,
    Terminated(String),
    /// `(loaded, total)`. `total == -2` signals "session is authenticated, ready
    /// to load the library"; `total == -1` means "loading, total unknown".
    Loaded(i32, i32),
}

pub enum Command {
    /// Verify the session. The `String` is the user
    /// name typed in the UI; it is only used for display/bookkeeping.
    Login(String),
    /// Probe the session health without a user name (stored session or daemon).
    CheckSession,
    /// Begin QR-code login attempt `n`, streaming challenge URLs into the
    /// shared QR cell while it is the current attempt.
    LoginQr(u64),
    /// Begin a classic username/password login. The `Receiver` delivers a Steam
    /// Guard code from the UI if one is prompted for.
    LoginClassic(String, String, Receiver<String>),
    /// Fetch and cache the full library.
    LoadGames,
    /// Invalidate the cache and re-verify the session.
    Restart,
    /// Install a game, streaming progress into the shared status cell. The
    /// control handle lets the UI pause/stop the in-flight download; the last
    /// field is the chosen library folder (drive/location), `None` for default.
    Install(
        i32,
        Arc<Mutex<Option<GameStatus>>>,
        steam::InstallControl,
        Option<String>,
    ),
    /// Verify a game's integrity, streaming progress into the shared status cell.
    Verify(i32, Arc<Mutex<Option<GameStatus>>>),
    /// Update a game to the latest version, streaming progress into the shared status cell.
    Update(i32, Arc<Mutex<Option<GameStatus>>>),
    /// Launch a game and wait for it to exit.
    Run(i32, Arc<Mutex<Option<GameStatus>>>),
    /// Launch a game with extra `play` flags (e.g. a chosen launch target).
    RunWith(i32, steam::PlayOpts, Arc<Mutex<Option<GameStatus>>>),
    /// Download and install a Proton/Wine runtime by name, streaming progress
    /// into the shared status cell.
    ProtonInstall(String, Arc<Mutex<Option<GameStatus>>>),
    /// No-op kept for UI compatibility (launching brings up what it needs).
    StartClient,
}

/// Set the session state from a session health check. A failed check is
/// recorded in `last_error` so the UI can show it instead of silently
/// dropping back to the sign-in page.
fn handle_login(
    state: &Arc<Mutex<State>>,
    last_error: &Arc<Mutex<Option<String>>>,
    account: &Arc<Mutex<Option<String>>>,
) -> Result<(), STError> {
    let (next, err, who) = match steam::health() {
        Ok(health) if health.logged_in => {
            tracing::info!(account = ?health.account, "session authenticated");
            // The footer shows `steam: {who}`. Prefer the Steam persona / display
            // name over the login (account) name: the health probe only reports
            // the account name, so resolve the persona for our own SteamID via
            // `friends search` (best-effort; falls back to the account name).
            let who = health.account.map(|account| {
                health
                    .steam_id
                    .filter(|id| *id != 0)
                    .and_then(|id| steam::friends_search(&id.to_string()).ok())
                    .and_then(|f| f.persona_name)
                    .filter(|n| !n.is_empty())
                    .unwrap_or(account)
            });
            (State::Loaded(0, -2), None, who)
        }
        Ok(_) => {
            tracing::info!("session not authenticated");
            (State::Failed, None, None)
        }
        Err(err) => {
            tracing::warn!(error = %err, "session health check failed");
            (State::Failed, Some(format!("{}", err)), None)
        }
    };
    *last_error.lock()? = err;
    *account.lock()? = who;
    *state.lock()? = next;
    Ok(())
}

/// Fetch the library and write it to the cache, then mark the session loaded.
fn handle_load_games(state: &Arc<Mutex<State>>) -> Result<(), STError> {
    match game::load_library() {
        Ok(mut games) => {
            games.sort_by_key(|a| a.name.to_lowercase());
            fs::write(cache_location()?, serde_json::to_string(&games)?)?;
            *state.lock()? = State::LoggedIn;
            tracing::info!(games = games.len(), "loaded library");
        }
        Err(err) => {
            tracing::warn!(error = %err, "failed to load library");
            *state.lock()? = State::Terminated(format!("Failed to load library:\n{}", err));
        }
    }
    Ok(())
}

fn execute(
    state: Arc<Mutex<State>>,
    qr: Arc<Mutex<Option<String>>>,
    qr_attempt: Arc<AtomicU64>,
    login_phase: Arc<Mutex<LoginPhase>>,
    last_error: Arc<Mutex<Option<String>>>,
    account: Arc<Mutex<Option<String>>>,
    receiver: Receiver<Command>,
) -> Result<(), STError> {
    let mut user = String::new();
    while let Ok(command) = receiver.recv() {
        match command {
            Command::Login(u) => {
                if !u.is_empty() {
                    user = u;
                }
                handle_login(&state, &last_error, &account)?;
            }
            Command::CheckSession => {
                handle_login(&state, &last_error, &account)?;
            }
            Command::LoginQr(attempt) => {
                // Run the (blocking) QR login on its own thread so the worker
                // stays responsive; it updates the shared state when it
                // resolves, unless the UI cancelled it or started another.
                let state = state.clone();
                let qr = qr.clone();
                let qr_attempt = qr_attempt.clone();
                let last_error = last_error.clone();
                thread::spawn(move || {
                    let stale = || qr_attempt.load(Ordering::SeqCst) != attempt;
                    let result = steam::login_qr(&qr, stale);
                    if stale() {
                        return;
                    }
                    let next = match result {
                        Ok(()) => State::Loaded(0, -2),
                        Err(err) => {
                            tracing::warn!(error = %err, "QR login failed");
                            if let Ok(mut e) = last_error.lock() {
                                *e = Some(format!("QR sign-in failed: {}", err));
                            }
                            State::Failed
                        }
                    };
                    if let Ok(mut guard) = state.lock() {
                        *guard = next;
                    }
                });
            }
            Command::LoginClassic(username, password, guard_rx) => {
                let state = state.clone();
                let phase = login_phase.clone();
                let last_error = last_error.clone();
                thread::spawn(move || {
                    let next = match steam::login_classic(&username, &password, &phase, guard_rx) {
                        Ok(()) => State::Loaded(0, -2),
                        Err(err) => {
                            tracing::warn!(error = %err, "classic login failed");
                            if let Ok(mut e) = last_error.lock() {
                                *e = Some(format!("Sign-in failed: {}", err));
                            }
                            State::Failed
                        }
                    };
                    if let Ok(mut guard) = state.lock() {
                        *guard = next;
                    }
                });
            }
            Command::LoadGames => {
                handle_load_games(&state)?;
            }
            Command::Restart => {
                // Drop the cached library so the next load is a fresh fetch.
                let _ = invalidate_cache();
                *state.lock()? = State::LoggedOut;
                tracing::info!(user = ?user, "restarting for user");
                handle_login(&state, &last_error, &account)?;
            }
            Command::Install(id, status, control, library) => {
                thread::spawn(move || steam::install(id, status, control, library));
            }
            Command::Verify(id, status) => {
                thread::spawn(move || steam::verify(id, status));
            }
            Command::Update(id, status) => {
                thread::spawn(move || steam::update(id, status));
            }
            Command::Run(id, status) => {
                thread::spawn(move || steam::play(id, status));
            }
            Command::RunWith(id, opts, status) => {
                thread::spawn(move || steam::play_with(id, opts, status));
            }
            Command::ProtonInstall(name, status) => {
                thread::spawn(move || steam::proton_install(name, status));
            }
            Command::StartClient => {
                // Launching brings up whatever it needs; nothing to do.
            }
        }
    }
    Ok(())
}

/// Manages and interfaces with the backend worker thread.
pub struct Client {
    sender: Mutex<Sender<Command>>,
    state: Arc<Mutex<State>>,
    /// Latest QR-login challenge URL, published by the current QR attempt.
    qr: Arc<Mutex<Option<String>>>,
    /// The current QR attempt; bumping it cancels the one in flight.
    qr_attempt: Arc<AtomicU64>,
    /// Progress of an in-flight classic (username/password) login.
    login_phase: Arc<Mutex<LoginPhase>>,
    /// Sender for delivering a Steam Guard code to the active classic login.
    guard_tx: Mutex<Option<Sender<String>>>,
    /// Last health/login failure message, shown on the sign-in page.
    last_error: Arc<Mutex<Option<String>>>,
    /// The authenticated account name, from the last health check.
    account: Arc<Mutex<Option<String>>>,
}

impl Client {
    /// Spawns the backend worker thread.
    pub fn new() -> Client {
        let (tx, rx) = channel();
        let client = Client {
            sender: Mutex::new(tx),
            state: Arc::new(Mutex::new(State::LoggedOut)),
            qr: Arc::new(Mutex::new(None)),
            qr_attempt: Arc::new(AtomicU64::new(0)),
            login_phase: Arc::new(Mutex::new(LoginPhase::Idle)),
            guard_tx: Mutex::new(None),
            last_error: Arc::new(Mutex::new(None)),
            account: Arc::new(Mutex::new(None)),
        };
        Client::start_process(
            client.state.clone(),
            client.qr.clone(),
            client.qr_attempt.clone(),
            client.login_phase.clone(),
            client.last_error.clone(),
            client.account.clone(),
            rx,
        );
        client
    }

    /// The authenticated account name, if known (from the last health check).
    pub fn get_account(&self) -> Option<String> {
        self.account.lock().ok().and_then(|a| a.clone())
    }

    /// Ensures `State` is `State::LoggedIn`.
    pub fn is_logged_in(&self) -> Result<bool, STError> {
        Ok(self.get_state()? == State::LoggedIn)
    }

    pub fn get_state(&self) -> Result<State, STError> {
        Ok(self.state.lock()?.clone())
    }

    /// Probe whether a session is already available (stored session or daemon),
    /// without a user name. Drives the startup login-vs-library decision.
    pub fn check_session(&self) -> Result<(), STError> {
        *self.state.lock()? = State::LoggedOut;
        *self.last_error.lock()? = None;
        self.sender.lock()?.send(Command::CheckSession)?;
        Ok(())
    }

    /// The last health/login failure message, if any.
    pub fn get_last_error(&self) -> Option<String> {
        self.last_error.lock().ok().and_then(|e| e.clone())
    }

    /// Start a QR-code login. Poll [`Client::get_qr`] for the challenge to
    /// render and [`Client::get_state`] for completion (`Loaded(_, -2)`) or
    /// failure (`Failed`).
    pub fn login_qr(&self) -> Result<(), STError> {
        let attempt = self.qr_attempt.fetch_add(1, Ordering::SeqCst) + 1;
        *self.state.lock()? = State::LoggedOut;
        *self.qr.lock()? = None;
        *self.last_error.lock()? = None;
        self.sender.lock()?.send(Command::LoginQr(attempt))?;
        Ok(())
    }

    /// Abandon the QR login in flight: it stops, and neither its code nor
    /// its outcome reaches the UI.
    pub fn cancel_qr(&self) -> Result<(), STError> {
        self.qr_attempt.fetch_add(1, Ordering::SeqCst);
        *self.qr.lock()? = None;
        Ok(())
    }

    /// The current QR-login challenge URL, if one is pending.
    pub fn get_qr(&self) -> Option<String> {
        self.qr.lock().ok().and_then(|q| q.clone())
    }

    /// Start a classic username/password login. Poll [`Client::get_login_phase`]
    /// for progress (including a Steam Guard prompt) and [`Client::get_state`]
    /// for completion (`Loaded(_, -2)`) or failure (`Failed`).
    pub fn login_classic(&self, username: &str, password: &str) -> Result<(), STError> {
        *self.state.lock()? = State::LoggedOut;
        *self.login_phase.lock()? = LoginPhase::Connecting;
        *self.last_error.lock()? = None;
        let (tx, rx) = channel();
        *self.guard_tx.lock()? = Some(tx);
        self.sender.lock()?.send(Command::LoginClassic(
            username.to_string(),
            password.to_string(),
            rx,
        ))?;
        Ok(())
    }

    /// Deliver a Steam Guard code to the in-flight classic login.
    pub fn submit_guard_code(&self, code: &str) -> Result<(), STError> {
        // Clear the GuardCode phase up front so the UI doesn't immediately
        // re-prompt before the login thread advances.
        *self.login_phase.lock()? = LoginPhase::Connecting;
        if let Some(tx) = &*self.guard_tx.lock()? {
            let _ = tx.send(code.to_string());
        }
        Ok(())
    }

    /// The current classic-login phase.
    pub fn get_login_phase(&self) -> LoginPhase {
        self.login_phase
            .lock()
            .map(|p| p.clone())
            .unwrap_or(LoginPhase::Idle)
    }

    /// Queues installation of the provided game. `control` lets the UI pause or
    /// stop the in-flight download (see [`crate::app::browser::Browser::begin_install`]); `library`
    /// is the chosen install location (a library root), `None` for default.
    pub fn install(
        &self,
        game: &Game,
        control: steam::InstallControl,
        library: Option<String>,
    ) -> Result<(), STError> {
        let sender = self.sender.lock()?;
        sender.send(Command::Install(
            game.id,
            game.status_counter(),
            control,
            library,
        ))?;
        Ok(())
    }

    /// Queues an integrity check of the provided game.
    pub fn verify(&self, game: &Game) -> Result<(), STError> {
        let sender = self.sender.lock()?;
        sender.send(Command::Verify(game.id, game.status_counter()))?;
        Ok(())
    }

    /// Queues an update of the provided game.
    pub fn update(&self, game: &Game) -> Result<(), STError> {
        let sender = self.sender.lock()?;
        sender.send(Command::Update(game.id, game.status_counter()))?;
        Ok(())
    }

    /// Queues a Proton/Wine runtime install by name, streaming progress into the
    /// shared status cell.
    pub fn proton_install(
        &self,
        name: &str,
        status: Arc<Mutex<Option<GameStatus>>>,
    ) -> Result<(), STError> {
        let sender = self.sender.lock()?;
        sender.send(Command::ProtonInstall(name.to_string(), status))?;
        Ok(())
    }

    /// Invalidates the cache and re-verifies the session, forcing a fresh load.
    pub fn restart(&self) -> Result<(), STError> {
        // Move off `LoggedIn` synchronously so the Loading loop doesn't act on
        // the stale state — and read the games cache the worker is about to
        // invalidate — before the reload completes. `Loaded(0, -1)` is the
        // "loading" sentinel the loop waits on.
        *self.state.lock()? = State::Loaded(0, -1);
        let sender = self.sender.lock()?;
        sender.send(Command::Restart)?;
        Ok(())
    }

    /// Launches the provided game.
    pub fn run(&self, game: &Game) -> Result<(), STError> {
        let sender = self.sender.lock()?;
        sender.send(Command::Run(game.id, game.status_counter()))?;
        Ok(())
    }

    /// Launches the provided game with extra flags.
    pub fn run_with(&self, game: &Game, opts: steam::PlayOpts) -> Result<(), STError> {
        let sender = self.sender.lock()?;
        sender.send(Command::RunWith(game.id, opts, game.status_counter()))?;
        Ok(())
    }

    /// Verifies the session for the provided user.
    /// The actual credential entry happens out-of-band.
    pub fn login(&self, user: &str) -> Result<(), STError> {
        if user.is_empty() {
            return Err(STError::Problem(
                "Blank string. Requires user to log in.".to_string(),
            ));
        }
        let mut state = self.state.lock()?;
        *state = State::LoggedOut;
        let sender = self.sender.lock()?;
        sender.send(Command::Login(user.to_string()))?;
        Ok(())
    }

    /// Fetches the full library and writes it to the cache.
    /// The state moves to `State::Loaded(0, -1)` while loading and
    /// `State::LoggedIn` once the cache has been written.
    pub fn load_games(&self) -> Result<(), STError> {
        let mut state = self.state.lock()?;
        *state = State::Loaded(0, -1);
        let sender = self.sender.lock()?;
        sender.send(Command::LoadGames)?;
        Ok(())
    }

    /// Extracts games from the cache written by [`Client::load_games`].
    pub fn games(&self) -> Result<Vec<Game>, STError> {
        let db_content = fs::read_to_string(cache_location()?)?;
        let parsed: Vec<Game> = serde_json::from_str(&db_content)?;
        Ok(parsed)
    }

    /// Kept for UI compatibility; launching brings up what it needs.
    pub fn start_client(&self) -> Result<(), STError> {
        let sender = self.sender.lock()?;
        sender.send(Command::StartClient)?;
        Ok(())
    }

    fn start_process(
        state: Arc<Mutex<State>>,
        qr: Arc<Mutex<Option<String>>>,
        qr_attempt: Arc<AtomicU64>,
        login_phase: Arc<Mutex<LoginPhase>>,
        last_error: Arc<Mutex<Option<String>>>,
        account: Arc<Mutex<Option<String>>>,
        receiver: Receiver<Command>,
    ) {
        thread::spawn(move || {
            let local = state.clone();
            if let Err(e) = execute(
                state,
                qr,
                qr_attempt,
                login_phase,
                last_error,
                account,
                receiver,
            ) {
                let mut state = local
                    .lock()
                    .expect("We need to inform the other thread that this broke.");
                *state = State::Terminated(format!("Fatal Error in client thread:\n{}", e));
            }
        });
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::app::client::Client;
    use crate::utils::error::STError;

    #[test]
    fn test_blank_login() {
        let client = Client::new();
        let result = client.login("");
        if let Err(STError::Problem(expected)) = result {
            assert!(expected.contains(&"Blank".to_string()));
            return;
        }
        panic!("Failed to unwrap")
    }
}
