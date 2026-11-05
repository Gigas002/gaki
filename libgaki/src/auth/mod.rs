//! Where gaki keeps the account it is signed in as.
//!
//! The secrets (the Steam refresh token and a browser web token) live in a
//! [`store::SecretStore`], by default the OS keyring, one item per secret;
//! they are never written to a file. What is not secret (account and persona
//! name, SteamID, which secrets are stored) is in `account.toml` under the
//! state directory.

pub mod session;
pub mod store;

pub use session::{
    StoredAccount, account_path, delete_session, load_session, save_session, stored_account,
};

#[cfg(test)]
mod redaction_tests {
    use crate::core::models::SessionState;

    const SECRET: &str = "eyJTOP-SECRET-TOKEN-VALUE";

    fn assert_redacted(what: &str, debug: String) {
        assert!(!debug.contains(SECRET), "{what} leaks a secret: {debug}");
    }

    /// `{:?}` of every type that holds a token or password shows none.
    #[test]
    fn debug_output_never_shows_a_secret() {
        let session = SessionState {
            account_name: Some("acct".into()),
            steam_id: Some(76561198000000001),
            refresh_token: Some(SECRET.to_owned().into()),
            web_token: Some(SECRET.to_owned().into()),
            ..SessionState::default()
        };
        assert_redacted("SessionState", format!("{session:?} {session:#?}"));

        let info = crate::web::web_token::WebTokenInfo {
            steam_id: 1,
            account_name: None,
            token: SECRET.to_owned().into(),
            expires_at: None,
        };
        assert_redacted("WebTokenInfo", format!("{info:?}"));

        let web = crate::web::web_session::WebSession::new(1, SECRET, Some("US")).unwrap();
        assert_redacted("WebSession", format!("{web:?}"));

        let login = crate::ops::auth::PasswordLogin {
            username: "acct".into(),
            password: SECRET.to_owned().into(),
            guard_code: None,
            read_code_from_stdin: false,
            timeout: None,
        };
        assert_redacted("PasswordLogin", format!("{login:?}"));
    }
}
