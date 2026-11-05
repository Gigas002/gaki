use crate::app::client::Command;

use std::error;
use std::fmt;
use std::fmt::Debug;
use std::io;
use std::sync::PoisonError;
use std::sync::mpsc::SendError;

#[derive(Debug)]
pub enum TuiError {
    Io(io::Error),
    Problem(String),
}

impl From<io::Error> for TuiError {
    fn from(err: io::Error) -> TuiError {
        TuiError::Io(err)
    }
}

impl From<SendError<Command>> for TuiError {
    fn from(err: SendError<Command>) -> TuiError {
        TuiError::Problem(format!("{:?}", err))
    }
}

impl From<serde_json::Error> for TuiError {
    fn from(err: serde_json::Error) -> TuiError {
        TuiError::Problem(format!("{:?}", err))
    }
}

impl<T> From<PoisonError<T>> for TuiError {
    fn from(err: PoisonError<T>) -> TuiError {
        TuiError::Problem(format!("{:?}", err))
    }
}

impl fmt::Display for TuiError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            // The message is already human-readable; show it directly rather
            // than the `Problem("…")` debug wrapper.
            TuiError::Problem(msg) => write!(f, "{}", msg),
            _ => write!(f, "{:?}", self),
        }
    }
}

impl error::Error for TuiError {
    fn description(&self) -> &str {
        "woosp"
    }

    fn cause(&self) -> Option<&dyn error::Error> {
        // Pass on reference
        None
    }
}
