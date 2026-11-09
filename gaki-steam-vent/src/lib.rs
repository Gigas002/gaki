// Based on aurelia-steam-vent (MIT); see README.md and LICENSE-MIT.
//! A client for Steam's CM network: server discovery, login, the encrypted
//! session, and typed RPC/notification messaging over it.
//!
//! Fork of [aurelia-steam-vent] (itself a fork of [steam-vent] with
//! per-session logon IDs). `aurelia-steam-vent-core` lives on as the internal
//! `base` module and `aurelia-steam-vent-chat` as the `chat` module behind the
//! `chat` feature; README.md lists what else changed.
//!
//! [aurelia-steam-vent]: https://codeberg.org/Drackrath/aurelia-steam-vent
//! [steam-vent]: https://codeberg.org/steam-vent/steam-vent

pub mod auth;
mod base;
#[cfg(feature = "chat")]
pub mod chat;
pub mod connection;
mod eresult;
pub mod message;
mod net;
mod serverlist;
mod session;
mod transport;

#[cfg(test)]
mod test_support;

pub use base::{
    ConnectionTrait, NetMessageHeader, ReadonlyConnection, ReceivableMessage, SendableMessage,
    ServiceMethodRequest,
};
pub use connection::Connection;
pub use eresult::EResult;
pub use net::{NetworkError, RawNetMessage};
pub use serverlist::{DiscoverOptions, ServerDiscoveryError, ServerList};
pub use session::{ConnectionError, LoginError};
