//! `depots files` / `depots download`: the depot browser.

use crate::cli::{DepotManifestArgs, DepotsCommand};
use crate::commands::common::*;

use anyhow::Result;
use libgaki::ops::depot::{self, EntryKind, FileDownload, ManifestRef};

fn manifest_ref(args: &DepotManifestArgs) -> ManifestRef {
    ManifestRef::from_options(args.manifest, args.branch.as_deref())
}

pub(crate) async fn cmd_depots_command(command: DepotsCommand, json: bool) -> Result<()> {
    match command {
        DepotsCommand::Files {
            app_id,
            depot_id,
            manifest,
            filter,
        } => cmd_depot_files(app_id, depot_id, &manifest, filter.as_deref(), json).await,
        DepotsCommand::Download {
            app_id,
            depot_id,
            path,
            manifest,
            output,
            force,
        } => {
            let request = FileDownload {
                app_id,
                depot_id,
                manifest: manifest_ref(&manifest),
                path,
                // Relative to where the command was run, not the daemon.
                out_dir: crate::output::user_path(&output.unwrap_or_else(|| ".".into())),
                force,
            };
            cmd_depot_download(&request, json).await
        }
    }
}

async fn cmd_depot_files(
    app_id: u32,
    depot_id: u32,
    manifest: &DepotManifestArgs,
    filter: Option<&str>,
    json: bool,
) -> Result<()> {
    let client = authed_client().await?;
    let listing = depot::files(&client, app_id, depot_id, &manifest_ref(manifest), filter).await?;

    if json {
        print_json(&serde_json::to_value(&listing)?);
        return Ok(());
    }

    cli_println!(
        "App {app_id}, depot {depot_id}, manifest {}",
        listing.manifest_id
    );
    if listing.files.is_empty() {
        cli_println!(
            "{}",
            if filter.is_some() {
                "No entries match the filter."
            } else {
                "The manifest is empty."
            }
        );
        return Ok(());
    }
    cli_println!("{:>12}  PATH", "SIZE");
    for f in &listing.files {
        let size = match f.kind {
            EntryKind::Directory => "<dir>".to_string(),
            EntryKind::Symlink => "<link>".to_string(),
            EntryKind::File => human_bytes(f.size),
        };
        let suffix = match (&f.kind, &f.link_target) {
            (EntryKind::Directory, _) => "/".to_string(),
            (EntryKind::Symlink, Some(target)) => format!(" -> {target}"),
            (EntryKind::File, _) if f.executable => "*".to_string(),
            _ => String::new(),
        };
        cli_println!("{size:>12}  {}{suffix}", f.path);
    }
    let file_count = listing
        .files
        .iter()
        .filter(|f| f.kind == EntryKind::File)
        .count();
    cli_println!(
        "\n{file_count} files, {}",
        human_bytes(listing.total_size())
    );
    Ok(())
}

async fn cmd_depot_download(request: &FileDownload, json: bool) -> Result<()> {
    let client = authed_client().await?;
    if !json {
        cli_eprintln!(
            "Downloading {} from depot {}…",
            request.path,
            request.depot_id
        );
    }
    let done = depot::download(&client, request).await?;

    if json {
        print_json(&serde_json::json!({
            "app_id": request.app_id,
            "depot_id": request.depot_id,
            "manifest_id": done.manifest_id,
            "file": request.path,
            "path": done.path,
            "size": done.size,
        }));
        return Ok(());
    }
    cli_println!(
        "Saved {} ({}, manifest {}).",
        done.path.display(),
        human_bytes(done.size),
        done.manifest_id
    );
    Ok(())
}
