use super::*;
use std::os::unix::fs::PermissionsExt;

fn store_in(dir: &tempfile::TempDir) -> FileGuardDataStore {
    FileGuardDataStore::new(dir.path().join("nested/machine_tokens.json"))
}

#[tokio::test]
async fn tokens_round_trip_per_account() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = store_in(&dir);
    assert_eq!(store.load("alice").await.unwrap(), None);
    store.store("alice", "token-a".into()).await.unwrap();
    store.store("bob", "token-b".into()).await.unwrap();
    assert_eq!(
        store.load("alice").await.unwrap().as_deref(),
        Some("token-a")
    );
    assert_eq!(store.load("bob").await.unwrap().as_deref(), Some("token-b"));
}

#[tokio::test]
async fn token_file_is_owner_only() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = store_in(&dir);
    store.store("alice", "token".into()).await.unwrap();
    let mode = std::fs::metadata(&store.path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[tokio::test]
async fn existing_world_readable_file_is_narrowed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("machine_tokens.json");
    std::fs::write(&path, "{}").unwrap();
    std::fs::set_permissions(&path, Permissions::from_mode(0o644)).unwrap();
    let mut store = FileGuardDataStore::new(path.clone());
    store.store("alice", "token".into()).await.unwrap();
    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[tokio::test]
async fn empty_tokens_are_not_stored() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = store_in(&dir);
    store.store("alice", String::new()).await.unwrap();
    assert!(!store.path.exists());
}

#[tokio::test]
async fn corrupt_file_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("machine_tokens.json");
    std::fs::write(&path, "not json").unwrap();
    let mut store = FileGuardDataStore::new(path);
    assert!(matches!(
        store.load("alice").await,
        Err(FileStoreError::Json { .. })
    ));
}

#[tokio::test]
async fn null_store_keeps_nothing() {
    let mut store = NullGuardDataStore;
    store.store("alice", "token".into()).await.unwrap();
    assert_eq!(store.load("alice").await.unwrap(), None);
}
