//! gaki's own state files (install backlog, collections, running games,
//! cloud-sync baselines, version pins): TOML, written atomically.

use anyhow::{Context, Result};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::path::Path;

/// Read `path` as TOML. `None` when the file doesn't exist.
pub fn read<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).with_context(|| format!("failed reading {}", path.display())),
    };
    toml::from_str(&text)
        .map(Some)
        .with_context(|| format!("failed parsing {}", path.display()))
}

/// Write `value` to `path` as TOML, through a temporary file so a reader
/// never sees half of it. Creates the directory.
pub fn write<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let text =
        toml::to_string(value).with_context(|| format!("failed serializing {}", path.display()))?;
    crate::config::edit::write_atomic(path, &text)
}

/// Serde adapter for a JSON value kept as JSON text inside a TOML string:
/// Steam's opaque blobs can hold `null`, which TOML can't.
pub mod json_text {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(
        value: &Option<serde_json::Value>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            Some(value) => serializer.serialize_some(&value.to_string()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<serde_json::Value>, D::Error> {
        Option::<String>::deserialize(deserializer)?
            .map(|text| serde_json::from_str(&text).map_err(serde::de::Error::custom))
            .transpose()
    }
}

#[cfg(test)]
mod tests {
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
}
