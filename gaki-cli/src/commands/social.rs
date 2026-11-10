//! `social` command handlers.

use crate::daemon;
use crate::output;

use crate::commands::common::*;

use anyhow::Result;
use libgaki::ops::social;

/// `gaki-cli friends`: list the logged-in user's friends with status and game.
///
/// Inside the daemon the roster is served from the background watcher's cache;
/// standalone it does a short best-effort collection over a fresh connection
/// (which may be partial — see `SteamClient::collect_friends`).
pub(crate) async fn cmd_friends() -> Result<()> {
    let friends = if daemon::in_daemon() {
        daemon::shared_roster().await
    } else {
        let client = authed_client().await?;
        social::friends(&client, social::FRIENDS_WAIT).await?
    };

    if friends.is_empty() {
        cli_println!(
            "No friends to show yet. (If you just started a session, the roster fills a moment \
             after connecting — try again.)"
        );
        return Ok(());
    }

    cli_println!("{:<20}  {:<17}  {:<12}  GAME", "STATUS", "STEAMID", "NAME");
    for f in &friends {
        let name = f.persona_name.as_deref().unwrap_or("?");
        let game = match (&f.game_name, f.game_app_id) {
            (Some(g), _) => g.clone(),
            (None, Some(id)) => format!("app {id}"),
            (None, None) => String::new(),
        };
        cli_println!(
            "{:<20}  {:<17}  {:<12}  {}",
            persona_state_label(f.persona_state),
            f.steam_id,
            name,
            game
        );
    }
    cli_println!("\n{} friend(s).", friends.len());
    Ok(())
}

/// `gaki-cli user IDENT`: profile over the CM.
pub(crate) async fn cmd_user(user: String) -> Result<()> {
    let client = authed_client().await?;
    let steam_id = resolve_user_ident(&client, &user).await?;
    let profile = social::profile(&client, steam_id).await?;
    cli_println!(
        "{}  ({})",
        profile
            .persona_name
            .as_deref()
            .unwrap_or("(no persona name)"),
        profile.steam_id
    );
    cli_println!("Profile  : {}", profile.profile_url);
    if let Some(url) = &profile.avatar_url {
        cli_println!("Avatar   : {url}");
    }
    let visibility = social::visibility_label(profile.visibility);
    let limited = if profile.is_limited {
        ", limited account"
    } else {
        ""
    };
    cli_println!("Privacy  : {visibility}{limited}");
    if let Some(name) = &profile.real_name {
        cli_println!("Name     : {name}");
    }
    if let Some(place) = &profile.location {
        cli_println!("Location : {place}");
    }
    if let Some(t) = profile.created {
        cli_println!(
            "Created  : {}",
            libgaki::steam_client::unix_to_ymd(t as i64)
        );
    }
    cli_println!("Status   : {}", persona_state_label(profile.persona_state));
    match (&profile.game_name, profile.game_app_id) {
        (Some(g), _) => cli_println!("Playing  : {g}"),
        (None, Some(id)) => cli_println!("Playing  : app {id}"),
        _ => {}
    }
    if let Some(t) = profile.last_logoff {
        cli_println!(
            "Last seen: {}",
            libgaki::steam_client::unix_to_ymd(t as i64)
        );
    }
    if let Some(h) = &profile.headline {
        cli_println!("Headline : {h}");
    }
    if let Some(s) = &profile.summary {
        cli_println!("\n{}", libgaki::web::store::strip_html(s));
    }
    if let Some(t) = profile.ban_expires {
        cli_println!(
            "\nCommunity ban until {}",
            libgaki::steam_client::unix_to_ymd(t as i64)
        );
    }
    Ok(())
}

/// `gaki-cli friends search IDENT`: same as `user`.
pub(crate) async fn cmd_friends_search(query: String) -> Result<()> {
    cmd_user(query).await
}

/// `gaki-cli friends add IDENT`: send a friend request.
pub(crate) async fn cmd_friends_add(query: String) -> Result<()> {
    let client = authed_client().await?;
    let steam_id = resolve_user_ident(&client, &query).await?;
    let added = social::add_friend(&client, steam_id).await?;
    let name = added.persona_name;

    match &name {
        Some(n) => cli_println!("Friend request sent to {n} ({}).", added.steam_id),
        None => cli_println!("Friend request sent to {}.", added.steam_id),
    }

    Ok(())
}

