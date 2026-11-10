//! `auth` command handlers.

use crate::daemon;
use crate::output;

use crate::commands::common::*;

use std::time::Duration;

use anyhow::{Context, Result, bail};
use libgaki::core::config::experimental_enabled;
use libgaki::core::models::QrEvent;
use libgaki::ops::auth::{LoginStep, PasswordLogin, StoredSession};
use libgaki::steam_client::SteamClient;

/// Refuse an experimental command unless experimental features are enabled.
///
/// The browser/OpenID and web-token flows only prove identity / enable the
/// web-surface commands — they never create the full client session that
/// library/install/launch need — so they are kept out of the core feature set to
/// avoid confusion. Opt in with `gaki-cli config experimental true`.
pub(crate) async fn require_experimental(what: &str) -> Result<()> {
    if experimental_enabled().await {
        return Ok(());
    }
    bail!(
        "`{what}` is an experimental feature and is disabled by default. It proves \
         identity / enables web-only commands but cannot create a full session for \
         library, install, or launch — use `gaki-cli login` or `gaki-cli login --qr` for \
         those. To enable it, run `gaki-cli config experimental true`."
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

pub(crate) async fn cmd_login(args: crate::cli::LoginArgs) -> Result<()> {
    let crate::cli::LoginArgs {
        username,
        password,
        password_stdin,
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
        return cmd_login_health().await;
    }
    if reconnect {
        return cmd_login_reconnect().await;
    }
    if qr {
        return cmd_login_qr(timeout).await;
    }
    if openid {
        require_experimental("login --openid").await?;
        return cmd_login_openid().await;
    }
    if let Some(pasted) = web_token {
        require_experimental("login --web-token").await?;
        return cmd_login_web_token(pasted).await;
    }
    let password = if password_stdin {
        Some(read_password_stdin().await?)
    } else {
        password
    };

    let username = match username {
        Some(u) => u,
        None => {
            warn_if_already_logged_in().await;
            prompt_line("Steam username: ")?
        }
    };
    let password = match password {
        Some(p) => p,
        None => {
            rpassword::prompt_password("Steam password: ").context("failed reading password")?
        }
    };

    password_login_interactive(username, password, guard, code).await?;
    report_login_success();
    Ok(())
}

/// A password login at the terminal. `--code` (alias `--pin`) lets the Steam
/// client read the Guard code from stdin itself; otherwise we wait for
/// Mobile-app approval and only prompt for a code when Steam asks for one.
async fn password_login_interactive(
    username: String,
    password: String,
    guard: Option<String>,
    code: bool,
) -> Result<SteamClient> {
    let mut client = SteamClient::new()?;
    let mut request = PasswordLogin {
        username,
        password: password.into(),
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
            bail!("approve this login in the Steam Mobile app, then run `gaki-cli login` again")
        }
    }
    Ok(client)
}

/// A web session and the grant for one sensitive read (inventory, wallet,
/// market listings). With `reauth_sensitive` on this signs in again first:
/// the stored account's password, or a QR code when the password is left
/// empty.
#[cfg(feature = "inventory")]
pub(crate) async fn sensitive_access() -> Result<(
    libgaki::web::web_session::WebSession,
    libgaki::ops::auth::SensitiveGrant,
)> {
    use libgaki::ops::auth::{reauth_needed, sensitive_grant};
    if !reauth_needed() {
        let web = web_access().await?;
        return Ok((web, sensitive_grant()?));
    }
    cli_eprintln!("reauth_sensitive is on: sign in again to continue.");
    let stored = libgaki::auth::load_session()
        .await
        .ok()
        .and_then(|session| session.account_name);
    let username = match stored {
        Some(name) => {
            cli_eprintln!("Account: {name}");
            name
        }
        None => prompt_line("Steam username: ")?,
    };
    let password = rpassword::prompt_password("Steam password (empty for a QR code): ")
        .context("failed reading password")?;
    let client = if password.is_empty() {
        let mut client = SteamClient::new()?;
        libgaki::ops::auth::login_qr(
            &mut client,
            libgaki::ops::auth::QR_DEFAULT_TIMEOUT,
            render_login_qr,
        )
        .await?;
        client
    } else {
        password_login_interactive(username, password, None, false).await?
    };
    let grant = sensitive_grant()?;
    let web = libgaki::ops::session::web_session(&client).await?;
    Ok((web, grant))
}

/// Log in by scanning a QR code with the Steam Mobile app. The code is drawn
/// to stderr, and redrawn whenever Steam rotates it.
pub(crate) async fn cmd_login_qr(timeout: Option<u64>) -> Result<()> {
    let timeout = timeout
        .map(Duration::from_secs)
        .unwrap_or(libgaki::ops::auth::QR_DEFAULT_TIMEOUT);
    let mut client = SteamClient::new()?;
    libgaki::ops::auth::login_qr(&mut client, timeout, render_login_qr).await?;
    report_login_success();
    Ok(())
}
/// Verify the user's identity on the official Steam sign-in page in the browser
/// (Steam OpenID 2.0 — Valve offers no OpenID Connect / OAuth2 endpoint).
///
/// The password is only ever typed on `steamcommunity.com`; gaki receives a
/// signed assertion naming the SteamID64 and confirms it with Steam directly.
/// Steam issues **no session token** over OpenID, so this verifies identity but
/// cannot mint a client session — it never touches the persisted session, and
/// the output says so explicitly.
///
/// The sign-in page opens in the default browser.
pub(crate) async fn cmd_login_openid() -> Result<()> {
    let identity = libgaki::ops::auth::verify_openid(|url| {
        cli_eprintln!("\nComplete the sign-in on the official Steam page:\n  {url}\n");
        if libgaki::web::openid::open_in_browser(url) {
            cli_eprintln!("(opened in your default browser)");
        } else {
            cli_eprintln!("(could not open a browser automatically — open the link yourself)");
        }
    })
    .await?;
    let steam_id = identity.steam_id;
    let matches_session = identity.matches_stored_session;

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
         `gaki-cli login` or `gaki-cli login --qr`.\n\
         Your browser is now signed in, though — run `gaki-cli login --web-token` to\n\
         also enable the web commands (inventory, wallet, market listings)."
    );

    Ok(())
}

