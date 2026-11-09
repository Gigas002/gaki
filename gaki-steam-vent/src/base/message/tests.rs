use super::*;
use gaki_steam_vent_proto::MsgKind;
use gaki_steam_vent_proto::enums_clientserver::EMsg;
use gaki_steam_vent_proto::protobuf::Message;
use gaki_steam_vent_proto::steammessages_clientserver_login::CMsgClientLogon;

fn logon() -> CMsgClientLogon {
    CMsgClientLogon {
        account_name: Some("gaki".into()),
        protocol_version: Some(65580),
        ..CMsgClientLogon::default()
    }
}

#[test]
fn protobuf_messages_encode_as_protobuf() {
    let msg = logon();
    let mut out = Vec::new();
    EncodableMessage::write_body(&msg, &mut out).unwrap();
    assert_eq!(out, msg.write_to_bytes().unwrap());
    assert_eq!(EncodableMessage::encode_size(&msg), out.len());
    assert!(SendableMessage::is_protobuf(&msg));
    assert_eq!(SendableMessage::kind(&msg), EMsg::k_EMsgClientLogon);
}

#[test]
fn protobuf_messages_decode() {
    let bytes = BytesMut::from(&logon().write_to_bytes().unwrap()[..]);
    let back = CMsgClientLogon::read_body(bytes, &NetMessageHeader::default()).unwrap();
    assert_eq!(back, logon());
}

#[test]
fn with_kind_overrides_kind_but_not_body() {
    let msg = logon().with_kind(EMsg::k_EMsgClientHeartBeat, false);
    assert!(MsgKind::from(msg.kind()) == EMsg::k_EMsgClientHeartBeat);
    assert!(!msg.is_protobuf());
    let mut out = Vec::new();
    msg.write_body(&mut out).unwrap();
    assert_eq!(out, logon().write_to_bytes().unwrap());
}
