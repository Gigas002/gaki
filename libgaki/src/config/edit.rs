//! Editing the config files in place, keeping the user's comments and layout.
//!
//! gaki only writes `config.toml` and `games.toml` when asked to (`gaki-cli
//! config ...`, the TUI's game settings), and a hand-edited file should come
//! back with only the requested keys changed. Every save is checked by parsing
//! the result as the typed file first, so an edit can't leave a file gaki
//! can't load.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, Value};

/// One TOML file, open for editing.
pub struct TomlFile {
    path: PathBuf,
    pub doc: DocumentMut,
}

impl TomlFile {
    /// Open `path`; a missing file is an empty document.
    pub fn open(path: &Path) -> Result<Self> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e).with_context(|| format!("could not read {}", path.display())),
        };
        let doc = text
            .parse::<DocumentMut>()
            .with_context(|| format!("could not parse {}", path.display()))?;
        Ok(Self {
            path: path.to_path_buf(),
            doc,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Parse the edited document as `T`, so a bad value is reported before
    /// anything is written.
    pub fn parse<T: serde::de::DeserializeOwned>(&self) -> Result<T> {
        toml::from_str(&self.doc.to_string())
            .with_context(|| format!("the change would make {} invalid", self.path.display()))
    }

    /// Write the document atomically (a temp file renamed over the old one).
    pub fn save(&self) -> Result<()> {
        let dir = self.path.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(dir).with_context(|| format!("could not create {}", dir.display()))?;
        let name = self
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let tmp = dir.join(format!(".{name}.tmp"));
        fs::write(&tmp, self.doc.to_string())
            .with_context(|| format!("could not write {}", tmp.display()))?;
        fs::rename(&tmp, &self.path)
            .with_context(|| format!("could not replace {}", self.path.display()))
    }

    /// Set (`Some`) or remove (`None`) a top-level key. Returns whether the
    /// file changed.
    pub fn set(&mut self, key: &str, value: Option<Value>) -> bool {
        set_in(self.doc.as_table_mut(), key, value)
    }
}

/// Set or remove `key` in `table`, keeping the comment above an existing key.
pub fn set_in(table: &mut Table, key: &str, value: Option<Value>) -> bool {
    match value {
        None => table.remove(key).is_some(),
        Some(mut value) => {
            match table.get_mut(key) {
                // Replace in place: the comment above belongs to the key.
                Some(Item::Value(old)) => {
                    if old.to_string().trim() == value.to_string().trim() {
                        return false;
                    }
                    // Keep the line's trailing comment and spacing.
                    *value.decor_mut() = old.decor().clone();
                    *old = value;
                }
                _ => {
                    table.insert(key, Item::Value(value));
                }
            }
            true
        }
    }
}

/// The `[[game]]` tables of a `games.toml` document.
fn games(doc: &mut DocumentMut) -> Option<&mut ArrayOfTables> {
    doc.get_mut("game").and_then(Item::as_array_of_tables_mut)
}

fn is_game(table: &Table, app_id: u32) -> bool {
    table.get("app_id").and_then(Item::as_integer) == Some(i64::from(app_id))
}

/// The `[[game]]` table for `app_id`, added at the end when there is none.
pub fn game_mut(doc: &mut DocumentMut, app_id: u32) -> &mut Table {
    if games(doc).is_none() {
        doc.insert("game", Item::ArrayOfTables(ArrayOfTables::new()));
    }
    let games = games(doc).expect("just inserted");
    let found = games.iter().position(|t| is_game(t, app_id));
    let index = match found {
        Some(index) => index,
        None => {
            let mut table = Table::new();
            table.insert("app_id", toml_edit::value(i64::from(app_id)));
            games.push(table);
            games.len() - 1
        }
    };
    games.get_mut(index).expect("index is in range")
}

/// Remove the `[[game]]` table for `app_id`. Returns whether there was one.
pub fn remove_game(doc: &mut DocumentMut, app_id: u32) -> bool {
    let Some(games) = games(doc) else {
        return false;
    };
    let before = games.len();
    games.retain(|t| !is_game(t, app_id));
    let removed = games.len() != before;
    if games.is_empty() {
        doc.remove("game");
    }
    removed
}

/// Drop `app_id`'s table when nothing but `app_id` is left in it.
pub fn prune_game(doc: &mut DocumentMut, app_id: u32) {
    let empty = games(doc)
        .and_then(|games| games.iter().find(|t| is_game(t, app_id)))
        .is_some_and(|t| t.len() == 1);
    if empty {
        remove_game(doc, app_id);
    }
}

/// Remove every `[[game]]` table. Returns how many there were.
pub fn clear_games(doc: &mut DocumentMut) -> usize {
    let count = games(doc).map_or(0, |games| games.len());
    doc.remove("game");
    count
}
