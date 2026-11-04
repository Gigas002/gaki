//! `auth` command handlers.

use crate::daemon;
use crate::output;

use crate::commands::common::*;

use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use libgaki::core::config::experimental_enabled;
use libgaki::core::error::{ErrorKind, Reported, TypedError, classify};
use libgaki::core::models::QrEvent;
use libgaki::ops::auth::{LoginStep, PasswordLogin, StoredSession};
use libgaki::steam_client::SteamClient;

/// Refuse an experimental command unless experimental features are enabled.
///
/// The browser/OpenID and web-token flows only prove identity / enable the
/// web-surface commands — they never create the full client session that
/// library/install/launch need — so they are kept out of the core feature set to
/// avoid confusion. Opt in per-run with `AURELIA_EXPERIMENTAL=1` or persistently
/// with `aurelia config experimental true`.
pub(crate) async fn require_experimental(what: &str) -> Result<()> {
    if experimental_enabled().await {
        return Ok(());
    }
    bail!(
        "`{what}` is an experimental feature and is disabled by default. It proves \
         identity / enables web-only commands but cannot create a full session for \
         library, install, or launch — use `aurelia login` or `aurelia login --qr` for \
         those. To enable it, run `aurelia config experimental true` (or set \
         AURELIA_EXPERIMENTAL=1)."
    )
}

/// Warn (on stderr) when a session already exists.
async fn warn_if_already_logged_in() {
    match libgaki::ops::auth::stored_session().await {
        StoredSession::Account(name) => cli_eprintln!(
            "Warning: already logged in as {name} — logging in again replaces that session."
        ),
        StoredSession::Unknown => {
            cli_eprintln!("Warning: an existing session is active — logging in again replaces it.")
        }
        StoredSession::None => {}
    }
}

pub(crate) async fn cmd_login(args: crate::cli::LoginArgs, json: bool) -> Result<()> {
    let crate::cli::LoginArgs {
        username,
        password,
        guard,
        qr,
        timeout,
        code,
        openid,
        web_token,
        health,
        reconnect,
    } = args;
    if health {
        return cmd_login_health(json).await;
    }
    if reconnect {
        return cmd_login_reconnect(json).await;
    }
    if qr {
        return cmd_login_qr(timeout, json).await;
    }
    if openid {
        require_experimental("login --openid").await?;
        return cmd_login_openid(json).await;
    }
    if let Some(pasted) = web_token {
        require_experimental("login --web-token").await?;
        return cmd_login_web_token(pasted, json).await;
    }

    // In `--json` mode the login is driven non-interactively (e.g. by Heroic): no
    // TTY prompts. Credentials come from flags/env, and a Steam Guard code is
    // requested via a `{event:"guard_required",...}` line and read back from stdin.
    if json {
        return cmd_login_json(username, password, guard).await;
    }

    let username = match username {
        Some(u) => u,
        None => {
            warn_if_already_logged_in().await;
            prompt_line("Steam username: ")?
        }
    };
    let account = username.clone();
    let password = match password.or_else(|| std::env::var("AURELIA_PASSWORD").ok()) {
        Some(p) => p,
        None => {
            rpassword::prompt_password("Steam password: ").context("failed reading password")?
        }
    };

    let mut client = SteamClient::new()?;
    // `--code` (alias `--pin`) lets the Steam client read the Guard code from
    // stdin itself; otherwise we wait for Mobile-app approval and only prompt
    // for a code when Steam asks for one.
    let mut request = PasswordLogin {
        username,
        password,
        guard_code: guard,
        read_code_from_stdin: code,
        timeout: None,
    };
    match libgaki::ops::auth::login(&mut client, &request).await? {
        LoginStep::LoggedIn => {}
        LoginStep::CodeRequired(_) => {
            tracing::info!("Login method awaited: Steam Guard code");
            request.guard_code = Some(prompt_line("Steam Guard code: ")?);
            request.read_code_from_stdin = false;
            match libgaki::ops::auth::login(&mut client, &request)
                .await
                .context("login failed after providing Steam Guard code")?
            {
                LoginStep::LoggedIn => {}
                _ => bail!("login failed after providing Steam Guard code"),
            }
        }
        LoginStep::ApprovalRequired => {
            tracing::info!("Login method awaited: Steam Mobile app approval");
            bail!("approve this login in the Steam Mobile app, then run `aurelia login` again")
        }
    }
    report_login_success(&account, json);
    Ok(())
}

