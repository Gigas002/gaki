//! Store price lookups (`price`).

use crate::commands::common::*;

use anyhow::Result;
use libgaki::ops::store::{self, Listing};
use libgaki::steam_client::{StoreAppInfo, unix_to_ymd};

/// `gaki-cli price APPID [--compare CC,…]`.
pub(crate) async fn cmd_price(
    app_id: u32,
    compare: Vec<String>,
    country: Option<String>,
) -> Result<()> {
    let mut regions = store::parse_regions(&compare)?;
    if regions.is_empty() {
        regions.push(store::resolve_country(country.as_deref())?);
    }

    let client = authed_client().await?;
    let store::Prices {
        name,
        regions: quotes,
    } = store::prices(&client, app_id, &regions).await?;

    cli_println!("{name}  (app {app_id})");
    cli_println!(
        "{:<8} {:<14} {:<9} {:<14} {}",
        "Country",
        "Price",
        "Discount",
        "Original",
        "Ends"
    );
    for q in &quotes {
        if !q.available {
            let why = q.error.as_deref().unwrap_or("not sold in this region");
            cli_println!("{:<8} {why}", q.country);
            continue;
        }
        let price = q.price.clone().unwrap_or_else(|| "Free".to_string());
        let discount = if q.discount_pct > 0 {
            format!("-{}%", q.discount_pct)
        } else {
            "-".to_string()
        };
        let original = q.original_price.clone().unwrap_or_else(|| "-".to_string());
        let ends = q
            .discount_end_date
            .clone()
            .unwrap_or_else(|| "-".to_string());
        cli_println!(
            "{:<8} {:<14} {:<9} {:<14} {ends}",
            q.country,
            price,
            discount,
            original
        );
    }

    // Packages and bundles, for the first priced region.
    if let Some(q) = quotes
        .iter()
        .find(|q| q.available && q.purchase_options.len() > 1)
    {
        cli_println!("\nPurchase options [{}]:", q.country);
        for o in &q.purchase_options {
            let price = o.price.clone().unwrap_or_else(|| "-".to_string());
            let mut note = String::new();
            if o.discount_pct > 0 {
                note.push_str(&format!("  -{}%", o.discount_pct));
            }
            if o.bundle_discount_pct > 0 {
                note.push_str(&format!(" (bundle -{}%)", o.bundle_discount_pct));
            }
            if o.included_games > 1 {
                note.push_str(&format!("  {} items", o.included_games));
            }
            if let Some(end) = o.discount_end {
                note.push_str(&format!("  until {}", unix_to_ymd(end as i64)));
            }
            cli_println!(
                "  {:<8} {:<44} {:<12}{note}",
                o.kind,
                truncate(&o.name, 44),
                price
            );
        }
    }
    Ok(())
}

/// Human table for listings.
fn print_store_table(records: &[StoreAppInfo]) {
    cli_println!(
        "{:>9}  {:<14} {:<9} {:<11} NAME",
        "APPID",
        "PRICE",
        "DISCOUNT",
        "ENDS"
    );
    for a in records {
        let price = a.price.clone().unwrap_or_else(|| "-".to_string());
        let discount = if a.discount_pct > 0 {
            format!("-{}%", a.discount_pct)
        } else {
            "-".to_string()
        };
        let ends = a
            .discount_end
            .map(|t| unix_to_ymd(t as i64))
            .unwrap_or_else(|| "-".to_string());
        let kind = if a.app_type.is_empty() || a.app_type == "Game" {
            String::new()
        } else {
            format!("  [{}]", a.app_type)
        };
        cli_println!(
            "{:>9}  {:<14} {:<9} {:<11} {}{kind}",
            a.app_id,
            price,
            discount,
            ends,
            a.name
        );
    }
}

/// `gaki-cli search TERM`: find app ids by title.
pub(crate) async fn cmd_search(term: String, count: u32, country: Option<String>) -> Result<()> {
    let country = store::resolve_country(country.as_deref())?;
    let client = authed_client().await?;
    let Listing {
        suggestions,
        records,
        ..
    } = store::search(&client, &term, count, &country).await?;
    if records.is_empty() {
        cli_println!("No store results for \"{term}\".");
        if !suggestions.is_empty() {
            cli_println!("Did you mean: {}", suggestions.join(", "));
        }
        return Ok(());
    }
    print_store_table(&records);
    Ok(())
}

