use super::*;
use gaki_steam_vent_proto::steammessages_clientserver_login::CMsgClientHeartBeat;

fn header() -> NetMessageHeader {
    NetMessageHeader {
        source_job_id: JobId::new(7),
        target_job_id: JobId::new(9),
        steam_id: RawSteamId::new(76_561_197_960_287_930),
        session_id: 42,
        ..NetMessageHeader::default()
    }
}

fn round_trip<T: SendableMessage>(header: NetMessageHeader, msg: T) -> RawNetMessage {
    let bytes = RawNetMessage::from_message(header, msg)
        .unwrap()
        .into_bytes();
    RawNetMessage::read(bytes).unwrap()
}

#[test]
fn protobuf_header_round_trips() {
    let mut sent = header();
    sent.target_job_name = Some("Player.GetOwnedGames#1".into());
    sent.source_app_id = Some(730);
    let back = round_trip(sent, CMsgClientHeartBeat::default());
    assert_eq!(back.kind, EMsg::k_EMsgClientHeartBeat);
    assert!(back.is_protobuf);
    assert_eq!(back.header.source_job_id, JobId::new(7));
    assert_eq!(back.header.target_job_id, JobId::new(9));
    assert_eq!(
        back.header.steam_id,
        RawSteamId::new(76_561_197_960_287_930)
    );
    assert_eq!(back.header.session_id, 42);
    assert_eq!(
        back.header.target_job_name.as_deref(),
        Some("Player.GetOwnedGames#1")
    );
    assert_eq!(back.header.source_app_id, Some(730));
}

#[test]
fn extended_header_round_trips() {
    let msg = CMsgClientHeartBeat::default().with_kind(EMsg::k_EMsgClientHeartBeat, false);
    let back = round_trip(header(), msg);
    assert!(!back.is_protobuf);
    assert_eq!(back.header.source_job_id, JobId::new(7));
    assert_eq!(back.header.target_job_id, JobId::new(9));
    assert_eq!(back.header.session_id, 42);
}

#[test]
fn encode_size_matches_what_is_written() {
    for proto in [true, false] {
        let mut written = Vec::new();
        write_header(&header(), &mut written, EMsg::k_EMsgClientHeartBeat, proto).unwrap();
        assert_eq!(
            written.len(),
            header_encode_size(&header(), EMsg::k_EMsgClientHeartBeat.into(), proto),
            "proto = {proto}"
        );
    }
}

#[test]
fn unauthenticated_service_calls_hide_the_steam_id() {
    let proto = proto_header(
        &header(),
        EMsg::k_EMsgServiceMethodCallFromClientNonAuthed.into(),
    );
    assert_eq!(proto.steamid(), 0);
    assert_eq!(proto.realm(), 1);
}

#[test]
fn truncated_protobuf_header_is_an_error() {
    let bytes = RawNetMessage::from_message(header(), CMsgClientHeartBeat::default())
        .unwrap()
        .into_bytes();
    // Kind + length + part of the header.
    let cut = BytesMut::from(&bytes[..10]);
    assert!(RawNetMessage::read(cut).is_err());
    assert!(matches!(
        RawNetMessage::read(BytesMut::from(&b"\x01\x02"[..])),
        Err(NetworkError::InvalidHeader)
    ));
}

#[test]
fn kind_carries_the_protobuf_flag() {
    assert_eq!(decode_kind(5514 | PROTO_MASK), (MsgKind(5514), true));
    assert_eq!(decode_kind(5514), (MsgKind(5514), false));
}

#[test]
fn error_result_in_header_fails_the_message() {
    let mut sent = header();
    sent.result = Some(EResult::InvalidPassword as i32);
    let back = round_trip(sent, CMsgClientHeartBeat::default());
    assert!(matches!(
        back.into_message::<CMsgClientHeartBeat>(),
        Err(NetworkError::ApiError(EResult::InvalidPassword))
    ));
}

#[test]
fn wrong_kind_is_reported() {
    use gaki_steam_vent_proto::steammessages_clientserver_login::CMsgClientLogOff;
    let back = round_trip(header(), CMsgClientHeartBeat::default());
    assert!(matches!(
        back.into_message::<CMsgClientLogOff>(),
        Err(NetworkError::DifferentMessage(..))
    ));
}
