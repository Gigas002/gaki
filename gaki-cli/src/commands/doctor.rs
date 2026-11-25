//! `doctor` command handler.

use anyhow::Result;
use libgaki::ops::doctor::{self, Level};

use crate::output::style;

/// `doctor`: print every finding; fail when one is an error.
pub(crate) fn cmd_doctor() -> Result<()> {
    let findings = doctor::run()?;
    for finding in &findings {
        let tag = format!("[{}]", finding.level);
        let tag = match finding.level {
            Level::Ok => style::green(&tag),
            Level::Warn => style::yellow(&tag),
            Level::Error => style::red(&tag),
        };
        cli_println!(" {tag} {}: {}", style::bold(finding.check), finding.message);
        if let Some(hint) = finding.hint.as_ref().filter(|_| finding.level != Level::Ok) {
            cli_println!("        {hint}");
        }
    }
    if doctor::worst(&findings) == Level::Error {
        anyhow::bail!("doctor found errors");
    }
    Ok(())
}
