use crate::base::{
    EncodableMessage, JobId, NetMessageHeader, RawSteamId, ReceivableMessage, SendableMessage,
};
use crate::eresult::EResult;
use crate::message::MalformedBody;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use gaki_steam_vent_proto::enums_clientserver::EMsg;
use gaki_steam_vent_proto::protobuf::Message;
use gaki_steam_vent_proto::steammessages_base::CMsgProtoBufHeader;
use gaki_steam_vent_proto::{MsgKind, MsgKindEnum};
use std::fmt::Debug;
use std::io::Error as IoError;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use thiserror::Error;
use tracing::{debug, trace};

pub const PROTO_MASK: u32 = 0x80000000;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum NetworkError {
    #[error("{0}")]
    IO(#[from] IoError),
    #[error("{0}")]
    Ws(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("Invalid message header")]
    InvalidHeader,
    #[error("Invalid message kind {0}")]
    InvalidMessageKind(i32),
    #[error("Different message expected, expected {0:?}, got {1:?}")]
    DifferentMessage(MsgKind, MsgKind),
    #[error("Different service method expected, expected {0:?}, got {1:?}")]
    DifferentServiceMethod(&'static str, String),
    #[error("{0}")]
    MalformedBody(#[from] MalformedBody),
    #[error("Unexpected end of stream")]
    EOF,
    #[error("Response timed out")]
    Timeout,
    #[error("Remote returned an error code: {0:?}")]
    ApiError(EResult),
}

impl From<EResult> for NetworkError {
    fn from(value: EResult) -> Self {
        NetworkError::ApiError(value)
    }
}

pub type Result<T, E = NetworkError> = std::result::Result<T, E>;

fn parse_proto_header(header: CMsgProtoBufHeader) -> NetMessageHeader {
    NetMessageHeader {
        source_job_id: JobId::new(header.jobid_source()),
        target_job_id: JobId::new(header.jobid_target()),
        steam_id: RawSteamId::new(header.steamid()),
        session_id: header.client_sessionid(),
        target_job_name: header
            .has_target_job_name()
            .then(|| header.target_job_name().to_string().into()),
        result: header.eresult,
        source_app_id: header.routing_appid,
    }
}

fn proto_header(header: &NetMessageHeader, kind: MsgKind) -> CMsgProtoBufHeader {
    let mut proto_header = CMsgProtoBufHeader::new();
    if header.source_job_id != JobId::NONE {
        proto_header.set_jobid_source(header.source_job_id.id());
    }
    if header.target_job_id != JobId::NONE {
        proto_header.set_jobid_target(header.target_job_id.id());
    }
    if header.steam_id != RawSteamId::NONE {
        proto_header.set_steamid(
            if kind == EMsg::k_EMsgServiceMethodCallFromClientNonAuthed {
                0
            } else {
                header.steam_id.id()
            },
        );
    }
    if header.session_id != 0 {
        proto_header.set_client_sessionid(header.session_id);
    }
    if kind == EMsg::k_EMsgServiceMethodCallFromClientNonAuthed
        || kind == EMsg::k_EMsgServiceMethodCallFromClient
    {
        proto_header.set_realm(1);
    }
    if let Some(target_job_name) = header.target_job_name.as_deref() {
        proto_header.set_target_job_name(target_job_name.into());
    }
    proto_header.routing_appid = header.source_app_id;
    proto_header.eresult = header.result;
    proto_header
}

/// Kind, then the non-protobuf "extended" header: size (1), version (2),
/// target job (8), source job (8), canary (1), steam id (8), session id (4).
const EXTENDED_HEADER_LEN: usize = 4 + 1 + 2 + 8 + 8 + 1 + 8 + 4;

pub(crate) fn read_u32_le(reader: &mut impl Read) -> std::io::Result<u32> {
    let mut buf = [0; 4];
    reader.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

fn read_u64_le(reader: &mut impl Read) -> std::io::Result<u64> {
    let mut buf = [0; 8];
    reader.read_exact(&mut buf)?;
    Ok(u64::from_le_bytes(buf))
}

fn read_i32_le(reader: &mut impl Read) -> std::io::Result<i32> {
    read_u32_le(reader).map(|v| v as i32)
}

pub(crate) fn read_header<R: Read + Seek>(
    mut reader: R,
    is_protobuf: bool,
) -> Result<(NetMessageHeader, usize)> {
    if is_protobuf {
        let header_length = read_u32_le(&mut reader)?;
        trace!("reading protobuf header of {} bytes", header_length);
        let header = if header_length > 0 {
            // `read_exact`: a header cut short is an error, not a smaller header.
            let mut bytes = vec![0; header_length as usize];
            reader.read_exact(&mut bytes)?;
            parse_proto_header(
                CMsgProtoBufHeader::parse_from_bytes(&bytes)
                    .map_err(|_| NetworkError::InvalidHeader)?,
            )
        } else {
            NetMessageHeader::default()
        };
        Ok((header, 8 + header_length as usize))
    } else {
        reader.seek(SeekFrom::Current(3))?; // 1 byte (fixed) header size, 2 bytes (fixed) header version
        let target_job_id = read_u64_le(&mut reader)?;
        let source_job_id = read_u64_le(&mut reader)?;
        reader.seek(SeekFrom::Current(1))?; // header canary (fixed)
        let steam_id = RawSteamId::new(read_u64_le(&mut reader)?);
        let session_id = read_i32_le(&mut reader)?;
        Ok((
            NetMessageHeader {
                source_job_id: JobId::new(source_job_id),
                target_job_id: JobId::new(target_job_id),
                steam_id,
                session_id,
                target_job_name: None,
                result: None,
                source_app_id: None,
            },
            EXTENDED_HEADER_LEN,
        ))
    }
}

pub(crate) fn write_header<W: Write, K: MsgKindEnum>(
    header: &NetMessageHeader,
    writer: &mut W,
    kind: K,
    proto: bool,
) -> std::io::Result<()> {
    if proto {
        trace!(
            "writing header for {:?} protobuf message: {:?}",
            kind, header
        );
        let proto_header = proto_header(header, kind.into());
        writer.write_all(&kind.encode_kind(true).to_le_bytes())?;
        writer.write_all(&(proto_header.compute_size() as u32).to_le_bytes())?;
        proto_header.write_to_writer(writer)?;
    } else {
        trace!("writing header for {:?} message: {:?}", kind, header);
        writer.write_all(&(kind.value() as u32).to_le_bytes())?;
        writer.write_all(&[32])?; // header size
        writer.write_all(&2u16.to_le_bytes())?; // header version
        writer.write_all(&header.target_job_id.id().to_le_bytes())?;
        writer.write_all(&header.source_job_id.id().to_le_bytes())?;
        writer.write_all(&[239])?; // header canary
        writer.write_all(&header.steam_id.id().to_le_bytes())?;
        writer.write_all(&header.session_id.to_le_bytes())?;
    }
    Ok(())
}

pub(crate) fn header_encode_size(header: &NetMessageHeader, kind: MsgKind, proto: bool) -> usize {
    if proto {
        let proto_header = proto_header(header, kind);
        4 + 4 + proto_header.compute_size() as usize
    } else {
        EXTENDED_HEADER_LEN
    }
}

#[derive(Debug, Clone)]
pub struct RawNetMessage {
    pub kind: MsgKind,
    pub is_protobuf: bool,
    pub header: NetMessageHeader,
    pub data: BytesMut,
    pub(crate) header_buffer: BytesMut,
}

pub(crate) fn decode_kind(kind: u32) -> (MsgKind, bool) {
    let is_protobuf = kind & PROTO_MASK == PROTO_MASK;
    let kind = MsgKind((kind & !PROTO_MASK) as i32);
    (kind, is_protobuf)
}

impl RawNetMessage {
    pub fn read<Body: Into<Bytes>>(body: Body) -> Result<Self> {
        let mut value = BytesMut::from(body.into());
        let mut reader = Cursor::new(&value);
        let kind = read_u32_le(&mut reader).map_err(|_| NetworkError::InvalidHeader)?;
        let (kind, is_protobuf) = decode_kind(kind);

        trace!(
            "reading header for {:?} {}message",
            kind,
            if is_protobuf { "protobuf " } else { "" }
        );

        let header_start = reader.position() as usize;
        let (header, body_start) = read_header(&mut reader, is_protobuf)?;

        value.advance(header_start);
        let header_buffer = value.split_to(body_start - header_start);

        Ok(RawNetMessage {
            kind,
            is_protobuf,
            header,
            data: value,
            header_buffer,
        })
    }

    pub fn from_message<T: SendableMessage>(header: NetMessageHeader, message: T) -> Result<Self> {
        let kind = message.kind();
        let is_protobuf = message.is_protobuf();
        Self::from_message_with_kind(header, message, kind, is_protobuf)
    }

    pub fn from_message_with_kind<T: EncodableMessage, K: MsgKindEnum>(
        mut header: NetMessageHeader,
        message: T,
        kind: K,
        is_protobuf: bool,
    ) -> Result<Self> {
        debug!("writing raw {:?} message", kind);

        message.process_header(&mut header);

        let body_size = message.encode_size();

        let mut buff = BytesMut::with_capacity(
            header_encode_size(&header, kind.into(), is_protobuf) + body_size,
        );

        {
            let mut writer = (&mut buff).writer();
            write_header(&header, &mut writer, kind, is_protobuf)?;
        }

        let header_buffer = buff.split();
        let mut writer = (&mut buff).writer();
        message.write_body(&mut writer)?;
        trace!("encoded body({} bytes): {:x?}", buff.len(), buff.as_ref());

        Ok(RawNetMessage {
            kind: kind.into(),
            is_protobuf,
            header,
            data: buff,
            header_buffer,
        })
    }

    /// Return a buffer containing the raw message bytes
    pub fn into_bytes(self) -> BytesMut {
        let mut body = self.header_buffer;
        body.unsplit(self.data);
        body
    }
}

impl RawNetMessage {
    pub fn into_header_and_message<T: ReceivableMessage>(self) -> Result<(NetMessageHeader, T)> {
        if let Some(result) = self.header.result {
            EResult::from_result(result)?;
        }
        if self.kind == T::KIND {
            trace!(
                "reading body of {:?} message({} bytes)",
                self.kind,
                self.data.len()
            );
            let body = T::read_body(self.data, &self.header)
                .map_err(|err| MalformedBody::new(T::KIND, err))?;
            Ok((self.header, body))
        } else {
            Err(NetworkError::DifferentMessage(T::KIND.into(), self.kind))
        }
    }

    pub fn into_message<T: ReceivableMessage>(self) -> Result<T> {
        self.into_header_and_message().map(|(_, msg)| msg)
    }
}

#[cfg(test)]
mod tests;
