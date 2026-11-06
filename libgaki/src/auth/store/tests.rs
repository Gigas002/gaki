use super::*;

#[test]
fn memory_store_sets_gets_and_deletes() {
    let store = Memory::default();
    assert_eq!(store.get("a").unwrap(), None);
    store.set("a", "1").unwrap();
    assert_eq!(store.get("a").unwrap().as_deref(), Some("1"));
    store.delete("a").unwrap();
    store.delete("a").unwrap();
    assert_eq!(store.get("a").unwrap(), None);
}

#[test]
fn keys_are_per_account() {
    assert_eq!(refresh_token_key(7), "refresh-token:7");
    assert_eq!(web_token_key(7), "web-token:7");
}
