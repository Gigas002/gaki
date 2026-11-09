//! The CM websocket transport (`wss://<server>/cmsocket/`).

use crate::net::NetworkError;
use bytes::{Bytes, BytesMut};
use futures::{Sink, SinkExt, StreamExt, TryStreamExt};
use rustls::{ClientConfig, RootCertStore};
use std::future::ready;
use std::sync::Arc;
use tokio_stream::Stream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{Connector, connect_async_tls_with_config};
use tracing::{debug, instrument};

type Result<T, E = NetworkError> = std::result::Result<T, E>;

#[instrument]
pub async fn connect(
    addr: &str,
) -> Result<(
    impl Sink<BytesMut, Error = NetworkError> + use<>,
    impl Stream<Item = Result<BytesMut>> + use<>,
)> {
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .ok(); // can only be once called
    let mut root_store = RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    // No `key_log`: upstream set `KeyLogFile`, which dumps the session's TLS
    // secrets to whatever `SSLKEYLOGFILE` names.
    let tls_config = ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    let tls_config = Connector::Rustls(Arc::new(tls_config));
    let (stream, _) = connect_async_tls_with_config(addr, None, false, Some(tls_config)).await?;
    debug!("connected to websocket server");
    let (raw_write, raw_read) = stream.split();

    Ok((
        raw_write.with(|msg: BytesMut| ready(Ok(Message::binary(msg)))),
        raw_read
            .map_err(NetworkError::from)
            .map_ok(Message::into_data)
            .map_ok(Bytes::from)
            .map_ok(BytesMut::from),
    ))
}