/// `gaki-cli friends remove <steamid>`: remove a friend or cancel a pending request.
pub(crate) async fn cmd_friends_remove(steamid: u64) -> Result<()> {
    let client = authed_client().await?;
    social::remove_friend(&client, steamid).await?;
    cli_println!("Removed {steamid} from your friends.");

    Ok(())
}

/// `gaki-cli chat send <steamid> <message>`: send a direct message to a friend.
pub(crate) async fn cmd_chat_send(steamid: u64, message: String) -> Result<()> {
    let client = authed_client().await?;
    social::send_message(&client, steamid, message).await?;
    cli_println!("Message sent to {steamid}.");

    Ok(())
}

/// `gaki-cli chat history <steamid>`: show recent messages with a friend.
pub(crate) async fn cmd_chat_history(steamid: u64, count: u32) -> Result<()> {
    let client = authed_client().await?;
    let messages = social::history(&client, steamid, count).await?;
    if messages.is_empty() {
        cli_println!("No messages with {steamid}.");
        return Ok(());
    }
    for m in &messages {
        let who = if m.from_self { "me" } else { "them" };
        cli_println!("[{}] {:>4}: {}", m.timestamp, who, m.message);
    }
    Ok(())
}

/// Render one incoming chat event for `gaki-cli chat open`, filtered to the
/// conversation with `target` as one friendly line.
pub(crate) fn render_chat_event(target: u64, event: gaki_steam_vent::chat::ChatEvent) {
    use gaki_steam_vent::chat::ChatEvent;
    // Every event carries the conversation partner's SteamID; ignore events for
    // other conversations sharing this connection's notification stream.
    if social::event_partner(&event) != Some(target) {
        return;
    }
    match event {
        ChatEvent::Message(e) => {
            let text = e.message_no_bbcode.unwrap_or(e.message);
            cli_println!("them: {text}");
        }
        ChatEvent::EchoMessage(e) => {
            let text = e.message_no_bbcode.unwrap_or(e.message);
            cli_println!("me (sent elsewhere): {text}");
        }
        ChatEvent::Typing(_) => cli_println!("* {target} is typing..."),
        ChatEvent::GameInvite(_) => cli_println!("* {target} sent a game invite"),
        // Other entry types (and kinds added later) carry nothing to show.
        _ => {}
    }
}

/// `gaki-cli chat open <steamid>`: interactive live chat. Incoming messages stream
/// to stdout while lines read from stdin are sent to the friend; the session ends
/// on stdin EOF (Ctrl-D / Ctrl-Z) or when the notification stream closes.
///
/// Runs naturally over the daemon — the thin client streams stdin and relays
/// stdout in real time — so it reuses the shared, already-online connection.
pub(crate) async fn cmd_chat_open(steamid: u64) -> Result<()> {
    use tokio_stream::StreamExt;

    let client = authed_client().await?;
    // Online, so Steam delivers incoming messages.
    let chat = social::open_chat(&client).await?;
    let mut events = chat.listen();

    cli_println!(
        "Chat with {steamid}. Type a message and press Enter to send; Ctrl-D (Ctrl-Z on \
         Windows) to quit."
    );

    loop {
        tokio::select! {
            incoming = events.next() => {
                match incoming {
                    Some(Ok(event)) => render_chat_event(steamid, event),
                    Some(Err(_)) => continue, // dropped/lagged notification — keep going
                    None => break,            // connection closed
                }
            }
            line = output::read_line_opt() => {
                match line {
                    Ok(Some(text)) => {
                        let text = text.trim();
                        if text.is_empty() {
                            continue;
                        }
                        if let Err(e) = social::send_message(&client, steamid, text.to_string()).await {
                            cli_eprintln!("send failed: {e:#}");

                        }
                    }
                    Ok(None) => break, // stdin closed
                    Err(_) => break,
                }
            }
        }
    }

    Ok(())
}