/// `gaki-cli deals`: discounted and top-selling games.
pub(crate) async fn cmd_deals(
    scope: crate::cli::DealsScopeArg,
    min_discount: i32,
    count: i32,
    start: i32,
    country: Option<String>,
) -> Result<()> {
    use libgaki::steam_client::DealsScope;
    let country = store::resolve_country(country.as_deref())?;
    let scope = match scope {
        crate::cli::DealsScopeArg::DiscountedTopSellers => DealsScope::DiscountedTopSellers,
        crate::cli::DealsScopeArg::TopSellers => DealsScope::TopSellers,
        crate::cli::DealsScopeArg::Specials => DealsScope::Specials,
    };
    let client = authed_client().await?;
    let query = store::DealsQuery {
        scope,
        min_discount,
        start,
        count,
    };
    let Listing { total, records, .. } = store::deals(&client, query, &country).await?;
    if records.is_empty() {
        cli_println!("No deals matched in {country}.");
        return Ok(());
    }
    cli_println!(
        "{scope:?} in {country} ({} matching, showing from {start}):",
        total
    );
    print_store_table(&records);
    Ok(())
}

/// `gaki-cli similar APPID`: related games.
pub(crate) async fn cmd_similar(app_id: u32, count: i32, country: Option<String>) -> Result<()> {
    let country = store::resolve_country(country.as_deref())?;
    let client = authed_client().await?;
    let records = store::similar(&client, app_id, count, &country)
        .await?
        .records;
    if records.is_empty() {
        cli_println!("No similar games reported for app {app_id}.");
        return Ok(());
    }
    print_store_table(&records);
    Ok(())
}

/// `gaki-cli players APPID`: current in-game count.
pub(crate) async fn cmd_players(app_id: u32) -> Result<()> {
    let client = authed_client().await?;
    let players = store::players(&client, app_id).await?;
    cli_println!("{players} players in-game (app {app_id})");

    Ok(())
}

/// `gaki-cli events`: active store sales and events.
pub(crate) async fn cmd_events(country: Option<String>) -> Result<()> {
    let country = store::resolve_country(country.as_deref())?;
    let client = authed_client().await?;
    let events = store::events(&client, &country).await?;
    if events.is_empty() {
        cli_println!("No active store events for {country}.");
        return Ok(());
    }
    cli_println!("{:<12} {:<11} {:<11} TITLE", "TYPE", "START", "END");
    for e in &events {
        let start = if e.start > 0 {
            unix_to_ymd(e.start as i64)
        } else {
            "-".to_string()
        };
        let end = if e.end > 0 {
            unix_to_ymd(e.end as i64)
        } else {
            "-".to_string()
        };
        let assoc = if e.associated_name.is_empty() {
            String::new()
        } else {
            format!("  ({})", e.associated_name)
        };
        cli_println!("{:<12} {start:<11} {end:<11} {}{assoc}", e.kind, e.title);
    }
    Ok(())
}

/// `gaki-cli news APPID`: announcements from the storefront feed.
pub(crate) async fn cmd_news(app_id: u32, count: u32, max_length: u32) -> Result<()> {
    let items = store::news(app_id, count, max_length).await?;
    if items.is_empty() {
        cli_println!("No news for app {app_id}.");
        return Ok(());
    }
    for n in &items {
        cli_println!("{}  {}  [{}]", unix_to_ymd(n.date as i64), n.title, n.feed);
        cli_println!("  {}", n.url);
        if !n.contents.is_empty() {
            cli_println!("  {}", n.contents.replace('\n', " "));
        }
        cli_println!();
    }
    Ok(())
}

/// `gaki-cli reviews APPID`: one page of user reviews.
pub(crate) async fn cmd_reviews(args: crate::cli::ReviewsArgs) -> Result<()> {
    let crate::cli::ReviewsArgs {
        app_id,
        filter,
        review_type,
        purchase,
        count,
        cursor,
    } = args;
    use libgaki::web::discovery::ReviewQuery;
    let query = ReviewQuery {
        filter,
        review_type,
        purchase_type: purchase,
        count,
        cursor,
    };
    let page = store::reviews(app_id, &query).await?;
    // Steam omits totals unless `--filter all`.
    let s = &page.summary;
    if s.total_reviews > 0 {
        cli_println!(
            "{} ({}% positive, {} reviews: {} up / {} down)",
            s.label,
            s.total_positive * 100 / s.total_reviews,
            s.total_reviews,
            s.total_positive,
            s.total_negative
        );
    }
    for r in &page.reviews {
        let verdict = if r.voted_up { "👍" } else { "👎" };
        cli_println!(
            "\n{verdict} {}  {}  {:.1}h played  {} helpful",
            unix_to_ymd(r.created as i64),
            r.author,
            r.playtime_hours,
            r.votes_up
        );
        for line in r.text.lines().take(6) {
            cli_println!("   {}", truncate(line, 110));
        }
    }
    if let Some(c) = &page.next_cursor {
        cli_println!("\nNext page: --cursor '{c}'");
    }
    Ok(())
}

