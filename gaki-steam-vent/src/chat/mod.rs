// Based on aurelia-steam-vent-chat (GPL-3.0-or-later); see README.md.
//! Friend chat on top of a [`Connection`]: incoming message events and sending.
//!
//! Fork of [aurelia-steam-vent-chat](https://codeberg.org/Drackrath/aurelia-steam-vent-chat).

use crate::{Connection, ConnectionTrait, NetworkError};
use gaki_steam_vent_proto::steammessages_friendmessages_steamclient::{
    CFriendMessages_IncomingMessage_Notification, CFriendMessages_SendMessage_Request,
};
use steamid_ng::SteamID;
use tokio_stream::{Stream, StreamExt};

/// High level wrapper around steam-vent's [`Connection`] for implementing a chat client
pub struct ChatClient {
    connection: Connection,
}

impl ChatClient {
    /// Create a chat client from a [`Connection`]
    pub fn new(connection: Connection) -> Self {
        ChatClient { connection }
    }

    /// Listen for incoming events
    pub fn listen(&self) -> impl Stream<Item = Result<ChatEvent, NetworkError>> + '_ {
        self.connection
            .on_notification::<CFriendMessages_IncomingMessage_Notification>()
            .filter_map(|notification| {
                notification
                    .map(|notification| ChatEvent::try_from(notification).ok())
                    .transpose()
            })
    }

    /// Send a chat message to a user.
    ///
    /// Flags text as BBCode, like Steam.
    pub async fn send_message(
        &self,
        target: SteamID,
        message: String,
    ) -> Result<MessageResult, NetworkError> {
        let req = CFriendMessages_SendMessage_Request {
            steamid: Some(target.into()),
            message: Some(message),
            chat_entry_type: Some(MessageType::Chat as i32),
            contains_bbcode: Some(true),
            ..CFriendMessages_SendMessage_Request::default()
        };
        let result = self.connection.service_method(req).await?;

        Ok(MessageResult {
            server_timestamp: result.server_timestamp() as u64,
            modified_message: result.modified_message,
        })
    }
}

/// Incoming chat event
#[derive(Debug)]
#[non_exhaustive]
pub enum ChatEvent {
    /// Another user sent a message
    Message(MessageEvent),
    /// The local user sent a message from another device
    EchoMessage(MessageEvent),
    /// Another user is typing
    Typing(TypingEvent),
    /// Another user sent a game invite
    GameInvite(MessageEvent),
    /// Any other entry type
    Other(OtherEvent),
}

impl TryFrom<CFriendMessages_IncomingMessage_Notification> for ChatEvent {
    type Error = ();

    fn try_from(
        notification: CFriendMessages_IncomingMessage_Notification,
    ) -> Result<Self, Self::Error> {
        // steamid-ng 3.x dropped `Default` for `SteamID`, so an unparseable id can
        // no longer fall back to a zero id — drop the event instead.
        let source = SteamID::try_from(notification.steamid_friend()).map_err(|_| ())?;
        let message_type =
            MessageType::try_from(notification.chat_entry_type()).unwrap_or_default();
        Ok(match message_type {
            // A message from the friend (not a local echo of our own send). The
            // upstream code guarded both arms on `local_echo()`, which made the
            // echo arm unreachable and silently dropped every genuine incoming
            // message; the real distinction is whether `local_echo` is set.
            MessageType::Chat if !notification.local_echo() => ChatEvent::Message(MessageEvent {
                source,
                server_timestamp: notification.rtime32_server_timestamp() as u64,
                message: notification.message.unwrap_or_default(),
                message_no_bbcode: notification.message_no_bbcode,
            }),
            MessageType::Chat => ChatEvent::EchoMessage(MessageEvent {
                source,
                server_timestamp: notification.rtime32_server_timestamp() as u64,
                message: notification.message.unwrap_or_default(),
                message_no_bbcode: notification.message_no_bbcode,
            }),
            // Own typing elsewhere is noise.
            MessageType::Typing if notification.local_echo() => return Err(()),
            MessageType::Typing => ChatEvent::Typing(TypingEvent {
                source,
                server_timestamp: notification.rtime32_server_timestamp() as u64,
            }),
            MessageType::GameInvite if !notification.local_echo() => {
                ChatEvent::GameInvite(MessageEvent {
                    source,
                    server_timestamp: notification.rtime32_server_timestamp() as u64,
                    message: notification.message.unwrap_or_default(),
                    message_no_bbcode: notification.message_no_bbcode,
                })
            }
            _ => ChatEvent::Other(OtherEvent {
                source,
                entry_type: notification.chat_entry_type(),
                local_echo: notification.local_echo(),
                server_timestamp: notification.rtime32_server_timestamp() as u64,
                message: notification.message,
            }),
        })
    }
}

/// Incoming chat message
#[derive(Debug)]
pub struct MessageEvent {
    /// SteamID of the sender
    pub source: SteamID,
    /// Raw message contents
    pub message: String,
    /// Message contents without any bbcode markup
    pub message_no_bbcode: Option<String>,
    /// Service side time when the message was sent
    pub server_timestamp: u64,
}

/// Entry type without its own variant
#[derive(Debug)]
pub struct OtherEvent {
    /// SteamID of the conversation partner
    pub source: SteamID,
    /// Raw `EChatEntryType` value
    pub entry_type: i32,
    /// Sent by the local user elsewhere
    pub local_echo: bool,
    /// Service side time of the event
    pub server_timestamp: u64,
    /// Raw message contents, if any
    pub message: Option<String>,
}

/// Incoming typing event
#[derive(Debug)]
pub struct TypingEvent {
    /// SteamID of the typer
    pub source: SteamID,
    /// Service side time when the user was typing
    pub server_timestamp: u64,
}

#[repr(i32)]
#[derive(Default)]
enum MessageType {
    #[default]
    Invalid,
    Chat = 1,
    Typing,
    GameInvite,
    Left = 6,
    Entered,
    Kicked,
    Banned,
    Disconnected,
    Historical,
    LinkBlocked = 14,
}

impl TryFrom<i32> for MessageType {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Ok(match value {
            1 => MessageType::Chat,
            2 => MessageType::Typing,
            3 => MessageType::GameInvite,
            6 => MessageType::Left,
            7 => MessageType::Entered,
            8 => MessageType::Kicked,
            9 => MessageType::Banned,
            10 => MessageType::Disconnected,
            11 => MessageType::Historical,
            14 => MessageType::LinkBlocked,
            _ => return Err(()),
        })
    }
}

/// Result of a sent chat message
#[derive(Debug)]
pub struct MessageResult {
    /// Chat message with bbcode added
    pub modified_message: Option<String>,
    pub server_timestamp: u64,
}

#[cfg(test)]
mod tests;
