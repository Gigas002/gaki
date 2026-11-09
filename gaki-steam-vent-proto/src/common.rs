//! The traits generated messages implement, and the message-kind wrapper.

pub use protobuf;
use protobuf::Enum;
use std::fmt::Debug;
use std::io::{Read, Write};

pub trait RpcService {
    const SERVICE_NAME: &'static str;
}

/// A protobuf message for an rpc method with associated response
pub trait RpcMethod: protobuf::Message + RpcMessage {
    const METHOD_NAME: &'static str;
    type Response: RpcMessage;
}

/// A protobuf message send as either request or response
pub trait RpcMessage: Debug + Sized {
    fn parse(_reader: &mut dyn Read) -> protobuf::Result<Self>;

    fn write(&self, _writer: &mut dyn Write) -> protobuf::Result<()>;

    fn encode_size(&self) -> usize;
}

/// An empty message
impl RpcMessage for () {
    fn parse(_reader: &mut dyn Read) -> protobuf::Result<Self> {
        Ok(())
    }

    fn write(&self, _writer: &mut dyn Write) -> protobuf::Result<()> {
        Ok(())
    }

    fn encode_size(&self) -> usize {
        0
    }
}

/// An rpc message with associated kind.
///
/// The associated kind is used to differentiate incoming messages
pub trait RpcMessageWithKind: RpcMessage {
    type KindEnum: MsgKindEnum;
    const KIND: Self::KindEnum;
}

/// A generic wrapper for "kind" constants used by network messages
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct MsgKind(pub i32);

impl MsgKind {
    pub fn value(&self) -> i32 {
        self.0
    }
}

impl From<MsgKind> for i32 {
    fn from(value: MsgKind) -> Self {
        value.0
    }
}

pub const PROTO_MASK: u32 = 0x80000000;

/// An enum containing "kind" constants used by the network messages
///
/// Though it is possible to use the generic [`MsgKind`] struct. Applications shipping their own protobufs
/// are encouraged to create their own enums containing the constants in use for ease of use.
pub trait MsgKindEnum: Enum + Debug {
    fn enum_value(&self) -> i32 {
        <Self as Enum>::value(self)
    }

    fn encode_kind(&self, is_protobuf: bool) -> u32 {
        if is_protobuf {
            self.enum_value() as u32 | PROTO_MASK
        } else {
            self.enum_value() as u32
        }
    }
}

impl<T: MsgKindEnum> From<T> for MsgKind {
    fn from(value: T) -> Self {
        MsgKind(value.enum_value())
    }
}

impl<T: MsgKindEnum> PartialEq<T> for MsgKind {
    fn eq(&self, other: &T) -> bool {
        self.0.eq(&other.enum_value())
    }
}

/// Trait for implementing a check for completion of a job that returns a response stream
pub trait JobMultiple {
    /// If the job has completed
    fn completed(&self) -> bool;
}