/// `gaki-cli wishlist [USER]`: a wishlist with store records.
pub(crate) async fn cmd_wishlist(
    user: Option<String>,
    count: usize,
    offset: usize,
    country: Option<String>,
) -> Result<()> {
    let country = store::resolve_country(country.as_deref())?;
    let client = authed_client().await?;
    let target = match &user {
        Some(u) => Some(resolve_user_ident(&client, u).await?),
        None => None,
    };
    let store::Wishlist {
        steam_id,
        total,
        items: page,
    } = store::wishlist(&client, target, offset, count, &country).await?;

    if total == 0 {
        cli_println!("Your wishlist is empty.");
        return Ok(());
    }
    cli_println!(
        "Wishlist of {steam_id}: {total} item(s), showing {}–{}",
        offset + 1,
        offset + page.len()
    );
    cli_println!(
        "{:>4}  {:>9}  {:<14} {:<9} {:<11} NAME",
        "#",
        "APPID",
        "PRICE",
        "DISCOUNT",
        "ADDED"
    );
    for (e, record) in &page {
        let rank = if e.priority > 0 {
            e.priority.to_string()
        } else {
            "-".to_string()
        };
        let (name, price, discount) = match record {
            Some(r) => (
                r.name.clone(),
                r.price.clone().unwrap_or_else(|| "-".to_string()),
                if r.discount_pct > 0 {
                    format!("-{}%", r.discount_pct)
                } else {
                    "-".to_string()
                },
            ),
            None => (
                "(not on the store)".to_string(),
                "-".to_string(),
                "-".to_string(),
            ),
        };
        cli_println!(
            "{rank:>4}  {:>9}  {price:<14} {discount:<9} {:<11} {name}",
            e.app_id,
            unix_to_ymd(e.date_added as i64)
        );
    }
    Ok(())
}

/// `gaki-cli wishlist add APPID`.
pub(crate) async fn cmd_wishlist_add(app_id: u32) -> Result<()> {
    let client = authed_client().await?;
    let count = store::wishlist_add(&client, app_id).await?;
    cli_println!("Added app {app_id} to your wishlist ({count} items).");

    Ok(())
}

/// `gaki-cli wishlist remove APPID`.
pub(crate) async fn cmd_wishlist_remove(app_id: u32) -> Result<()> {
    let client = authed_client().await?;
    let count = store::wishlist_remove(&client, app_id).await?;
    cli_println!("Removed app {app_id} from your wishlist ({count} items).");

    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{cut}…")
}

/// The draft `gaki-cli review` flags make, over `existing` when there is
/// one. `text` is the new text, when one was given.
pub(crate) fn review_draft(
    args: &crate::cli::ReviewArgs,
    existing: Option<&store::OwnReview>,
    text: Option<String>,
) -> Result<store::ReviewDraft> {
    let pick = |on: bool, off: bool, current: Option<bool>, new: bool| {
        if on {
            true
        } else if off {
            false
        } else {
            current.unwrap_or(new)
        }
    };
    let voted_up = match (args.up, args.down, existing) {
        (true, _, _) => true,
        (_, true, _) => false,
        (_, _, Some(review)) => review.voted_up,
        (_, _, None) => anyhow::bail!("a new review needs --up or --down"),
    };
    let current = existing.map(store::OwnReview::draft);
    let current = current.as_ref();
    let text = text
        .or_else(|| current.map(|d| d.text.clone()))
        .unwrap_or_default();
    Ok(store::ReviewDraft {
        text,
        voted_up,
        is_public: pick(
            args.public,
            args.private,
            current.map(|d| d.is_public),
            true,
        ),
        language: args
            .language
            .clone()
            .or_else(|| current.and_then(|d| d.language.clone())),
        received_compensation: pick(
            args.free,
            args.paid,
            current.map(|d| d.received_compensation),
            false,
        ),
        comments_disabled: pick(
            args.no_comments,
            args.comments,
            current.map(|d| d.comments_disabled),
            false,
        ),
    })
}