/// Log in by scanning a QR code with the Steam Mobile app.
///
/// In `--json` mode the challenge URL is streamed as `{event:"qr_challenge",url}`
/// (re-emitted whenever Steam rotates the code) so a driver like Heroic can render
/// the QR itself; otherwise it's drawn to stderr as a terminal QR.
///
/// `--json`: exactly one typed result line.
pub(crate) async fn cmd_login_qr(timeout: Option<u64>, json: bool) -> Result<()> {
    let timeout = timeout
        .map(Duration::from_secs)
        .unwrap_or(libgaki::ops::auth::QR_DEFAULT_TIMEOUT);
    let mut client = SteamClient::new()?;
    if !json {
        let session = libgaki::ops::auth::login_qr(&mut client, timeout, render_login_qr).await?;
        report_login_success(&session.account_name.unwrap_or_default(), false);
        return Ok(());
    }

    let outcome = race_control(
        libgaki::ops::auth::login_qr(&mut client, timeout, emit_qr_challenge_json),
        watch_control(output::read_line_opt),
    )
    .await;
    match outcome {
        Ok(session) => {
            let account = session.account_name.clone().unwrap_or_default();
            report_login_success(&account, true);
            print_result_line(qr_result_ok(&account, session.steam_id));
            Ok(())
        }
        Err(err) => {
            print_result_line(qr_result_err(&err));
            Err(anyhow!(Reported(err)))
        }
    }
}

/// Run `work` unless the driver stops first.
async fn race_control<T>(
    work: impl std::future::Future<Output = Result<T>>,
    control: impl std::future::Future<Output = ErrorKind>,
) -> Result<T> {
    // Dropping the future closes its connection.
    tokio::select! {
        r = work => r,
        kind = control => Err(anyhow!(TypedError::new(kind, match kind {
            ErrorKind::Cancelled => "QR login cancelled by the driver",
            _ => "stdin closed; QR login abandoned",
        }))),
    }
}

/// Resolve when the driver cancels or leaves.
async fn watch_control<F, Fut>(mut next_line: F) -> ErrorKind
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = std::io::Result<Option<String>>>,
{
    loop {
        match next_line().await {
            Ok(Some(line)) if is_cancel_line(&line) => return ErrorKind::Cancelled,
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => return ErrorKind::InputClosed,
        }
    }
}

/// Whether a stdin line requests cancellation.
fn is_cancel_line(line: &str) -> bool {
    line.trim().eq_ignore_ascii_case("cancel")
}

/// The approved QR login's result line.
fn qr_result_ok(account: &str, steam_id: Option<u64>) -> serde_json::Value {
    serde_json::json!({
        "event": "result",
        "ok": true,
        "type": "approved",
        "account": account,
        "steam_id": steam_id,
    })
}

/// A failed QR login's result line.
fn qr_result_err(err: &anyhow::Error) -> serde_json::Value {
    let kind = classify(err).kind;
    let mut value = serde_json::json!({
        "event": "result",
        "ok": false,
        "type": kind,
        "error": format!("{err:#}"),
    });
    if let Some(hint) = kind.hint() {
        value["hint"] = hint.into();
    }
    value
}

