use super::*;

#[test]
fn unknown_persona_states_read_as_online() {
    assert_eq!(persona_state_label(None), "offline");
    assert_eq!(persona_state_label(Some(3)), "away");
    assert_eq!(persona_state_label(Some(42)), "online");
}

#[test]
fn visibility_has_labels() {
    assert_eq!(visibility_label(3), "public");
    assert_eq!(visibility_label(0), "unknown");
}