/// The text from `-m`, `-F` (`-` is stdin), or the editor (`-e`, or a new
/// review) when stdin is a terminal; `None` keeps the current text.
fn review_text(args: &crate::cli::ReviewArgs, current: Option<&str>) -> Result<Option<String>> {
    use std::io::{IsTerminal, Read};
    if let Some(text) = &args.message {
        return Ok(Some(text.clone()));
    }
    if let Some(path) = &args.file {
        let mut text = String::new();
        if path.as_os_str() == "-" {
            std::io::stdin().read_to_string(&mut text)?;
        } else {
            text = std::fs::read_to_string(path)
                .map_err(|e| anyhow::anyhow!("could not read {}: {e}", path.display()))?;
        }
        return Ok(Some(text));
    }
    // Changing only a flag keeps the text.
    if current.is_some() && !args.edit {
        return Ok(None);
    }
    if !std::io::stdin().is_terminal() {
        anyhow::bail!("give the review's text with -m or -F; there's no terminal for an editor");
    }
    edit_text(current.unwrap_or_default()).map(Some)
}

/// Opens `$VISUAL`/`$EDITOR` (or `vi`) on `text` and returns what was saved.
fn edit_text(text: &str) -> Result<String> {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .ok()
        .filter(|e| !e.trim().is_empty())
        .unwrap_or_else(|| "vi".into());
    let path = std::env::temp_dir().join(format!("gaki-review-{}.txt", std::process::id()));
    std::fs::write(&path, text)?;
    let mut words = editor.split_whitespace();
    let program = words.next().unwrap_or("vi");
    let status = std::process::Command::new(program)
        .args(words)
        .arg(&path)
        .status()
        .map_err(|e| anyhow::anyhow!("could not start {program}: {e}"));
    let text = std::fs::read_to_string(&path);
    let _ = std::fs::remove_file(&path);
    if !status?.success() {
        anyhow::bail!("{program} failed; nothing posted");
    }
    Ok(text?)
}

fn print_review(review: &store::OwnReview) {
    let verdict = if review.voted_up {
        "👍 Recommended"
    } else {
        "👎 Not recommended"
    };
    let mut notes = vec![if review.is_public {
        "public"
    } else {
        "friends only"
    }];
    if review.received_compensation {
        notes.push("received for free");
    }
    if review.comments_disabled {
        notes.push("comments off");
    }
    cli_println!(
        "{verdict}  {}  {}{}",
        unix_to_ymd(review.created as i64),
        notes.join(", "),
        if review.updated > review.created {
            format!(", updated {}", unix_to_ymd(review.updated as i64))
        } else {
            String::new()
        }
    );
    cli_println!(
        "{} helpful, {} not, {} funny, {} comments",
        review.votes_up,
        review.votes_down,
        review.votes_funny,
        review.comments
    );
    for line in review.text.lines() {
        cli_println!("   {line}");
    }
}

/// `gaki-cli review APPID [--up|--down] [-m TEXT|-F FILE] …`.
pub(crate) async fn cmd_review(args: crate::cli::ReviewArgs) -> Result<()> {
    let client = authed_client().await?;
    let existing = store::my_review(&client, args.app_id).await?;
    if !args.writes() {
        match &existing {
            Some(review) => print_review(review),
            None => cli_println!(
                "You haven't reviewed app {}. Write one with `gaki-cli review {} --up` or `--down`.",
                args.app_id,
                args.app_id
            ),
        }
        return Ok(());
    }
    // Check the verdict before opening the editor.
    review_draft(&args, existing.as_ref(), Some(String::new()))?;
    let text = review_text(&args, existing.as_ref().map(|r| r.text.as_str()))?;
    let draft = review_draft(&args, existing.as_ref(), text)?;
    if let Some(problem) = draft.check() {
        anyhow::bail!("{problem}; nothing posted");
    }
    let what = if existing.is_some() {
        "Replace your review of"
    } else {
        "Post a review of"
    };
    let verdict = if draft.voted_up {
        "recommended"
    } else {
        "not recommended"
    };
    let who = if draft.is_public {
        "public"
    } else {
        "friends only"
    };
    confirm_write(
        &format!(
            "{what} app {} ({verdict}, {who}, {} characters)? [y/N] ",
            args.app_id,
            draft.text.trim().chars().count()
        ),
        args.yes,
    )?;
    match store::write_review(&client, args.app_id, &draft).await? {
        store::ReviewWritten::Created => cli_println!("Posted your review of app {}.", args.app_id),
        store::ReviewWritten::Updated => {
            cli_println!("Updated your review of app {}.", args.app_id)
        }
    }
    Ok(())
}
