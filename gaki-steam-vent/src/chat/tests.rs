use super::*;

const FRIEND: u64 = 76_561_198_000_000_001;

fn note(entry_type: i32, local_echo: bool) -> CFriendMessages_IncomingMessage_Notification {
    CFriendMessages_IncomingMessage_Notification {
        steamid_friend: Some(FRIEND),
        chat_entry_type: Some(entry_type),
        local_echo: Some(local_echo),
        message: Some("hi".into()),
        rtime32_server_timestamp: Some(42),
        ..Default::default()
    }
}

#[test]
fn friend_message_is_message() {
    assert!(matches!(
        ChatEvent::try_from(note(1, false)),
        Ok(ChatEvent::Message(_))
    ));
}

#[test]
fn own_message_is_echo() {
    assert!(matches!(
        ChatEvent::try_from(note(1, true)),
        Ok(ChatEvent::EchoMessage(_))
    ));
}

#[test]
fn friend_typing_is_typing() {
    assert!(matches!(
        ChatEvent::try_from(note(2, false)),
        Ok(ChatEvent::Typing(_))
    ));
}

#[test]
fn own_typing_is_dropped() {
    assert!(ChatEvent::try_from(note(2, true)).is_err());
}

#[test]
fn game_invite_is_delivered() {
    match ChatEvent::try_from(note(3, false)) {
        Ok(ChatEvent::GameInvite(e)) => {
            assert_eq!(u64::from(e.source), FRIEND);
            assert_eq!(e.message, "hi");
        }
        other => panic!("expected GameInvite, got {other:?}"),
    }
}

#[test]
fn unknown_type_is_other_with_raw_value() {
    match ChatEvent::try_from(note(99, false)) {
        Ok(ChatEvent::Other(e)) => {
            assert_eq!(e.entry_type, 99);
            assert_eq!(e.server_timestamp, 42);
        }
        other => panic!("expected Other, got {other:?}"),
    }
}