/// Verify the user's identity on the official Steam sign-in page in the browser
/// (Steam OpenID 2.0 — Valve offers no OpenID Connect / OAuth2 endpoint).
///
/// The password is only ever typed on `steamcommunity.com`; Aurelia receives a
/// signed assertion naming the SteamID64 and confirms it with Steam directly.
/// Steam issues **no session token** over OpenID, so this verifies identity but
/// cannot mint a client session — it never touches the persisted session, and
/// the output says so explicitly.
///
/// In `--json` mode the sign-in URL is streamed as `{event:"openid_challenge",url}`
/// (the driver opens/renders it); otherwise the default browser is opened.
pub(crate) async fn cmd_login_openid(json: bool) -> Result<()> {
    let identity = libgaki::ops::auth::verify_openid(|url| {
        if json {
            eprint_json_line(&serde_json::json!({ "event": "openid_challenge", "url": url }));
        } else {
            cli_eprintln!("\nComplete the sign-in on the official Steam page:\n  {url}\n");
            if libgaki::web::openid::open_in_browser(url) {
                cli_eprintln!("(opened in your default browser)");
            } else {
                cli_eprintln!("(could not open a browser automatically — open the link yourself)");
            }
        }
    })
    .await?;
    let steam_id = identity.steam_id;
    let matches_session = identity.matches_stored_session;

    if json {
        print_json(&serde_json::json!({
            "openid_verified": true,
            "steam_id": steam_id,
            "matches_stored_session": matches_session,
            // Explicit: no session was created — Steam's OpenID attests identity only.
            "logged_in": false,
        }));
    } else {
        cli_println!("Steam identity verified on the official sign-in page.");
        cli_println!("SteamID64 : {steam_id}");
        match matches_session {
            Some(true) => cli_println!("Session   : matches the stored session's account"),
            Some(false) => cli_println!(
                "Session   : WARNING — this is a different account than the stored session"
            ),
            None => {}
        }
        cli_eprintln!(
            "\nNote: Steam's OpenID sign-in proves who you are, but Valve issues no client\n\
             session token over it. Commands that need a session still require\n\
             `aurelia login` or `aurelia login --qr`.\n\
             Your browser is now signed in, though — run `aurelia login --web-token` to\n\
             also enable the web commands (inventory, wallet, market listings)."
        );
    }
    Ok(())
}

/// Store a browser **web token** (the Legendary-style "paste the code" entry
/// point) so the web-surface commands work without a client login.
///
/// The token comes from `https://steamcommunity.com/chat/clientjstoken`, opened
/// in a browser signed in to Steam — e.g. straight after `login --openid`. A
/// driver with its own webview (Heroic) can capture that JSON automatically and
/// pass it as the flag's value or over stdin in `--json` mode (after a
/// `{event:"web_token_required"}` line, mirroring the Guard-code handshake).
///
/// Web-audience and short-lived (~24h): it powers inventory/wallet/market
/// listings, never CM commands, so it complements rather than replaces `login`.
/// Refuses a token for a different account than the stored session.
pub(crate) async fn cmd_login_web_token(pasted: Option<String>, json: bool) -> Result<()> {
    // Token sources, in order: the flag's value, the AURELIA_WEB_TOKEN env var
    // (drivers use it to avoid shell-quoting the JSON on the command line —
    // Windows spawn chains mangle embedded quotes), then prompt/stdin.
    let env_token = || {
        std::env::var("AURELIA_WEB_TOKEN")
            .ok()
            .filter(|v| !v.trim().is_empty())
    };
    let raw = match pasted.filter(|v| !v.trim().is_empty()).or_else(env_token) {
        Some(v) => v,
        None if json => {
            eprint_json_line(&serde_json::json!({
                "event": "web_token_required",
                "url": libgaki::web::web_token::CLIENTJSTOKEN_URL,
            }));
            read_stdin_line().await?
        }
        None => {
            cli_eprintln!(
                "\nIn the browser where you are signed in to Steam, open:\n  {}\n",
                libgaki::web::web_token::CLIENTJSTOKEN_URL
            );
            prompt_line("Paste the entire JSON line from that page: ")?
        }
    };

    let saved = libgaki::ops::auth::store_web_token(&raw).await?;
    let full_session = saved.full_session;

    if json {
        print_json(&serde_json::json!({
            "web_token_saved": true,
            "steam_id": saved.steam_id,
            "account": saved.account,
            // null for opaque (non-JWT) tokens — only Steam knows their expiry.
            "expires_at": saved.expires_at,
            // Web-audience token: the web commands work, CM commands do not.
            "logged_in": full_session,
        }));
    } else {
        cli_println!("Web token saved.");
        if let Some(account) = &saved.account {
            cli_println!("Account   : {account}");
        }
        cli_println!("SteamID64 : {}", saved.steam_id);
        match saved.expires_at {
            Some(exp) => cli_println!(
                "Expires   : {} UTC",
                crate::commands::common::format_unix_timestamp(exp)
            ),
            None => cli_println!("Expires   : unknown (typically ~24h — Steam decides)"),
        }
        if !full_session {
            cli_eprintln!(
                "\nThis enables the web commands (inventory, wallet, market listings) only.\n\
                 Library/install/launch still need a full `aurelia login` or `aurelia login --qr`.\n\
                 When it expires, reload the clientjstoken page and re-run `aurelia login --web-token`."
            );
        }
    }
    Ok(())
}

