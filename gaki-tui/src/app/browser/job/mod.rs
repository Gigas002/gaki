//! Background loads for the overlays: a job runs on a thread and drops its
//! result in a slot; the tick adopts it, so the UI never waits on Steam.

use std::sync::{Arc, Mutex};
use std::thread;

use crate::utils::error::TuiError;

/// A background job's result, filled in by its thread.
pub type Slot<T> = Arc<Mutex<Option<Result<T, String>>>>;

/// Run `job` on a thread; its result lands in the returned slot.
pub fn spawn<T: Send + 'static>(
    job: impl FnOnce() -> Result<T, TuiError> + Send + 'static,
) -> Slot<T> {
    let slot: Slot<T> = Arc::new(Mutex::new(None));
    let out = Arc::clone(&slot);
    thread::spawn(move || {
        let result = job().map_err(|e| e.to_string());
        if let Ok(mut guard) = out.lock() {
            *guard = Some(result);
        }
    });
    slot
}

/// Take a finished job's result, if it has finished.
pub fn take<T>(slot: &Slot<T>) -> Option<Result<T, String>> {
    match slot.lock() {
        Ok(mut guard) => guard.take(),
        Err(_) => Some(Err("the background job crashed".to_string())),
    }
}

/// Data that is loading in the background, loaded, or failed to load.
pub enum Load<T> {
    Loading(Slot<T>),
    Ready(T),
    Failed(String),
}

/// A clone shares a running job's slot: whichever polls first adopts it.
impl<T: Clone> Clone for Load<T> {
    fn clone(&self) -> Self {
        match self {
            Self::Loading(slot) => Self::Loading(Arc::clone(slot)),
            Self::Ready(value) => Self::Ready(value.clone()),
            Self::Failed(e) => Self::Failed(e.clone()),
        }
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for Load<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Loading(_) => f.write_str("Loading"),
            Self::Ready(value) => f.debug_tuple("Ready").field(value).finish(),
            Self::Failed(e) => f.debug_tuple("Failed").field(e).finish(),
        }
    }
}

impl<T: Default> Default for Load<T> {
    fn default() -> Self {
        Self::Ready(T::default())
    }
}

impl<T: Send + 'static> Load<T> {
    pub fn start(job: impl FnOnce() -> Result<T, TuiError> + Send + 'static) -> Self {
        Self::Loading(spawn(job))
    }
}

impl<T> Load<T> {
    /// Adopt a finished job. Returns whether anything changed.
    pub fn poll(&mut self) -> bool {
        let Self::Loading(slot) = self else {
            return false;
        };
        match take(slot) {
            Some(Ok(value)) => *self = Self::Ready(value),
            Some(Err(e)) => *self = Self::Failed(e),
            None => return false,
        }
        true
    }

    pub fn ready(&self) -> Option<&T> {
        match self {
            Self::Ready(value) => Some(value),
            _ => None,
        }
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading(_))
    }

    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Failed(e) => Some(e),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