/// Store a browser **web token** (the Legendary-style "paste the code" entry
/// point) so the web-surface commands work without a client login.
///
/// The token comes from `https://steamcommunity.com/chat/clientjstoken`, opened
/// in a browser signed in to Steam — e.g. straight after `login --openid`.
///
/// Web-audience and short-lived (~24h): it powers inventory/wallet/market
/// listings, never CM commands, so it complements rather than replaces `login`.
/// Refuses a token for a different account than the stored session.
pub(crate) async fn cmd_login_web_token(pasted: Option<String>) -> Result<()> {
    // Token sources: the flag's value, then a prompt (which spares shell-quoting
    // the JSON on the command line).
    let raw = match pasted.filter(|v| !v.trim().is_empty()) {
        Some(v) => v,
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
             Library/install/launch still need a full `gaki-cli login` or `gaki-cli login --qr`.\n\
             When it expires, reload the clientjstoken page and re-run `gaki-cli login --web-token`."
        );
    }

    Ok(())
}

/// Read a password from one line of stdin, dropping only the line ending (a
/// password may begin or end with spaces).
pub(crate) async fn read_password_stdin() -> Result<String> {
    let line = output::read_line().await.context("failed reading stdin")?;
    Ok(line.trim_end_matches(['\r', '\n']).to_string())
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

pub(crate) fn report_login_success() {
    cli_println!("Login successful.");
}

pub(crate) async fn cmd_logout() -> Result<()> {
    use libgaki::ops::auth::Revocation;
    // The daemon's live session can revoke without a second logon. Standalone,
    // logout restores only to revoke: a session that can't be restored is
    // still removed. In the daemon, the shared session is dropped afterwards.
    let mut client = if daemon::in_daemon() {
        daemon::shared_restored_client().await
    } else {
        libgaki::steam_client::SteamClient::new()?
    };
    let revocation = libgaki::ops::auth::logout(&mut client).await?;
    match &revocation {
        Revocation::Revoked => cli_println!("Logged out; Steam revoked the refresh token."),
        Revocation::NoToken => cli_println!("Logged out."),
        Revocation::Failed(reason) => {
            cli_println!("Logged out locally.");
            cli_eprintln!(
                "Warning: Steam could not revoke the refresh token ({reason}); a copy of it \
                 may keep working until it expires. Revoke it from another device under \
                 Steam Guard → \"Deauthorize all other devices\" if that matters."
            );
        }
    }
    Ok(())
}

/// `login --health`: report whether a session is authenticated, without logging in.
/// When a daemon is in use this reflects its shared session (no new logon); standalone
/// it does a one-off live restore check.
pub(crate) async fn cmd_login_health() -> Result<()> {
    let via_daemon = daemon::in_daemon();
    let status = if via_daemon {
        daemon::session_status().await
    } else {
        let client = restored_client().await?;
        libgaki::ops::auth::session_status(Some(&client)).await
    };
    report_session_status(&status, via_daemon);
    Ok(())
}

/// `login --reconnect`: tear down and re-establish the daemon's shared session from
/// the stored token (for use after the live connection dropped).
pub(crate) async fn cmd_login_reconnect() -> Result<()> {
    if !daemon::in_daemon() {
        bail!(
            "--reconnect needs the session daemon, but this command is running standalone \
             (--no-daemon was given, or the daemon is unreachable). Start `gaki-cli daemon` first."
        );
    }
    let status = daemon::force_reconnect().await;
    report_session_status(&status, true);
    Ok(())
}

/// Print a session status for `--health`/`--reconnect`.
pub(crate) fn report_session_status(status: &libgaki::ops::auth::SessionStatus, via_daemon: bool) {
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
