use super::*;
use std::pin::Pin;

fn method(ty: EAuthSessionGuardType, message: &str) -> ConfirmationMethod {
    let mut raw = CAuthentication_AllowedConfirmation::new();
    raw.set_confirmation_type(ty);
    raw.set_associated_message(message.into());
    ConfirmationMethod::from(raw)
}

fn email() -> ConfirmationMethod {
    method(
        EAuthSessionGuardType::k_EAuthSessionGuardType_EmailCode,
        "a***@example.com",
    )
}

fn device_confirmation() -> ConfirmationMethod {
    method(
        EAuthSessionGuardType::k_EAuthSessionGuardType_DeviceConfirmation,
        "",
    )
}

async fn run(
    handler: &mut impl AuthConfirmationHandler,
    allowed: &[ConfirmationMethod],
) -> Option<ConfirmationAction> {
    Pin::from(handler.handle_confirmation(allowed)).await
}

#[test]
fn methods_classify() {
    use EAuthSessionGuardType::*;
    let cases = [
        (
            k_EAuthSessionGuardType_None,
            ConfirmationMethodClass::None,
            false,
        ),
        (
            k_EAuthSessionGuardType_EmailCode,
            ConfirmationMethodClass::Code,
            true,
        ),
        (
            k_EAuthSessionGuardType_DeviceCode,
            ConfirmationMethodClass::Code,
            true,
        ),
        (
            k_EAuthSessionGuardType_DeviceConfirmation,
            ConfirmationMethodClass::Confirmation,
            false,
        ),
        (
            k_EAuthSessionGuardType_MachineToken,
            ConfirmationMethodClass::Stored,
            false,
        ),
    ];
    for (ty, class, wants_code) in cases {
        let m = method(ty, "");
        assert_eq!(m.class(), class, "{ty:?}");
        assert_eq!(m.token_type().is_some(), wants_code, "{ty:?}");
    }
    assert!(!method(k_EAuthSessionGuardType_None, "").action_required());
    assert!(email().action_required());
    assert_eq!(email().confirmation_details(), "a***@example.com");
}

#[tokio::test]
async fn user_provided_code_is_sent() {
    let mut handler = UserProvidedAuthConfirmationHandler::new(&b" ABCDE \n"[..], Vec::new());
    let action = run(&mut handler, &[email()]).await;
    match action {
        Some(ConfirmationAction::GuardToken(token, GuardTokenType::Email)) => {
            assert_eq!(token.0, "ABCDE")
        }
        other => panic!("expected an email code, got {other:?}"),
    }
    let prompt = String::from_utf8(handler.output.clone()).unwrap();
    assert_eq!(prompt, "email: a***@example.com");
}

#[tokio::test]
async fn empty_input_aborts() {
    let mut handler = UserProvidedAuthConfirmationHandler::new(&b"\n"[..], Vec::new());
    assert!(matches!(
        run(&mut handler, &[email()]).await,
        Some(ConfirmationAction::Abort)
    ));
}

#[tokio::test]
async fn handlers_pass_on_methods_they_cannot_serve() {
    let mut code = UserProvidedAuthConfirmationHandler::new(&b"1\n"[..], Vec::new());
    assert!(run(&mut code, &[device_confirmation()]).await.is_none());
    assert!(
        run(&mut DeviceConfirmationHandler, &[email()])
            .await
            .is_none()
    );
}

#[tokio::test]
async fn device_handler_waits_for_the_app() {
    assert!(matches!(
        run(&mut DeviceConfirmationHandler, &[device_confirmation()]).await,
        Some(ConfirmationAction::None)
    ));
}

#[tokio::test]
async fn shared_secret_generates_a_code() {
    let mut handler =
        SharedSecretAuthConfirmationHandler::new("000000000000000000000000000=").unwrap();
    match run(&mut handler, &[email()]).await {
        Some(ConfirmationAction::GuardToken(token, _)) => assert_eq!(token.0.len(), 5),
        other => panic!("expected a code, got {other:?}"),
    }
}

#[test]
fn bad_shared_secret_fails_up_front() {
    assert!(SharedSecretAuthConfirmationHandler::new("").is_err());
}

#[tokio::test]
async fn combined_handlers_prefer_a_real_action() {
    let code = UserProvidedAuthConfirmationHandler::new(&b"12345\n"[..], Vec::new());
    let mut both = (DeviceConfirmationHandler, code);
    let action = run(&mut both, &[device_confirmation(), email()]).await;
    assert!(matches!(
        action,
        Some(ConfirmationAction::GuardToken(_, GuardTokenType::Email))
    ));
}

#[tokio::test]
async fn handler_lists_take_the_first_answer() {
    let mut handlers = vec![DeviceConfirmationHandler, DeviceConfirmationHandler];
    assert!(matches!(
        run(&mut handlers, &[device_confirmation()]).await,
        Some(ConfirmationAction::None)
    ));
    assert!(run(&mut handlers, &[email()]).await.is_none());
}
