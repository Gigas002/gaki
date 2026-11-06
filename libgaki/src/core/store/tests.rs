use super::*;

#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct Blob {
    #[serde(default, with = "json_text", skip_serializing_if = "Option::is_none")]
    spec: Option<serde_json::Value>,
}

#[test]
fn json_with_nulls_round_trips_as_text() {
    let blob = Blob {
        spec: Some(serde_json::json!({ "a": null, "b": [1, 2] })),
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sub/blob.toml");
    write(&path, &blob).unwrap();
    assert_eq!(read::<Blob>(&path).unwrap(), Some(blob));
    assert_eq!(read::<Blob>(&dir.path().join("none.toml")).unwrap(), None);
}

#[test]
fn a_broken_file_is_an_error_naming_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.toml");
    std::fs::write(&path, "spec = ").unwrap();
    let err = format!("{:#}", read::<Blob>(&path).unwrap_err());
    assert!(err.contains("bad.toml"), "{err}");
}
