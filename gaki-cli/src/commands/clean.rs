//! `gaki-cli clean`: list gaki's caches and leftovers, then delete them.

use anyhow::Result;
use libgaki::core::utils::human_bytes;
use libgaki::ops::clean::{self, Kind, Selection};

use crate::commands::common::{confirm_write, print_json};

/// The kinds the subset flags picked, in order.
pub(crate) struct CleanFlags {
    pub(crate) images: bool,
    pub(crate) info: bool,
    pub(crate) library: bool,
    pub(crate) tui: bool,
    pub(crate) logs: bool,
    pub(crate) locks: bool,
    pub(crate) shaders: bool,
    pub(crate) all_shaders: bool,
    pub(crate) dry_run: bool,
    pub(crate) yes: bool,
}

impl CleanFlags {
    fn selection(&self) -> Selection {
        let picked = [
            (self.images, Kind::Images),
            (self.info, Kind::Info),
            (self.library, Kind::Library),
            (self.tui, Kind::Tui),
            (self.logs, Kind::Logs),
            (self.locks, Kind::Locks),
            (self.shaders || self.all_shaders, Kind::Shaders),
        ];
        let any_subset = [
            self.images,
            self.info,
            self.library,
            self.tui,
            self.logs,
            self.locks,
            self.shaders,
        ]
        .contains(&true);
        Selection {
            // `--all-shaders` alone widens the default; with subset flags it
            // also picks the shader caches.
            kinds: if any_subset {
                picked
                    .into_iter()
                    .filter_map(|(on, kind)| on.then_some(kind))
                    .collect()
            } else {
                Vec::new()
            },
            all_shaders: self.all_shaders,
        }
    }
}

pub(crate) async fn cmd_clean(flags: CleanFlags, json: bool) -> Result<()> {
    let items = clean::plan(&flags.selection()).await?;
    let total: u64 = items.iter().map(|i| i.bytes).sum();

    if items.is_empty() {
        if json {
            print_json(
                &serde_json::json!({ "items": [], "freed_bytes": 0, "dry_run": flags.dry_run }),
            );
        } else {
            cli_println!("Nothing to clean.");
        }
        return Ok(());
    }
    if !json {
        for (kind, (count, bytes)) in clean::totals(&items) {
            cli_println!(
                "{:<8} {:>10}  {count} item(s)",
                kind.as_str(),
                human_bytes(bytes)
            );
        }
        cli_println!("{:<8} {:>10}", "total", human_bytes(total));
    }
    if flags.dry_run {
        if json {
            print_json(&serde_json::json!({ "items": items, "freed_bytes": 0, "dry_run": true }));
        }
        return Ok(());
    }
    confirm_write(
        "clean",
        &format!("Delete these ({})? [y/N] ", human_bytes(total)),
        flags.yes,
        json,
    )?;
    let report = clean::delete(&items);
    if json {
        print_json(&serde_json::json!({
            "items": report.deleted,
            "freed_bytes": report.freed_bytes,
            "failed": report
                .failed
                .iter()
                .map(|(path, error)| serde_json::json!({ "path": path, "error": error }))
                .collect::<Vec<_>>(),
            "dry_run": false,
        }));
    } else {
        cli_println!("Freed {}.", human_bytes(report.freed_bytes));
        for (path, error) in &report.failed {
            cli_eprintln!("  couldn't delete {}: {error}", path.display());
        }
    }
    if report.failed.is_empty() {
        Ok(())
    } else {
        anyhow::bail!("{} item(s) couldn't be deleted", report.failed.len())
    }
}
