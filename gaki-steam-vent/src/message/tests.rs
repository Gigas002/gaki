use super::*;
use flate2::Compression;
use flate2::write::GzEncoder;
use gaki_steam_vent_proto::steammessages_clientserver_login::{
    CMsgClientHeartBeat, CMsgClientLogOff,
};
use gaki_steam_vent_proto::steammessages_player_steamclient::{
    CPlayer_GetOwnedGames_Request, CPlayer_GetOwnedGames_Response,
};

fn frame<T: SendableMessage>(msg: T) -> BytesMut {
    RawNetMessage::from_message(NetMessageHeader::default(), msg)
        .unwrap()
        .into_bytes()
}

/// Two child messages, length-prefixed as in `CMsgMulti.message_body`.
fn multi_body() -> Vec<u8> {
    let mut body = Vec::new();
    for child in [
        frame(CMsgClientHeartBeat::default()),
        frame(CMsgClientLogOff::default()),
    ] {
        body.extend_from_slice(&(child.len() as u32).to_le_bytes());
        body.extend_from_slice(&child);
    }
    body
}

async fn flatten(multi: CMsgMulti) -> Vec<MsgKind> {
    let raw = RawNetMessage::read(frame(multi)).unwrap();
    flatten_multi(futures::stream::iter([Ok(raw)]))
        .map(|res| res.unwrap().kind)
        .collect()
        .await
}

#[tokio::test]
async fn multi_messages_are_split() {
    let multi = CMsgMulti {
        message_body: Some(multi_body()),
        ..CMsgMulti::default()
    };
    assert_eq!(
        flatten(multi).await,
        [
            MsgKind::from(EMsg::k_EMsgClientHeartBeat),
            MsgKind::from(EMsg::k_EMsgClientLogOff)
        ]
    );
}

#[tokio::test]
async fn gzipped_multi_messages_are_split() {
    let body = multi_body();
    let mut gz = GzEncoder::new(Vec::new(), Compression::default());
    gz.write_all(&body).unwrap();
    let multi = CMsgMulti {
        size_unzipped: Some(body.len() as u32),
        message_body: Some(gz.finish().unwrap()),
        ..CMsgMulti::default()
    };
    assert_eq!(flatten(multi).await.len(), 2);
}

#[tokio::test]
async fn other_messages_pass_through() {
    let raw = RawNetMessage::read(frame(CMsgClientHeartBeat::default())).unwrap();
    let out: Vec<_> = flatten_multi(futures::stream::iter([Ok(raw)]))
        .collect()
        .await;
    assert_eq!(out.len(), 1);
}

fn response_named(name: &'static str, body: &impl Message) -> ServiceMethodResponseMessage {
    let header = NetMessageHeader {
        target_job_name: Some(name.into()),
        ..NetMessageHeader::default()
    };
    let data = BytesMut::from(&body.write_to_bytes().unwrap()[..]);
    ServiceMethodResponseMessage::read_body(data, &header).unwrap()
}

#[test]
fn service_response_parses_for_its_method() {
    let body = CPlayer_GetOwnedGames_Response {
        game_count: Some(3),
        ..Default::default()
    };
    let response = response_named("Player.GetOwnedGames#1", &body)
        .into_response::<CPlayer_GetOwnedGames_Request>()
        .unwrap();
    assert_eq!(response.game_count(), 3);
}

#[test]
fn service_response_for_another_method_is_rejected() {
    let body = CPlayer_GetOwnedGames_Response::default();
    let err = response_named("Player.Other#1", &body)
        .into_response::<CPlayer_GetOwnedGames_Request>()
        .unwrap_err();
    assert!(matches!(err, NetworkError::DifferentServiceMethod(..)));
}

#[test]
fn service_request_sets_the_job_name() {
    let mut header = NetMessageHeader::default();
    ServiceMethodMessage(CPlayer_GetOwnedGames_Request::default()).process_header(&mut header);
    assert_eq!(
        header.target_job_name.as_deref(),
        Some("Player.GetOwnedGames#1")
    );
}
