//! `workshop` command handlers.

use crate::cli::*;
use crate::commands::common::*;

use anyhow::{Result, bail};
use libgaki::core::models::WorkshopItem;
use libgaki::ops::workshop;
use libgaki::steam_client::SteamClient;

/// `workshop browse`: search/browse a game's Workshop to discover items.
pub(crate) async fn cmd_workshop_browse(
    app_id: u32,
    search: Option<String>,
    sort: WorkshopSort,
    count: u32,
    cursor: String,
    tags: Vec<String>,
    json: bool,
) -> Result<()> {
    let client = authed_client().await?;
    let query = workshop::Query {
        app_id,
        search,
        sort: sort.into(),
        cursor: cursor.clone(),
        count,
        tags,
    };
    let page = workshop::browse(&client, &query).await?;

    if json {
        print_json(&serde_json::json!({
            "app_id": app_id,
            "total": page.total,
            "next_cursor": page.next_cursor,
            "items": page.items,
        }));
        return Ok(());
    }

    if page.items.is_empty() {
        cli_println!("No Workshop items found.");
        return Ok(());
    }
    cli_println!("{:>12}  {:>10}  TITLE", "ID", "SIZE");
    for item in &page.items {
        let size = if item.file_size > 0 {
            human_bytes(item.file_size)
        } else {
            "-".to_string()
        };
        cli_println!("{:>12}  {:>10}  {}", item.id, size, item.title);
    }
    cli_println!(
        "\nShowing {} of {} result(s).",
        page.items.len(),
        page.total
    );
    // Offer the next page only when the cursor actually advances.
    if !page.next_cursor.is_empty() && page.next_cursor != cursor {
        cli_println!("Next page: --cursor \"{}\"", page.next_cursor);
    }
    Ok(())
}

/// `workshop info`: show metadata for one or more published files.
pub(crate) async fn cmd_workshop_info(ids: Vec<u64>, json: bool) -> Result<()> {
    let client = authed_client().await?;
    let items = workshop::details(&client, &ids).await?;

    if json {
        print_json(&items);
        return Ok(());
    }
    if items.is_empty() {
        cli_println!("No Workshop items found.");
        return Ok(());
    }
    for item in &items {
        cli_println!("ID      : {}", item.id);
        cli_println!("Title   : {}", item.title);
        cli_println!("App     : {}", item.app_id);
        cli_println!("Type    : {}", workshop::kind_name(item.kind));
        if item.kind == libgaki::core::models::WorkshopItemKind::Collection {
            cli_println!("Items   : {}", item.children.len());
        } else {
            cli_println!("Size    : {}", human_bytes(item.file_size));
            cli_println!("Manifest: {}", item.hcontent_file);
        }
        cli_println!(
            "Updated : {}",
            format_unix_timestamp(item.time_updated.max(0) as u64)
        );
        cli_println!();
    }
    Ok(())
}

/// `workshop list`: the items you're subscribed to for a game.
pub(crate) async fn cmd_workshop_list(app_id: u32, json: bool) -> Result<()> {
    let client = authed_client().await?;
    let items = workshop::subscribed(&client, app_id).await?;

    if json {
        print_json(&items);
        return Ok(());
    }
    if items.is_empty() {
        cli_println!("No subscribed Workshop items for app {app_id}.");
        return Ok(());
    }
    cli_println!("{:>12}  {:>12}  TITLE", "ID", "SIZE");
    for item in &items {
        cli_println!(
            "{:>12}  {:>12}  {}",
            item.id,
            human_bytes(item.file_size),
            item.title
        );
    }
    cli_println!("\n{} subscribed item(s).", items.len());
    Ok(())
}

/// Connect, expand collections, resolve metadata.
async fn resolve_workshop_items(
    ids: Vec<u64>,
    no_recurse: bool,
) -> Result<(SteamClient, Vec<WorkshopItem>)> {
    let client = authed_client().await?;
    let items = workshop::resolve(&client, ids, !no_recurse).await?;
    Ok((client, items))
}

/// Run `op` per item, reporting progress.
async fn for_each_workshop_item<'a, F, Fut>(
    client: &'a SteamClient,
    items: &'a [WorkshopItem],
    past_tense: &str,
    json: bool,
    op: F,
) -> Result<Vec<u64>>
where
    F: Fn(&'a SteamClient, &'a WorkshopItem) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    let mut acted = Vec::new();
    for item in items {
        op(client, item).await?;
        acted.push(item.id);
        if !json {
            cli_println!("{past_tense} Workshop item {} ({}).", item.id, item.title);
        }
    }
    Ok(acted)
}

/// Download one item; drive progress.
async fn install_one_workshop_item(
    client: &SteamClient,
    item: &libgaki::core::models::WorkshopItem,
    json: bool,
) -> Result<()> {
    workshop::install(client, item, |rx| drive_progress_until(rx, json)).await
}

/// `workshop install`: download item(s)/collection(s) and register them.
pub(crate) async fn cmd_workshop_install(
    ids: Vec<u64>,
    no_recurse: bool,
    json: bool,
) -> Result<()> {
    let (client, items) = resolve_workshop_items(ids, no_recurse).await?;
    if items.is_empty() {
        bail!("no installable Workshop items resolved");
    }

    for item in &items {
        if !json {
            cli_println!("Installing Workshop item {} ({}) ...", item.id, item.title);
        }
        install_one_workshop_item(&client, item, json).await?;
        if json {
            print_result_line(serde_json::json!({
                "id": item.id,
                "app_id": item.app_id,
                "status": "installed",
            }));
        }
    }
    Ok(())
}

