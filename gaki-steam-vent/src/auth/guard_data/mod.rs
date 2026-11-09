use std::collections::HashMap;
use std::convert::Infallible;
use std::error::Error;
use std::fs::{OpenOptions, Permissions, create_dir_all, read_to_string};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Trait for storing steam guard machine tokens
pub trait GuardDataStore {
    type Err: Error;

    /// Store a machine token for an account
    fn store(
        &mut self,
        account: &str,
        machine_token: String,
    ) -> impl Future<Output = Result<(), Self::Err>> + Send;

    /// Retrieve the stored token for an account
    fn load(
        &mut self,
        account: &str,
    ) -> impl Future<Output = Result<Option<String>, Self::Err>> + Send;
}

impl<Store: GuardDataStore> GuardDataStore for &mut Store {
    type Err = Store::Err;

    fn store(
        &mut self,
        account: &str,
        machine_token: String,
    ) -> impl Future<Output = Result<(), Self::Err>> + Send {
        (**self).store(account, machine_token)
    }

    fn load(
        &mut self,
        account: &str,
    ) -> impl Future<Output = Result<Option<String>, Self::Err>> + Send {
        (**self).load(account)
    }
}

/// Error while storing or loading guard data from json file
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum FileStoreError {
    /// Error while reading the json file
    #[error("error while reading tokens from {}: {:#}", path.display(), err)]
    Read { err: std::io::Error, path: PathBuf },
    /// Error while writing the json file
    #[error("error while writing tokens to {}: {:#}", path.display(), err)]
    Write { err: std::io::Error, path: PathBuf },
    /// Error when encoding or decoding the tokens
    #[error("error while parsing tokens from {}: {:#}", path.display(), err)]
    Json {
        err: serde_json::error::Error,
        path: PathBuf,
    },
    /// Error while creating the parent directory of the file
    #[error("error while directory {} for tokens: {:#}", path.display(), err)]
    DirCreation { err: std::io::Error, path: PathBuf },
}

/// Store the steam guard data in a json file
pub struct FileGuardDataStore {
    path: PathBuf,
}

impl FileGuardDataStore {
    /// Store the machine tokens in the JSON file at `path`.
    ///
    /// The caller picks the location; the library writes nowhere on its own.
    pub fn new(path: PathBuf) -> Self {
        FileGuardDataStore { path }
    }

    fn all_tokens(&self) -> Result<HashMap<String, String>, FileStoreError> {
        if !self.path.exists() {
            return Ok(HashMap::default());
        }
        let raw = read_to_string(&self.path).map_err(|err| FileStoreError::Read {
            err,
            path: self.path.clone(),
        })?;
        serde_json::from_str(&raw).map_err(|err| FileStoreError::Json {
            err,
            path: self.path.clone(),
        })
    }

    fn save(&self, tokens: HashMap<String, String>) -> Result<(), FileStoreError> {
        if let Some(parent) = self.path.parent() {
            create_dir_all(parent).map_err(|err| FileStoreError::DirCreation {
                err,
                path: parent.into(),
            })?;
        }

        let raw = serde_json::to_string(&tokens).map_err(|err| FileStoreError::Json {
            err,
            path: self.path.clone(),
        })?;
        write_private(&self.path, raw.as_bytes()).map_err(|err| FileStoreError::Write {
            err,
            path: self.path.clone(),
        })?;
        Ok(())
    }
}

impl GuardDataStore for FileGuardDataStore {
    type Err = FileStoreError;

    async fn store(&mut self, account: &str, machine_token: String) -> Result<(), Self::Err> {
        if !machine_token.is_empty() {
            let mut tokens = self.all_tokens()?;
            tokens.insert(account.into(), machine_token);
            self.save(tokens)
        } else {
            Ok(())
        }
    }

    async fn load(&mut self, account: &str) -> Result<Option<String>, Self::Err> {
        let mut tokens = self.all_tokens()?;
        Ok(tokens.remove(account).filter(|token| !token.is_empty()))
    }
}

/// Write `data` to `path` readable by the owner only: machine tokens let
/// this device skip Steam Guard. A file that already exists is narrowed too.
fn write_private(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.set_permissions(Permissions::from_mode(0o600))?;
    file.write_all(data)
}

/// Don't store guard data
pub struct NullGuardDataStore;

impl GuardDataStore for NullGuardDataStore {
    type Err = Infallible;

    async fn store(&mut self, _account: &str, _machine_token: String) -> Result<(), Self::Err> {
        Ok(())
    }

    async fn load(&mut self, _account: &str) -> Result<Option<String>, Self::Err> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests;
