//! What `build.rs` adds on top of plain rust-protobuf output: the kinds,
//! RPC method names and response types gaki-steam-vent routes messages by.

use gaki_steam_vent_proto::enums_clientserver::EMsg;
use gaki_steam_vent_proto::steammessages_auth_steamclient::{
    CAuthentication_BeginAuthSessionViaQR_Request, CAuthentication_BeginAuthSessionViaQR_Response,
};
use gaki_steam_vent_proto::steammessages_clientserver_appinfo::CMsgClientPICSProductInfoResponse;
use gaki_steam_vent_proto::steammessages_clientserver_login::CMsgClientLogon;
use gaki_steam_vent_proto::{
    JobMultiple, MsgKind, MsgKindEnum, PROTO_MASK, RpcMessage, RpcMessageWithKind, RpcMethod,
};
use std::any::TypeId;

#[test]
fn messages_get_their_emsg_kind() {
    assert_eq!(CMsgClientLogon::KIND, EMsg::k_EMsgClientLogon);
    assert_eq!(
        CMsgClientPICSProductInfoResponse::KIND,
        EMsg::k_EMsgClientPICSProductInfoResponse
    );
}

#[test]
fn kind_enums_encode_the_proto_flag() {
    let kind = EMsg::k_EMsgClientLogon;
    assert_eq!(kind.encode_kind(false), 5514);
    assert_eq!(kind.encode_kind(true), 5514 | PROTO_MASK);
    assert_eq!(MsgKind::from(kind), kind);
}

#[test]
fn service_requests_name_their_method_and_response() {
    assert_eq!(
        CAuthentication_BeginAuthSessionViaQR_Request::METHOD_NAME,
        "Authentication.BeginAuthSessionViaQR#1"
    );
    assert_eq!(
        TypeId::of::<<CAuthentication_BeginAuthSessionViaQR_Request as RpcMethod>::Response>(),
        TypeId::of::<CAuthentication_BeginAuthSessionViaQR_Response>()
    );
}

#[test]
fn rpc_message_round_trips() {
    let logon = CMsgClientLogon {
        protocol_version: Some(65580),
        account_name: Some("gaki".into()),
        ..Default::default()
    };
    let mut buf = Vec::new();
    logon.write(&mut buf).unwrap();
    assert_eq!(buf.len(), logon.encode_size());
    let parsed = CMsgClientLogon::parse(&mut buf.as_slice()).unwrap();
    assert_eq!(parsed, logon);
}

#[test]
fn product_info_completes_when_nothing_is_pending() {
    let mut response = CMsgClientPICSProductInfoResponse::default();
    assert!(response.completed());
    response.response_pending = Some(true);
    assert!(!response.completed());
}
