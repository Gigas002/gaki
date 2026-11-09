use super::*;
use crate::base::{EncodableMessage, NetMessageHeader, SendableMessage};
use crate::message::ServiceMethodMessage;
use gaki_steam_vent_proto::steammessages_clientserver_login::{
    CMsgClientHeartBeat, CMsgClientLogOff,
};
use gaki_steam_vent_proto::steammessages_friendmessages_steamclient::CFriendMessages_IncomingMessage_Notification;
use std::time::Duration;

fn raw<T: SendableMessage>(header: NetMessageHeader, msg: T) -> RawNetMessage {
    let bytes = RawNetMessage::from_message(header, msg)
        .unwrap()
        .into_bytes();
    RawNetMessage::read(bytes).unwrap()
}

fn to_job(id: u64) -> NetMessageHeader {
    NetMessageHeader {
        target_job_id: JobId::new(id),
        ..NetMessageHeader::default()
    }
}

/// A filter fed from a channel the test controls.
fn filter() -> (MessageFilter, mpsc::UnboundedSender<RawNetMessage>) {
    let (tx, rx) = mpsc::unbounded_channel();
    let source = tokio_stream::wrappers::UnboundedReceiverStream::new(rx).map(Ok);
    (MessageFilter::new(source), tx)
}

async fn settle() {
    tokio::time::sleep(Duration::from_millis(20)).await;
}

#[tokio::test]
async fn job_replies_go_to_their_job() {
    let (filter, input) = filter();
    let reply = filter.on_job_id(JobId::new(5));
    input
        .send(raw(to_job(5), CMsgClientHeartBeat::default()))
        .unwrap();
    assert_eq!(reply.await.unwrap().header.target_job_id, JobId::new(5));
}

#[tokio::test]
async fn multi_job_replies_stream_until_completed() {
    let (filter, input) = filter();
    let mut replies = filter.on_job_id_multi(JobId::new(6));
    for _ in 0..2 {
        input
            .send(raw(to_job(6), CMsgClientHeartBeat::default()))
            .unwrap();
    }
    assert!(replies.recv().await.is_some());
    assert!(replies.recv().await.is_some());
    filter.complete_job_id_multi(JobId::new(6));
    input
        .send(raw(to_job(6), CMsgClientHeartBeat::default()))
        .unwrap();
    settle().await;
    assert_eq!(filter.unprocessed().len(), 1);
}

#[tokio::test]
async fn kinds_reach_one_shot_and_broadcast_listeners() {
    let (filter, input) = filter();
    let once = filter.one_kind(EMsg::k_EMsgClientHeartBeat);
    let mut all = filter.on_kind(EMsg::k_EMsgClientLogOff);
    input
        .send(raw(to_job(u64::MAX), CMsgClientHeartBeat::default()))
        .unwrap();
    input
        .send(raw(to_job(u64::MAX), CMsgClientLogOff::default()))
        .unwrap();
    input
        .send(raw(to_job(u64::MAX), CMsgClientLogOff::default()))
        .unwrap();
    assert_eq!(once.await.unwrap().kind, EMsg::k_EMsgClientHeartBeat);
    assert_eq!(all.recv().await.unwrap().kind, EMsg::k_EMsgClientLogOff);
    assert_eq!(all.recv().await.unwrap().kind, EMsg::k_EMsgClientLogOff);
}

#[tokio::test]
async fn notifications_route_by_job_name() {
    let (filter, input) = filter();
    let name = "FriendMessagesClient.IncomingMessage#1";
    let mut notes = filter.on_notification(name);
    // Steam pushes notifications as service-method messages named after them.
    let note = ServiceMethodMessage(CFriendMessages_IncomingMessage_Notification::default())
        .with_kind(EMsg::k_EMsgServiceMethod, true);
    input.send(raw(NetMessageHeader::default(), note)).unwrap();
    assert_eq!(notes.recv().await.unwrap().job_name, name);
}

#[tokio::test]
async fn unclaimed_messages_keep_the_latest_32() {
    let (filter, input) = filter();
    for _ in 0..40 {
        input
            .send(raw(to_job(u64::MAX), CMsgClientHeartBeat::default()))
            .unwrap();
    }
    settle().await;
    assert_eq!(filter.unprocessed().len(), 32);
    assert!(filter.unprocessed().is_empty(), "taking clears the buffer");
}

#[test]
fn ring_buffer_drops_the_oldest() {
    let ring = RingBuffer::new(2);
    assert_eq!(ring.push(1), None);
    assert_eq!(ring.push(2), None);
    assert_eq!(ring.push(3), Some(1));
    assert_eq!(ring.take(), [2, 3]);
}
