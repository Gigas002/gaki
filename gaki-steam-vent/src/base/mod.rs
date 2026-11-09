//! The message and connection traits the rest of the crate builds on.
//!
//! Upstream ships these as the separate `aurelia-steam-vent-core` crate, a
//! small API surface for wrappers to pin; vendored, it is just a module.

mod connection;
mod message;
mod net;
mod service_method;

pub use connection::{ConnectionTrait, ReadonlyConnection};
pub use message::{DecodableMessage, EncodableMessage, ReceivableMessage, SendableMessage};
pub use net::{JobId, NetMessageHeader, RawSteamId};
pub use service_method::{ServiceMethodRequest, ServiceNotification};
