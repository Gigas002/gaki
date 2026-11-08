use super::*;
use serde_json::json;

#[test]
fn static_entry_round_trips() {
    let c = Collection {
        id: "uc-0001".into(),
        name: "RPGs".into(),
        added: vec![10, 20, 30],
        removed: vec![20],
        filter_spec: None,
        deleted: false,
        changed: Vec::new(),
    };
    let (key, value) = c.to_entry();
    assert_eq!(key, "user-collections.uc-0001");
    let parsed = Collection::from_entry(&key, &value.unwrap()).unwrap();
    assert_eq!(parsed, c);
    // Membership: 10 in, 20 removed, 30 in.
    assert!(parsed.contains(10));
    assert!(!parsed.contains(20));
    assert!(parsed.contains(30));
}

#[test]
fn dynamic_entry_preserves_filter_spec() {
    let filter = json!({
        "nFormatVersion": 2,
        "filterGroups": [{ "rgOptions": [492], "bAcceptUnion": false }],
    });
    let c = Collection {
        id: "uc-dyn".into(),
        name: "Free Games".into(),
        added: vec![],
        removed: vec![],
        filter_spec: Some(filter.clone()),
        deleted: false,
        changed: Vec::new(),
    };
    let (key, value) = c.to_entry();
    let value = value.unwrap();
    // The opaque filter must survive verbatim.
    assert!(value.contains("filterGroups"));
    let parsed = Collection::from_entry(&key, &value).unwrap();
    assert!(parsed.is_dynamic());
    assert_eq!(parsed.filter_spec.as_ref().unwrap(), &filter);
    assert_eq!(parsed, c);
}

#[test]
fn deleted_collection_becomes_tombstone() {
    let c = Collection {
        id: "uc-x".into(),
        name: "Gone".into(),
        added: vec![1],
        removed: vec![],
        filter_spec: None,
        deleted: true,
        changed: Vec::new(),
    };
    let (key, value) = c.to_entry();
    assert_eq!(key, "user-collections.uc-x");
    assert!(value.is_none());
}

#[test]
fn merge_unions_and_honors_remote_deletion() {
    let mut store = CollectionsStore {
        namespace_version: 5,
        collections: vec![
            Collection {
                id: "uc-a".into(),
                name: "Local A".into(),
                added: vec![1, 2],
                removed: vec![],
                filter_spec: None,
                deleted: false,
                changed: Vec::new(),
            },
            Collection {
                id: "uc-gone".into(),
                name: "Doomed".into(),
                added: vec![9],
                removed: vec![],
                filter_spec: None,
                deleted: false,
                changed: Vec::new(),
            },
        ],
    };

    // Remote: adds appid 3 to uc-a and renames it, brings a brand-new uc-b,
    // and tombstones uc-gone.
    let a_entry = Collection {
        id: "uc-a".into(),
        name: "Remote A".into(),
        added: vec![2, 3],
        removed: vec![7],
        filter_spec: None,
        deleted: false,
        changed: Vec::new(),
    }
    .to_entry();
    let b_entry = Collection {
        id: "uc-b".into(),
        name: "Remote B".into(),
        added: vec![100],
        removed: vec![],
        filter_spec: None,
        deleted: false,
        changed: Vec::new(),
    }
    .to_entry();

    let remote = RemoteNamespace {
        version: 11,
        entries: vec![
            (a_entry.0, a_entry.1),
            (b_entry.0, b_entry.1),
            ("user-collections.uc-gone".into(), None),
        ],
    };
    store.apply_remote(remote);

    assert_eq!(store.namespace_version, 11);
    // uc-gone dropped.
    assert!(store.collections.iter().all(|c| c.id != "uc-gone"));
    // uc-b added.
    assert!(store.collections.iter().any(|c| c.id == "uc-b"));
    // uc-a: union of added {1,2}∪{2,3} = {1,2,3}; removed {7}; remote name.
    let a = store.collections.iter().find(|c| c.id == "uc-a").unwrap();
    assert_eq!(a.name, "Remote A");
    assert!(a.added.contains(&1) && a.added.contains(&2) && a.added.contains(&3));
    assert!(a.removed.contains(&7));
}

