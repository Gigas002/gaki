//! Friends, profiles and chat.

use crate::steam_client::{
    AddedFriend, ChatMessage, Friend, SentMessage, SteamClient, UserProfile,
};
use anyhow::Result;
use std::time::Duration;

/// How long a fresh connection gathers friend presence before reporting.
pub const FRIENDS_WAIT: Duration = Duration::from_secs(3);

/// Steam's relationship code for an accepted friend.
const RELATIONSHIP_FRIEND: u32 = 3;

/// The account's friends with their presence, gathered over `wait` on this
/// connection. Best-effort: presence still arriving after `wait` is missed.
/// A long-lived session (the daemon) should keep a live roster instead.
pub async fn friends(client: &SteamClient, wait: Duration) -> Result<Vec<Friend>> {
    let mut all = client.collect_friends(wait).await?;
    all.retain(is_friend);
    Ok(all)
}

/// Whether a roster entry is an accepted friend (not a pending request or a
/// blocked user).
pub fn is_friend(friend: &Friend) -> bool {
    friend.relationship == RELATIONSHIP_FRIEND
}

/// Label for a raw `EPersonaState`.
pub fn persona_state_label(state: Option<u32>) -> &'static str {
    match state {
        Some(0) | None => "offline",
        Some(1) => "online",
        Some(2) => "busy",
        Some(3) => "away",
        Some(4) => "snooze",
        Some(5) => "looking to trade",
        Some(6) => "looking to play",
        Some(_) => "online",
    }
}

/// Label for a profile's visibility.
pub fn visibility_label(visibility: i32) -> &'static str {
    match visibility {
        3 => "public",
        2 => "friends-only",
        1 => "private",
        _ => "unknown",
    }
}

/// A user's public profile.
pub async fn profile(client: &SteamClient, steam_id: u64) -> Result<UserProfile> {
    client.user_profile(steam_id).await
}

/// Send a friend request.
pub async fn add_friend(client: &SteamClient, steam_id: u64) -> Result<AddedFriend> {
    client.add_friend(steam_id).await
}

/// Remove a friend, or cancel a pending request.
pub async fn remove_friend(client: &SteamClient, steam_id: u64) -> Result<()> {
    client.remove_friend(steam_id).await
}

/// Send a direct message.
pub async fn send_message(
    client: &SteamClient,
    steam_id: u64,
    text: String,
) -> Result<SentMessage> {
    client.send_chat_message(steam_id, text).await
}

/// The most recent messages with a friend.
pub async fn history(client: &SteamClient, steam_id: u64, count: u32) -> Result<Vec<ChatMessage>> {
    client.recent_chat_messages(steam_id, count).await
}

/// Go online (so Steam delivers messages) and open the chat notification
/// stream. Presence failures are logged, not fatal.
pub async fn open_chat(client: &SteamClient) -> Result<steam_vent_chat::ChatClient> {
    if let Err(e) = client.announce_configured_presence().await {
        tracing::warn!("could not announce presence: {e:#}");
    }
    client.chat_client()
}

/// The conversation partner an incoming chat event belongs to, or `None`
/// for a kind of event this version doesn't know.
pub fn event_partner(event: &steam_vent_chat::ChatEvent) -> Option<u64> {
    use steam_vent_chat::ChatEvent;
    match event {
        ChatEvent::Message(e) | ChatEvent::EchoMessage(e) | ChatEvent::GameInvite(e) => {
            Some(u64::from(e.source))
        }
        ChatEvent::Typing(e) => Some(u64::from(e.source)),
        ChatEvent::Other(e) => Some(u64::from(e.source)),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
