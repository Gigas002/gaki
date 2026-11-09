use crate::base::{
    DecodableMessage, EncodableMessage, NetMessageHeader, ReceivableMessage, SendableMessage,
    ServiceMethodRequest, ServiceNotification,
};
use crate::net::read_u32_le;
use crate::net::{NetworkError, RawNetMessage};
use bytes::{Buf, BytesMut};
use flate2::read::GzDecoder;
use futures::{
    StreamExt,
    future::ready,
    stream::{iter, once},
};
use gaki_steam_vent_proto::enums_clientserver::EMsg;
use gaki_steam_vent_proto::protobuf::Message;
use gaki_steam_vent_proto::steammessages_base::CMsgMulti;
use gaki_steam_vent_proto::{MsgKind, RpcMessage};
use std::io::Error as IoError;
use std::io::{Cursor, Read, Write};
use std::{fmt::Debug, io::ErrorKind};
use thiserror::Error;
use tokio_stream::Stream;
use tracing::{debug, trace};

/// Malformed message body
#[derive(Error, Debug)]
#[error("Malformed message body for {0:?}: {1}")]
pub struct MalformedBody(MsgKind, MessageParseError);

impl MalformedBody {
    pub fn new<K: Into<MsgKind>>(kind: K, err: impl Into<MessageParseError>) -> Self {
        MalformedBody(kind.into(), err.into())
    }
}

/// Error while parsing the message body
#[derive(Error, Debug)]
#[non_exhaustive]
#[error("{error:#}")]
pub struct MessageParseError {
    #[from]
    error: IoError,
}

enum MaybeZipReader {
    Raw(Cursor<Vec<u8>>),
    Zipped(Box<GzDecoder<Cursor<Vec<u8>>>>),
}

impl Read for MaybeZipReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            MaybeZipReader::Raw(raw) => raw.read(buf),
            MaybeZipReader::Zipped(zipped) => zipped.read(buf),
        }
    }
}

/// Flatten any "multi" messages in a stream of raw messages
pub(crate) fn flatten_multi<S: Stream<Item = Result<RawNetMessage, NetworkError>>>(
    source: S,
) -> impl Stream<Item = Result<RawNetMessage, NetworkError>> {
    source.flat_map(|res| match res {
        Ok(next) if next.kind == EMsg::k_EMsgMulti => {
            let reader = Cursor::new(next.data);
            let multi = match MultiBodyIter::new(reader) {
                Err(e) => return once(ready(Err(e.into()))).right_stream(),
                Ok(iter) => iter,
            };
            iter(multi).left_stream()
        }
        res => once(ready(res)).right_stream(),
    })
}

struct MultiBodyIter<R> {
    reader: R,
}

impl MultiBodyIter<MaybeZipReader> {
    pub fn new<R: Read>(mut reader: R) -> Result<Self, MalformedBody> {
        let mut multi = CMsgMulti::parse_from_reader(&mut reader)
            .map_err(|e| MalformedBody::new(EMsg::k_EMsgMulti, IoError::from(e)))?;

        let data = match multi.size_unzipped() {
            0 => MaybeZipReader::Raw(Cursor::new(multi.take_message_body())),
            _ => MaybeZipReader::Zipped(Box::new(GzDecoder::new(Cursor::new(
                multi.take_message_body(),
            )))),
        };

        Ok(MultiBodyIter { reader: data })
    }
}

impl<R: Read> Iterator for MultiBodyIter<R> {
    type Item = Result<RawNetMessage, NetworkError>;

    fn next(&mut self) -> Option<Self::Item> {
        let size = read_u32_le(&mut self.reader).ok()?;

        let mut msg_data = BytesMut::with_capacity(size as usize);
        msg_data.resize(size as usize, 0);
        if let Err(e) = self.reader.read_exact(&mut msg_data) {
            return Some(Err(NetworkError::IO(e)));
        }
        let raw = match RawNetMessage::read(msg_data) {
            Ok(raw) => raw,
            Err(e) => return Some(Err(e)),
        };

        debug!("Reading child message {:?}", raw.kind);

        Some(Ok(raw))
    }
}