#[test]
fn resolve_by_name_id_and_ambiguity() {
    let store = CollectionsStore {
        namespace_version: 0,
        collections: vec![
            Collection {
                id: "uc-1".into(),
                name: "Shooters".into(),
                added: vec![],
                removed: vec![],
                filter_spec: None,
                deleted: false,
                changed: Vec::new(),
            },
            Collection {
                id: "uc-2".into(),
                name: "Dupe".into(),
                added: vec![],
                removed: vec![],
                filter_spec: None,
                deleted: false,
                changed: Vec::new(),
            },
            Collection {
                id: "uc-3".into(),
                name: "Dupe".into(),
                added: vec![],
                removed: vec![],
                filter_spec: None,
                deleted: false,
                changed: Vec::new(),
            },
        ],
    };
    // Case-insensitive name.
    assert_eq!(store.resolve("shooters").unwrap().id, "uc-1");
    // Exact id.
    assert_eq!(store.resolve("uc-2").unwrap().id, "uc-2");
    // Unknown.
    assert!(store.resolve("nope").is_err());
    // Ambiguous name.
    assert!(store.resolve("Dupe").is_err());
}

#[test]
fn builtins_are_created_on_first_edit() {
    let mut store = CollectionsStore::default();
    // Only the built-in ids are created on demand.
    assert!(store.add("uc-missing", &[1]).is_err());
    store.collections.clear();
    store.add(FAVORITE, &[570]).unwrap();
    store.remove(HIDDEN, &[10]).unwrap();
    let fav = store.resolve(FAVORITE).unwrap();
    assert!(fav.is_builtin() && fav.contains(570));
    assert_eq!(fav.name, "Favorites");
    let hidden = store.resolve(HIDDEN).unwrap();
    assert!(!hidden.contains(10));
    assert_eq!(hidden.removed, [10]);
}

fn remote_with(c: &Collection, version: u64) -> RemoteNamespace {
    let (key, value) = c.to_entry();
    RemoteNamespace {
        version,
        entries: vec![(key, value)],
    }
}

#[test]
fn local_changes_survive_a_pull() {
    // Steam still has 570 removed from an earlier push, and 440 added.
    let remote = Collection {
        id: FAVORITE.into(),
        name: "Favorites".into(),
        added: vec![440],
        removed: vec![570],
        ..Collection::default()
    };
    let mut store = CollectionsStore::default();
    store.collections.push(remote.clone());
    // Re-favourite 570 and drop 440 here, then pull before pushing.
    let c = &mut store.collections[0];
    c.set_member(570, true);
    c.set_member(440, false);
    store.apply_remote(remote_with(&remote, 3));

    let fav = store.resolve(FAVORITE).unwrap();
    assert!(fav.contains(570), "re-add kept");
    assert!(!fav.contains(440), "removal kept");
    assert_eq!(store.namespace_version, 3);
}

#[test]
fn remote_changes_win_for_untouched_apps() {
    let mut store = CollectionsStore::default();
    store.collections.push(Collection {
        id: FAVORITE.into(),
        added: vec![1, 2],
        ..Collection::default()
    });
    // Removed on Steam; nothing changed here.
    let remote = Collection {
        id: FAVORITE.into(),
        added: vec![2],
        removed: vec![1],
        ..Collection::default()
    };
    store.apply_remote(remote_with(&remote, 4));
    assert!(!store.resolve(FAVORITE).unwrap().contains(1));
}

#[test]
fn upload_clears_only_what_was_sent() {
    let mut store = CollectionsStore::default();
    store.collections.push(Collection {
        id: FAVORITE.into(),
        ..Collection::default()
    });
    store.collections.push(Collection {
        id: "uc-old".into(),
        deleted: true,
        ..Collection::default()
    });
    store.collections[0].set_member(1, true);
    let sent = store.clone();
    // Changed while the upload ran: 1 again (now out) and 2 (new).
    store.collections[0].set_member(1, false);
    store.collections[0].set_member(2, true);
    store.uploaded(&sent, 9);

    assert_eq!(store.namespace_version, 9);
    assert!(store.collections.iter().all(|c| c.id != "uc-old"));
    let mut pending = store.collections[0].changed.clone();
    pending.sort_unstable();
    assert_eq!(pending, [1, 2]);

    let sent = store.clone();
    store.uploaded(&sent, 10);
    assert!(store.collections[0].changed.is_empty());
}
