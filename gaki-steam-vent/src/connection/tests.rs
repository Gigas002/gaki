use super::*;
use crate::auth::{DeviceConfirmationHandler, NullGuardDataStore};
use crate::test_support::{self, FakeCm};
use gaki_steam_vent_proto::steammessages_clientserver_appinfo::{
    CMsgClientPICSProductInfoRequest, CMsgClientPICSProductInfoResponse,
};

fn assert_send<T: Send>(_: &T) {}

// Compiling this is the test: the login futures must be `Send` so callers can
// spawn them.
#[allow(dead_code)]
fn login_futures_are_send(list: &ServerList, token: &RefreshToken) {
    let info = ClientInfo::default();
    let fut = Connection::login(
        list,
        "a",
        "b",
        NullGuardDataStore,
        DeviceConfirmationHandler,
        &info,
    );
    assert_send(&fut);
    let boxed = DeviceConfirmationHandler.boxed();
    let fut = Connection::login(list, "a", "b", NullGuardDataStore, boxed, &info);
    assert_send(&fut);
    let fut = Connection::login_with_refresh_token_and_logon_id(list, token, 7);
    assert_send(&fut);
}

fn product_info(pending: bool) -> CMsgClientPICSProductInfoResponse {
    CMsgClientPICSProductInfoResponse {
        response_pending: Some(pending),
        ..Default::default()
    }
}

/// Answer the next job with `responses`, addressed to its source job id.
async fn answer(cm: &mut FakeCm, responses: Vec<CMsgClientPICSProductInfoResponse>) -> JobId {
    let request = cm.next_sent().await;
    let job = request.header.source_job_id;
    for response in responses {
        let header = NetMessageHeader {
            target_job_id: job,
            ..NetMessageHeader::default()
        };
        cm.reply(header, response);
    }
    job
}

async fn eventually_unprocessed(connection: &RawConnection, count: usize) {
    for _ in 0..50 {
        if connection.filter.unprocessed().len() == count {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("expected {count} unclaimed message(s)");
}

#[tokio::test]
async fn job_multi_streams_until_complete_then_unregisters() {
    let (connection, mut cm) = test_support::connect().await;
    let stream = connection.job_multi::<_, CMsgClientPICSProductInfoResponse>(
        CMsgClientPICSProductInfoRequest::default(),
    );
    let server = answer(&mut cm, vec![product_info(true), product_info(false)]);
    let (responses, job): (Vec<_>, _) = tokio::join!(stream.collect(), server);
    let pending: Vec<bool> = responses
        .into_iter()
        .map(|r| r.unwrap().response_pending())
        .collect();
    assert_eq!(pending, [true, false]);

    // A late reply to the finished job is no longer routed to it.
    cm.reply(
        NetMessageHeader {
            target_job_id: job,
            ..NetMessageHeader::default()
        },
        product_info(false),
    );
    eventually_unprocessed(&connection, 1).await;
}

#[tokio::test]
async fn dropping_a_job_multi_stream_unregisters_it() {
    let (connection, mut cm) = test_support::connect().await;
    let mut stream = Box::pin(
        connection.job_multi::<_, CMsgClientPICSProductInfoResponse>(
            CMsgClientPICSProductInfoRequest::default(),
        ),
    );
    let (first, job) = tokio::join!(stream.next(), answer(&mut cm, vec![product_info(true)]));
    assert!(first.unwrap().is_ok());
    drop(stream);

    cm.reply(
        NetMessageHeader {
            target_job_id: job,
            ..NetMessageHeader::default()
        },
        product_info(false),
    );
    eventually_unprocessed(&connection, 1).await;
}

#[tokio::test]
async fn job_multi_times_out() {
    let (mut connection, mut cm) = test_support::connect().await;
    connection.timeout = Duration::from_millis(30);
    let stream = connection.job_multi::<_, CMsgClientPICSProductInfoResponse>(
        CMsgClientPICSProductInfoRequest::default(),
    );
    let (responses, _) = tokio::join!(
        stream.collect::<Vec<_>>(),
        answer(&mut cm, vec![product_info(true)])
    );
    assert_eq!(responses.len(), 2);
    assert!(responses[0].is_ok());
    assert!(matches!(responses[1], Err(NetworkError::Timeout)));
}

#[tokio::test]
async fn job_gets_its_single_reply() {
    let (connection, mut cm) = test_support::connect().await;
    let job = connection
        .job::<_, CMsgClientPICSProductInfoResponse>(CMsgClientPICSProductInfoRequest::default());
    let (reply, _) = tokio::join!(job, answer(&mut cm, vec![product_info(false)]));
    assert!(!reply.unwrap().response_pending());
}