#[derive(Debug)]
pub(crate) struct ServiceMethodMessage<Request: Debug>(pub Request);

impl<Request: ServiceMethodRequest + Debug> EncodableMessage for ServiceMethodMessage<Request> {
    fn write_body<W: Write>(&self, mut writer: W) -> Result<(), IoError> {
        trace!("writing body of protobuf message {:?}", self.kind());
        self.0
            .write(&mut writer)
            .map_err(|_| IoError::from(ErrorKind::InvalidData))
    }

    fn encode_size(&self) -> usize {
        self.0.encode_size()
    }

    fn process_header(&self, header: &mut NetMessageHeader) {
        header.target_job_name = Some(Request::REQ_NAME.into())
    }
}

impl<Request: ServiceMethodRequest + Debug> SendableMessage for ServiceMethodMessage<Request> {
    type KindEnum = EMsg;

    fn kind(&self) -> Self::KindEnum {
        EMsg::k_EMsgServiceMethodCallFromClient
    }

    fn is_protobuf(&self) -> bool {
        true
    }
}

#[derive(Debug)]
pub(crate) struct ServiceMethodResponseMessage {
    job_name: String,
    body: BytesMut,
}

impl ServiceMethodResponseMessage {
    pub fn into_response<Request: ServiceMethodRequest>(
        self,
    ) -> Result<Request::Response, NetworkError> {
        if self.job_name == Request::REQ_NAME {
            Ok(Request::Response::parse(&mut self.body.reader())
                .map_err(|e| MalformedBody::new(Self::KIND, IoError::from(e)))?)
        } else {
            Err(NetworkError::DifferentServiceMethod(
                Request::REQ_NAME,
                self.job_name,
            ))
        }
    }
}

impl DecodableMessage for ServiceMethodResponseMessage {
    fn read_body(data: BytesMut, header: &NetMessageHeader) -> Result<Self, IoError> {
        trace!("reading body of protobuf message {:?}", Self::KIND);
        Ok(ServiceMethodResponseMessage {
            job_name: header
                .target_job_name
                .as_deref()
                .unwrap_or_default()
                .to_string(),
            body: data,
        })
    }
}

impl ReceivableMessage for ServiceMethodResponseMessage {
    type KindEnum = EMsg;
    const KIND: Self::KindEnum = EMsg::k_EMsgServiceMethodResponse;
    const IS_PROTOBUF: bool = true;
}

#[derive(Debug, Clone)]
pub(crate) struct ServiceMethodNotification {
    pub(crate) job_name: String,
    body: BytesMut,
}

impl ServiceMethodNotification {
    pub fn into_notification<Request: ServiceNotification>(self) -> Result<Request, NetworkError> {
        if self.job_name == Request::NOTIFICATION_NAME {
            Ok(Request::parse(&mut self.body.reader())
                .map_err(|e| MalformedBody::new(Self::KIND, e))?)
        } else {
            Err(NetworkError::DifferentServiceMethod(
                Request::NOTIFICATION_NAME,
                self.job_name,
            ))
        }
    }
}

impl DecodableMessage for ServiceMethodNotification {
    fn read_body(data: BytesMut, header: &NetMessageHeader) -> Result<Self, IoError> {
        trace!("reading body of protobuf message {:?}", Self::KIND);
        Ok(ServiceMethodNotification {
            job_name: header
                .target_job_name
                .as_deref()
                .unwrap_or_default()
                .to_string(),
            body: data,
        })
    }
}

impl ReceivableMessage for ServiceMethodNotification {
    type KindEnum = EMsg;
    const KIND: Self::KindEnum = EMsg::k_EMsgServiceMethod;
    const IS_PROTOBUF: bool = true;
}

#[cfg(test)]
mod tests;
