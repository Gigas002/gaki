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
    json: bool,
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

    if json {
        print_json(&serde_json::json!({
            "app_id": app_id,
            "name": name,
            "regions": quotes,
        }));
        return Ok(());
    }

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

/// Compact JSON row for listings.
fn store_row_json(a: &StoreAppInfo) -> serde_json::Value {
    serde_json::json!({
        "app_id": a.app_id,
        "name": a.name,
        "type": a.app_type,
        "is_free": a.is_free,
        "price": a.price,
        "price_cents": a.price_cents,
        "original_price": a.original_price,
        "discount_pct": a.discount_pct,
        "discount_end_date": a.discount_end.map(|t| unix_to_ymd(t as i64)),
        "release_date": a.release_date,
        "platforms": a.platforms,
        "reviews": a.review_summary,
        "country": a.country,
    })
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
pub(crate) async fn cmd_search(
    term: String,
    count: u32,
    country: Option<String>,
    json: bool,
) -> Result<()> {
    let country = store::resolve_country(country.as_deref())?;
    let client = authed_client().await?;
    let Listing {
        total,
        suggestions,
        records,
    } = store::search(&client, &term, count, &country).await?;
    if json {
        print_json(&serde_json::json!({
            "term": term,
            "total": total,
            "suggestions": suggestions,
            "results": records.iter().map(store_row_json).collect::<Vec<_>>(),
        }));
        return Ok(());
    }
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
    json: bool,
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
    if json {
        print_json(&serde_json::json!({
            "country": country,
            "scope": format!("{scope:?}"),
            "start": start,
            "total": total,
            "results": records.iter().map(store_row_json).collect::<Vec<_>>(),
        }));
        return Ok(());
    }
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
pub(crate) async fn cmd_similar(
    app_id: u32,
    count: i32,
    country: Option<String>,
    json: bool,
) -> Result<()> {
    let country = store::resolve_country(country.as_deref())?;
    let client = authed_client().await?;
    let records = store::similar(&client, app_id, count, &country)
        .await?
        .records;
    if json {
        print_json(&serde_json::json!({
            "app_id": app_id,
            "results": records.iter().map(store_row_json).collect::<Vec<_>>(),
        }));
        return Ok(());
    }
    if records.is_empty() {
        cli_println!("No similar games reported for app {app_id}.");
        return Ok(());
    }
    print_store_table(&records);
    Ok(())
}

/// `gaki-cli players APPID`: current in-game count.
pub(crate) async fn cmd_players(app_id: u32, json: bool) -> Result<()> {
    let client = authed_client().await?;
    let players = store::players(&client, app_id).await?;
    if json {
        print_json(&serde_json::json!({ "app_id": app_id, "players": players }));
    } else {
        cli_println!("{players} players in-game (app {app_id})");
    }
    Ok(())
}

/// `gaki-cli events`: active store sales and events.
pub(crate) async fn cmd_events(country: Option<String>, json: bool) -> Result<()> {
    let country = store::resolve_country(country.as_deref())?;
    let client = authed_client().await?;
    let events = store::events(&client, &country).await?;
    if json {
        print_json(&serde_json::json!({ "country": country, "events": events }));
        return Ok(());
    }
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
pub(crate) async fn cmd_news(app_id: u32, count: u32, max_length: u32, json: bool) -> Result<()> {
    let items = store::news(app_id, count, max_length).await?;
    if json {
        print_json(&serde_json::json!({ "app_id": app_id, "news": items }));
        return Ok(());
    }
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
pub(crate) async fn cmd_reviews(args: crate::cli::ReviewsArgs, json: bool) -> Result<()> {
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
    if json {
        print_json(&serde_json::json!({
            "app_id": app_id,
            "summary": page.summary,
            "reviews": page.reviews,
            "next_cursor": page.next_cursor,
        }));
        return Ok(());
    }
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
    json: bool,
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

    if json {
        let items: Vec<serde_json::Value> = page
            .iter()
            .map(|(e, record)| {
                let mut v = record
                    .as_ref()
                    .map(store_row_json)
                    .unwrap_or_else(|| serde_json::json!({ "app_id": e.app_id, "name": null }));
                v["priority"] = e.priority.into();
                v["date_added"] = e.date_added.into();
                v["date_added_date"] = unix_to_ymd(e.date_added as i64).into();
                v
            })
            .collect();
        print_json(&serde_json::json!({
            "steam_id": steam_id,
            "total": total,
            "offset": offset,
            "items": items,
        }));
        return Ok(());
    }

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
pub(crate) async fn cmd_wishlist_add(app_id: u32, json: bool) -> Result<()> {
    let client = authed_client().await?;
    let count = store::wishlist_add(&client, app_id).await?;
    if json {
        print_json(
            &serde_json::json!({ "app_id": app_id, "status": "added", "wishlist_count": count }),
        );
    } else {
        cli_println!("Added app {app_id} to your wishlist ({count} items).");
    }
    Ok(())
}

/// `gaki-cli wishlist remove APPID`.
pub(crate) async fn cmd_wishlist_remove(app_id: u32, json: bool) -> Result<()> {
    let client = authed_client().await?;
    let count = store::wishlist_remove(&client, app_id).await?;
    if json {
        print_json(
            &serde_json::json!({ "app_id": app_id, "status": "removed", "wishlist_count": count }),
        );
    } else {
        cli_println!("Removed app {app_id} from your wishlist ({count} items).");
    }
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{cut}…")
}
