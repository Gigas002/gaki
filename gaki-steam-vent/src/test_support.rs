//! An in-memory CM for tests: no network. Frames the client sends come out
//! of [`FakeCm::next_sent`]; frames given to [`FakeCm::reply`] arrive as if
//! Steam sent them.

use crate::base::{NetMessageHeader, SendableMessage};
use crate::connection::raw::RawConnection;
use crate::net::{NetworkError, RawNetMessage};
use bytes::BytesMut;
use futures::StreamExt;
use tokio::sync::mpsc;

pub(crate) struct FakeCm {
    sent: mpsc::UnboundedReceiver<BytesMut>,
    replies: mpsc::UnboundedSender<crate::net::Result<BytesMut>>,
}

/// A client connection wired to a fake CM, past the initial hello.
pub(crate) async fn connect() -> (RawConnection, FakeCm) {
    let (sent_tx, mut sent) = mpsc::unbounded_channel::<BytesMut>();
    let (replies, replies_rx) = mpsc::unbounded_channel();
    let sender = futures::sink::unfold(sent_tx, |tx, msg: BytesMut| async move {
        tx.send(msg).map_err(|_| NetworkError::EOF)?;
        Ok::<_, NetworkError>(tx)
    });
    let receiver = tokio_stream::wrappers::UnboundedReceiverStream::new(replies_rx).boxed();
    let connection = RawConnection::from_sender_receiver(Box::pin(sender), receiver)
        .await
        .expect("connect to the fake CM");
    let hello = sent.recv().await.expect("client says hello");
    assert!(RawNetMessage::read(hello).is_ok());
    (connection, FakeCm { sent, replies })
}

impl FakeCm {
    /// The next message the client sent.
    pub(crate) async fn next_sent(&mut self) -> RawNetMessage {
        let frame = self.sent.recv().await.expect("client sent a message");
        RawNetMessage::read(frame).expect("client sent a valid message")
    }

    /// Deliver `msg` to the client.
    pub(crate) fn reply<T: SendableMessage>(&self, header: NetMessageHeader, msg: T) {
        let frame = RawNetMessage::from_message(header, msg)
            .expect("encode reply")
            .into_bytes();
        self.replies.send(Ok(frame)).expect("client is listening");
    }
}