/// `workshop uninstall`: remove installed item(s)/collection(s).
pub(crate) async fn cmd_workshop_uninstall(
    ids: Vec<u64>,
    no_recurse: bool,
    json: bool,
) -> Result<()> {
    // GetDetails resolves each item's owning app id (needed to find its files).
    let (client, items) = resolve_workshop_items(ids, no_recurse).await?;

    let removed =
        for_each_workshop_item(&client, &items, "Uninstalled", json, workshop::uninstall).await?;
    if json {
        print_json(&serde_json::json!({ "uninstalled": removed }));
    } else if removed.is_empty() {
        cli_println!("Nothing to uninstall.");
    }
    Ok(())
}

/// `workshop subscribe`: subscribe (and optionally install) item(s)/collection(s).
pub(crate) async fn cmd_workshop_subscribe(
    ids: Vec<u64>,
    install: bool,
    no_recurse: bool,
    json: bool,
) -> Result<()> {
    let (client, items) = resolve_workshop_items(ids, no_recurse).await?;

    let subscribed =
        for_each_workshop_item(&client, &items, "Subscribed to", json, workshop::subscribe).await?;

    if install {
        for item in &items {
            if !json {
                cli_println!("Installing Workshop item {} ...", item.id);
            }
            install_one_workshop_item(&client, item, json).await?;
        }
    }

    if json {
        print_json(&serde_json::json!({ "subscribed": subscribed, "installed": install }));
    }
    Ok(())
}

/// `workshop unsubscribe`: unsubscribe from item(s)/collection(s).
pub(crate) async fn cmd_workshop_unsubscribe(
    ids: Vec<u64>,
    no_recurse: bool,
    json: bool,
) -> Result<()> {
    let (client, items) = resolve_workshop_items(ids, no_recurse).await?;

    let unsubscribed = for_each_workshop_item(
        &client,
        &items,
        "Unsubscribed from",
        json,
        workshop::unsubscribe,
    )
    .await?;
    if json {
        print_json(&serde_json::json!({ "unsubscribed": unsubscribed }));
    }
    Ok(())
}

/// `workshop status`: installed vs subscribed (with update detection) for a game.
pub(crate) async fn cmd_workshop_status(app_id: u32, json: bool) -> Result<()> {
    let client = authed_client().await?;
    let rows = workshop::status(&client, app_id).await?;

    if json {
        let arr: Vec<_> = rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "id": r.id,
                    "title": r.title,
                    "installed": r.installed,
                    "subscribed": r.subscribed,
                    "update_available": r.update_available,
                })
            })
            .collect();
        print_json(&serde_json::json!({ "app_id": app_id, "items": arr }));
        return Ok(());
    }

    if rows.is_empty() {
        cli_println!("No installed or subscribed Workshop items for app {app_id}.");
        return Ok(());
    }
    cli_println!(
        "{:>12}  {:<9}  {:<10}  {:<7}  TITLE",
        "ID",
        "INSTALLED",
        "SUBSCRIBED",
        "UPDATE"
    );
    for r in &rows {
        cli_println!(
            "{:>12}  {:<9}  {:<10}  {:<7}  {}",
            r.id,
            if r.installed { "yes" } else { "no" },
            if r.subscribed { "yes" } else { "no" },
            if r.update_available { "yes" } else { "-" },
            r.title,
        );
    }
    Ok(())
}

/// `workshop rate`: thumbs-up/down a Workshop item.
pub(crate) async fn cmd_workshop_rate(id: u64, up: bool, json: bool) -> Result<()> {
    let client = authed_client().await?;
    workshop::rate(&client, id, up).await?;
    if json {
        print_json(&serde_json::json!({
            "id": id,
            "vote": if up { "up" } else { "down" },
            "status": "rated",
        }));
    } else {
        cli_println!(
            "Rated Workshop item {id} {}.",
            if up { "thumbs-up" } else { "thumbs-down" }
        );
    }
    Ok(())
}

/// `workshop comments`: read a page of a Workshop item's comments.
pub(crate) async fn cmd_workshop_comments_read(
    id: u64,
    start: i32,
    count: i32,
    json: bool,
) -> Result<()> {
    let client = authed_client().await?;
    let comments = workshop::comments(&client, id, start, count).await?;

    if json {
        print_json(&serde_json::json!({ "id": id, "comments": comments }));
        return Ok(());
    }
    if comments.is_empty() {
        cli_println!("No comments on Workshop item {id}.");
        return Ok(());
    }
    for c in &comments {
        cli_println!(
            "[{}] {} (+{})",
            format_unix_timestamp(c.timestamp.max(0) as u64),
            c.author,
            c.upvotes
        );
        cli_println!("  {}", c.text);
    }
    cli_println!("\n{} comment(s).", comments.len());
    Ok(())
}

/// `workshop comment`: post a comment to a Workshop item.
pub(crate) async fn cmd_workshop_comment(id: u64, text: String, json: bool) -> Result<()> {
    let client = authed_client().await?;
    let comment_id = workshop::post_comment(&client, id, &text).await?;
    if json {
        print_json(&serde_json::json!({
            "id": id,
            "comment_id": comment_id,
            "status": "posted",
        }));
    } else {
        cli_println!("Posted comment {comment_id} to Workshop item {id}.");
    }
    Ok(())
}
