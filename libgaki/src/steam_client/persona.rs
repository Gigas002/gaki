//! Announcing the account's own persona and asking for persona data. Login
//! needs this to learn the display name, so it isn't part of `social`.
use super::*;
use gaki_steam_vent_proto::steammessages_clientserver_friends::{
    CMsgClientChangeStatus, CMsgClientRequestFriendData,
};

/// EClientPersonaStateFlag bits to request when asking Steam for friend data:
/// Status(1) | PlayerName(2) | Presence(16) | GameExtraInfo(256) |
/// GameDataBlob(512) = 787. These cover online status, display name, and the
/// currently-played game.
pub const PERSONA_REQUEST_FLAGS: u32 = 1 | 2 | 16 | 256 | 512;

impl SteamClient {
    /// Announce an online persona so Steam starts delivering friend persona data
    /// (display names, status, current game) and incoming chat. A refresh-token
    /// logon is "offline" by default, and Steam withholds friend persona state —
    /// and friend messages — until the client declares a persona. `persona_state`
    /// is a raw EPersonaState (1 = online, 7 = invisible); `need_persona_response`
    /// asks Steam to push the friends' persona state in reply.
    pub async fn announce_persona(&self, persona_state: u32) -> Result<()> {
        let connection = self.require_connection()?;
        let mut status = CMsgClientChangeStatus::new();
        status.set_persona_state(persona_state);
        status.set_need_persona_response(true);
        connection
            .send(status)
            .await
            .map_err(|e| anyhow!("failed to announce persona status: {e}"))?;
        Ok(())
    }

    /// Announce the presence configured in `LauncherConfig::chat_presence`
    /// (defaults to invisible/"offline"). Falls back to invisible if the config
    /// can't be read. Public so an interactive chat session can announce presence
    /// when running standalone (the daemon's friends watcher already does so).
    pub async fn announce_configured_presence(&self) -> Result<()> {
        let persona_state = crate::core::config::load_launcher_config()
            .await
            .map(|c| c.chat_presence.persona_state())
            .unwrap_or(7);
        self.announce_persona(persona_state).await
    }

    /// Ask Steam to push persona state for the given friends. Best-effort and
    /// fire-and-forget: the responses arrive asynchronously as
    /// `CMsgClientPersonaState` messages. A no-op for an empty id list.
    pub async fn request_friend_data(&self, ids: &[u64]) -> Result<()> {
        let connection = self.require_connection()?;
        if ids.is_empty() {
            return Ok(());
        }
        let mut req = CMsgClientRequestFriendData::new();
        req.set_persona_state_requested(PERSONA_REQUEST_FLAGS);
        req.friends = ids.to_vec();
        connection
            .send(req)
            .await
            .map_err(|e| anyhow!("failed to request friend data: {e}"))?;
        Ok(())
    }
}
