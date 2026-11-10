use super::*;

#[test]
fn typed_error_survives_context_layers() {
    let err = anyhow::Error::new(
        TypedError::new(ErrorKind::RateLimited, "slow down")
            .with_retry_after(Some(Duration::from_secs(30))),
    )
    .context("outer");
    let c = classify(&err);
    assert_eq!(c.kind, ErrorKind::RateLimited);
    assert_eq!(c.retry_after, Some(Duration::from_secs(30)));
}

#[cfg(feature = "authentication")]
#[test]
fn login_rate_limit_is_rate_limited() {
    let err = anyhow::Error::new(LoginError::RateLimited).context("refresh token login failed");
    assert_eq!(classify(&err).kind, ErrorKind::RateLimited);
}

#[cfg(feature = "authentication")]
#[test]
fn invalid_credentials_is_auth_required() {
    let err = anyhow::Error::new(LoginError::InvalidCredentials);
    assert_eq!(classify(&err).kind, ErrorKind::AuthRequired);
}

#[cfg(feature = "authentication")]
#[test]
fn api_eresult_maps_through_network_error() {
    let err = anyhow::Error::new(NetworkError::ApiError(EResult::RateLimitExceeded));
    assert_eq!(classify(&err).kind, ErrorKind::RateLimited);
    let err = anyhow::Error::new(NetworkError::ApiError(EResult::AccessDenied));
    assert_eq!(classify(&err).kind, ErrorKind::AccessDenied);
    let err = anyhow::Error::new(NetworkError::Timeout);
    assert_eq!(classify(&err).kind, ErrorKind::NetworkTimeout);
}

#[test]
fn plain_errors_are_unknown() {
    let err = anyhow::anyhow!("something else");
    assert_eq!(classify(&err).kind, ErrorKind::Unknown);
    assert_eq!(ErrorKind::Unknown.exit_code(), 1);
    assert_eq!(ErrorKind::RateLimited.exit_code(), 75);
    assert_eq!(ErrorKind::AuthRequired.exit_code(), 77);
}

#[test]
fn storage_full_is_disk_full() {
    let io = std::io::Error::from(std::io::ErrorKind::StorageFull);
    let err = anyhow::Error::new(io).context("writing chunk");
    assert_eq!(classify(&err).kind, ErrorKind::DiskFull);
    assert_eq!(ErrorKind::DiskFull.exit_code(), 85);
}

#[test]
fn enospc_and_edquot_are_disk_full() {
    for code in [28, 122] {
        let err = anyhow::Error::new(std::io::Error::from_raw_os_error(code));
        assert_eq!(classify(&err).kind, ErrorKind::DiskFull, "errno {code}");
    }
}

#[test]
fn unreachable_network_is_offline() {
    for kind in [
        std::io::ErrorKind::NetworkUnreachable,
        std::io::ErrorKind::HostUnreachable,
        std::io::ErrorKind::NetworkDown,
    ] {
        let err = anyhow::Error::new(std::io::Error::from(kind));
        assert_eq!(classify(&err).kind, ErrorKind::Offline, "{kind:?}");
    }
    assert_eq!(ErrorKind::Offline.exit_code(), 86);
}

#[cfg(feature = "authentication")]
#[test]
fn io_inside_steam_vent_is_classified() {
    let io = std::io::Error::from(std::io::ErrorKind::NetworkUnreachable);
    let err = anyhow::Error::new(NetworkError::IO(io));
    assert_eq!(classify(&err).kind, ErrorKind::Offline);
}

#[test]
fn refused_and_timed_out_stay_network_timeout() {
    for kind in [
        std::io::ErrorKind::ConnectionRefused,
        std::io::ErrorKind::TimedOut,
    ] {
        let err = anyhow::Error::new(std::io::Error::from(kind));
        assert_eq!(classify(&err).kind, ErrorKind::NetworkTimeout);
    }
}

#[test]
fn text_only_errors_fall_back_to_messages() {
    let cases = [
        (
            "Failed to download depot 1: No space left on device (os error 28)",
            ErrorKind::DiskFull,
        ),
        (
            "There is not enough space on the disk. (os error 112)",
            ErrorKind::DiskFull,
        ),
        (
            "web request - error sending request: dns error: failed to lookup address",
            ErrorKind::Offline,
        ),
        (
            "connect: Network is unreachable (os error 101)",
            ErrorKind::Offline,
        ),
    ];
    for (text, kind) in cases {
        assert_eq!(classify(&anyhow::anyhow!("{text}")).kind, kind, "{text}");
    }
    assert_eq!(
        classify(&anyhow::anyhow!("something else")).kind,
        ErrorKind::Unknown
    );
}

#[test]
fn typed_kind_beats_message_text() {
    let err = anyhow::Error::new(TypedError::new(ErrorKind::Cancelled, "no route to host"));
    assert_eq!(classify(&err).kind, ErrorKind::Cancelled);
}

#[test]
fn special_kinds_have_distinct_exit_codes() {
    let codes = [
        0,
        1,
        ErrorKind::RateLimited.exit_code(),
        ErrorKind::AuthRequired.exit_code(),
        ErrorKind::LoginDenied.exit_code(),
        ErrorKind::LoginExpired.exit_code(),
        ErrorKind::Timeout.exit_code(),
        ErrorKind::Cancelled.exit_code(),
        ErrorKind::DiskFull.exit_code(),
        ErrorKind::Offline.exit_code(),
    ];
    let unique: std::collections::HashSet<_> = codes.iter().collect();
    assert_eq!(unique.len(), codes.len(), "{codes:?}");
}

#[test]
fn kind_serializes_snake_case_for_the_backlog() {
    assert_eq!(
        serde_json::to_value(ErrorKind::PrivacyRestricted).unwrap(),
        serde_json::json!("privacy_restricted")
    );
}
