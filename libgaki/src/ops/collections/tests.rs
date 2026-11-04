use super::*;

fn collection(id: &str, added: &[u32], removed: &[u32], dynamic: bool) -> Collection {
    Collection {
        id: id.to_string(),
        name: id.to_uppercase(),
        added: added.to_vec(),
        removed: removed.to_vec(),
        filter_spec: dynamic.then(|| serde_json::json!({})),
        deleted: false,
    }
}

#[test]
fn kinds_are_told_apart() {
    assert_eq!(
        Kind::of(&collection("favorite", &[], &[], false)),
        Kind::BuiltIn
    );
    assert_eq!(Kind::of(&collection("uc-1", &[], &[], true)), Kind::Dynamic);
    assert_eq!(Kind::of(&collection("uc-2", &[], &[], false)), Kind::Static);
    assert_eq!(Kind::BuiltIn.as_str(), "built-in");
}

#[test]
fn members_exclude_removals() {
    let s = Summary::of(&collection("uc-1", &[1, 2, 3], &[2], false));
    assert_eq!(s.app_ids, Some(vec![1, 3]));
    assert_eq!(s.count(), 2);
}

#[test]
fn dynamic_collections_have_no_member_list() {
    let s = Summary::of(&collection("uc-1", &[1], &[], true));
    assert_eq!(s.app_ids, None);
    assert_eq!(s.count(), 0);
}
