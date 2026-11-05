//! Open a URL in the default browser (`xdg-open`). Call from a worker: the
//! launcher can block while locating or starting the browser.
use crate::utils::error::TuiError;
use std::process::{Command, Stdio};

pub fn open(url: &str) -> Result<(), TuiError> {
    let status = Command::new("xdg-open")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if !status.success() {
        return Err(TuiError::Problem(format!(
            "Browser launcher exited with {status}"
        )));
    }
    Ok(())
}