/// Non-interactive password login for `--json` drivers (e.g. Heroic). Credentials
/// come from flags or `AURELIA_PASSWORD`. To keep the driver informed (and never
/// silent), it emits:
/// - `{event:"awaiting_confirmation"}` right away — the login may block while
///   Steam waits for the Guard code or Mobile-app approval, so the driver should
///   prompt the user to approve on their device;
/// - `{event:"guard_required",type:"email"|"device"}` if a typed Guard code is
///   needed and none was supplied (the code is then read as one line from stdin);
/// - `{event:"guard_required",type:"device_confirmation"}` for mobile-approval
///   accounts;
/// - finally `{logged_in:true,...}` or `{error:...}`.
///
/// Each login attempt is bounded by [`libgaki::ops::auth::LOGIN_TIMEOUT`].
pub(crate) async fn cmd_login_json(
    username: Option<String>,
    password: Option<String>,
    guard: Option<String>,
) -> Result<()> {
    let username = username.context("--json login requires a username (-u/--username)")?;
    let password = password
        .or_else(|| std::env::var("AURELIA_PASSWORD").ok())
        .context("--json login requires a password (-p/--password or AURELIA_PASSWORD)")?;
    let account = username.clone();

    let mut client = SteamClient::new()?;

    // The login call below blocks while Steam waits for the Guard code / mobile
    // confirmation. Emit this first so the driver can immediately tell the user
    // to approve the login (otherwise it sees no output until the attempt
    // completes or times out).
    eprint_json_line(&serde_json::json!({
        "event": "awaiting_confirmation",
        "message": "Signing in — if prompted, approve this login in your Steam Mobile app."
    }));

    let mut request = PasswordLogin {
        username,
        password,
        guard_code: guard,
        read_code_from_stdin: false,
        timeout: Some(libgaki::ops::auth::LOGIN_TIMEOUT),
    };
    match libgaki::ops::auth::login(&mut client, &request).await? {
        LoginStep::LoggedIn => {}
        LoginStep::CodeRequired(kind) => {
            eprint_json_line(
                &serde_json::json!({ "event": "guard_required", "type": kind.as_str() }),
            );
            let code = read_stdin_line()
                .await
                .context("failed reading Steam Guard code from stdin")?;
            request.guard_code = Some(code);
            match libgaki::ops::auth::login(&mut client, &request)
                .await
                .context("login failed after providing Steam Guard code")?
            {
                LoginStep::LoggedIn => {}
                _ => bail!("login failed after providing Steam Guard code"),
            }
        }
        LoginStep::ApprovalRequired => {
            eprint_json_line(
                &serde_json::json!({ "event": "guard_required", "type": "device_confirmation" }),
            );
            bail!("approve this login in the Steam Mobile app, then run login again")
        }
    }
    report_login_success(&account, true);
    Ok(())
}

/// Emit a QR login challenge URL as one NDJSON line
pub(crate) fn emit_qr_challenge_json(event: QrEvent) {
    let line = match event {
        QrEvent::Challenge(url) => serde_json::json!({ "event": "qr_challenge", "url": url }),
        QrEvent::Scanned => serde_json::json!({ "event": "qr_scanned" }),
    };
    eprint_json_line(&line);
}

