use super::*;

#[test]
fn logon_id_is_sent_as_private_ip() {
    let logon = logon_message(Some("user"), "token", 0xDEAD_BEEF);
    assert_eq!(logon.obfuscated_private_ip.v4(), 0xDEAD_BEEF);
}

#[test]
fn default_logon_id_stays_zero() {
    let logon = logon_message(None, "token", 0);
    assert_eq!(logon.obfuscated_private_ip.v4(), 0);
    assert!(logon.account_name.is_none());
    assert_eq!(logon.access_token(), "token");
}
#[tokio::test]
async fn anonymous_logon_needs_no_token() {
    let (connection, mut cm) = crate::test_support::connect().await;
    let session = anonymous(&connection, AccountType::AnonUser);
    let server = async {
        let logon = cm.next_sent().await;
        assert_eq!(logon.kind, CMsgClientLogon::KIND);
        let mut response = CMsgClientLogonResponse::new();
        response.set_eresult(1);
        response.set_heartbeat_seconds(9);
        cm.reply(NetMessageHeader::default(), response);
    };
    let (session, ()) = tokio::join!(session, server);
    let session = session.expect("anonymous logon must not demand a token");
    assert_eq!(session.session.heartbeat_interval, Duration::from_secs(9));
    assert!(session.auth.refresh_token.is_empty());
    assert_eq!(session.auth.refresh_token.token(), "");
}

#[test]
fn real_logons_still_need_a_valid_token() {
    assert!(RefreshToken::new("not-a-jwt".into()).is_err());
}

#[test]
fn heartbeat_falls_back_when_steam_gives_none() {
    let default = RawSession::default().heartbeat_interval;
    assert_eq!(heartbeat_interval(0), default);
    assert_eq!(heartbeat_interval(-5), default);
    assert_eq!(heartbeat_interval(30), Duration::from_secs(30));
}

#[tokio::test]
async fn failed_logon_maps_to_login_error() {
    let (connection, mut cm) = crate::test_support::connect().await;
    let session = anonymous(&connection, AccountType::AnonUser);
    let server = async {
        let _logon = cm.next_sent().await;
        let mut response = CMsgClientLogonResponse::new();
        response.set_eresult(EResult::InvalidPassword as i32);
        cm.reply(NetMessageHeader::default(), response);
    };
    let (session, ()) = tokio::join!(session, server);
    assert!(matches!(
        session,
        Err(ConnectionError::LoginError(LoginError::InvalidCredentials))
    ));
}

#[test]
fn eresults_map_to_login_errors() {
    assert!(matches!(
        LoginError::from(EResult::AccountNotFound),
        LoginError::InvalidCredentials
    ));
    assert!(matches!(
        LoginError::from(EResult::RateLimitExceeded),
        LoginError::RateLimited
    ));
    assert!(matches!(
        LoginError::from(EResult::AccountLoginDeniedNeedTwoFactor),
        LoginError::SteamGuardRequired
    ));
    assert!(matches!(
        LoginError::from(EResult::Busy),
        LoginError::Unknown(EResult::Busy)
    ));
}

#[test]
fn job_ids_count_up_from_one() {
    let counter = JobIdCounter::default();
    assert_eq!(counter.next(), JobId::new(1));
    assert_eq!(counter.next(), JobId::new(2));
    assert_eq!(counter.clone().next(), JobId::new(3));
}