/// Read a single line from stdin (used to receive a Guard code from a `--json`
/// driver). Returns the trimmed contents. Routes through [`output::read_line`] so
/// that, inside the daemon, it reads the forwarding client's stdin.
pub(crate) async fn read_stdin_line() -> Result<String> {
    output::read_line()
        .await
        .context("failed reading stdin")
        .map(|s| s.trim().to_string())
}

/// Render a Steam login challenge URL as a scannable QR code on stderr, with the
/// raw URL as a fallback. Diagnostics go to stderr so stdout stays clean.
pub(crate) fn render_login_qr(event: QrEvent) {
    let url = match event {
        QrEvent::Challenge(url) => url,
        QrEvent::Scanned => {
            cli_eprintln!("\nQR code scanned — approve the sign-in in your Steam Mobile app.");
            return;
        }
    };
    match qrcode::QrCode::new(url.as_bytes()) {
        Ok(code) => {
            let rendered = code
                .render::<qrcode::render::unicode::Dense1x2>()
                .quiet_zone(true)
                .build();
            cli_eprintln!("\nScan this QR code with the Steam Mobile app:\n{rendered}");
        }
        Err(e) => cli_eprintln!("\n(could not render QR code: {e})"),
    }
    cli_eprintln!("Or open this link in the Steam Mobile app:\n  {url}\n");
}

pub(crate) fn report_login_success(account: &str, json: bool) {
    if json {
        // One line for NDJSON drivers.
        print_json_line(&serde_json::json!({ "logged_in": true, "account": account }));
    } else {
        cli_println!("Login successful.");
    }
}

pub(crate) async fn cmd_logout(json: bool) -> Result<()> {
    let mut client = restored_client().await?;
    libgaki::ops::auth::logout(&mut client).await?;
    if json {
        print_json(&serde_json::json!({ "logged_out": true }));
    } else {
        cli_println!("Logged out.");
    }
    Ok(())
}

/// `login --health`: report whether a session is authenticated, without logging in.
/// When a daemon is in use this reflects its shared session (no new logon); standalone
/// it does a one-off live restore check.
pub(crate) async fn cmd_login_health(json: bool) -> Result<()> {
    let via_daemon = daemon::in_daemon();
    let status = if via_daemon {
        daemon::session_status().await
    } else {
        let client = restored_client().await?;
        libgaki::ops::auth::session_status(Some(&client)).await
    };
    report_session_status(&status, via_daemon, json);
    Ok(())
}

/// `login --reconnect`: tear down and re-establish the daemon's shared session from
/// the stored token (for use after the live connection dropped).
pub(crate) async fn cmd_login_reconnect(json: bool) -> Result<()> {
    if !daemon::in_daemon() {
        bail!(
            "--reconnect needs the session daemon, but this command is running standalone \
             (AURELIA_NO_DAEMON is set, or the daemon is unreachable). Start `aurelia daemon` first."
        );
    }
    let status = daemon::force_reconnect().await;
    report_session_status(&status, true, json);
    Ok(())
}

/// Print a session status for `--health`/`--reconnect`.
pub(crate) fn report_session_status(
    status: &libgaki::ops::auth::SessionStatus,
    via_daemon: bool,
    json: bool,
) {
    if json {
        print_json(&serde_json::json!({
            "logged_in": status.authenticated,
            "account": status.account,
            "steam_id": status.steam_id,
            "web_token": status.web_token,
            "daemon": via_daemon,
        }));
    } else {
        cli_println!(
            "Session : {}",
            if status.authenticated {
                "authenticated"
            } else {
                "not logged in"
            }
        );
        if let Some(account) = &status.account {
            cli_println!("Account : {account}");
        }
        if let Some(steam_id) = status.steam_id {
            cli_println!("SteamID : {steam_id}");
        }
        if status.web_token {
            cli_println!("Web     : web token stored (inventory/wallet/market available)");
        }
        cli_println!(
            "Daemon  : {}",
            if via_daemon {
                "yes (shared session)"
            } else {
                "no (standalone)"
            }
        );
    }
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;
