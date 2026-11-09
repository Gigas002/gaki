//! Storefront lookup for `info --extended` requirements.
//!
//! System requirements exist only in `appdetails`;
//! everything else `--extended` shows comes over the CM.

use crate::core::error::{ErrorKind, TypedError};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;

/// Minimum / recommended requirement lines.
#[derive(Debug, Clone, Default)]
pub struct Requirements {
    pub minimum: Vec<String>,
    pub recommended: Vec<String>,
}

#[derive(Deserialize)]
struct Envelope {
    success: bool,
    data: Option<RawRequirements>,
}

#[derive(Deserialize)]
struct RawRequirements {
    // Object `{minimum, recommended}` or empty array.
    #[serde(default)]
    pc_requirements: serde_json::Value,
    #[serde(default)]
    linux_requirements: serde_json::Value,
    #[serde(default)]
    mac_requirements: serde_json::Value,
}

/// Fetch requirements (`filters=` is ignored for these keys).
pub async fn fetch_requirements(
    client: &reqwest::Client,
    app_id: u32,
    country: &str,
) -> Result<Option<Requirements>> {
    let language = crate::core::STEAM_LANGUAGE;
    let cc = country.to_ascii_lowercase();
    let url = format!(
        "https://store.steampowered.com/api/appdetails?appids={app_id}&l={language}&cc={cc}"
    );
    let resp = crate::core::net::send_with_retry(client, client.get(&url))
        .await
        .with_context(|| format!("failed requesting requirements for app {app_id}"))?;
    let map: HashMap<String, Envelope> = resp.json().await.map_err(|e| {
        TypedError::new(
            ErrorKind::SourceChanged,
            format!("failed parsing requirements for app {app_id}: {e}"),
        )
    })?;
    Ok(requirements_from(&map, app_id))
}

/// The requirements in an `appdetails` response, if the app has any.
fn requirements_from(map: &HashMap<String, Envelope>, app_id: u32) -> Option<Requirements> {
    let env = map.get(&app_id.to_string())?;
    let data = env.data.as_ref().filter(|_| env.success)?;

    // Prefer Windows, then Linux, then macOS.
    let block = [
        &data.pc_requirements,
        &data.linux_requirements,
        &data.mac_requirements,
    ]
    .into_iter()
    .find(|v| v.get("minimum").is_some() || v.get("recommended").is_some());
    let lines = |key: &str| {
        block
            .and_then(|req| req.get(key))
            .and_then(|v| v.as_str())
            .map(requirements_lines)
            .unwrap_or_default()
    };
    Some(Requirements {
        minimum: lines("minimum"),
        recommended: lines("recommended"),
    })
}

/// HTML blob → clean "Label: value" lines.
fn requirements_lines(html: &str) -> Vec<String> {
    let normalized = html
        .replace("</li>", "\n")
        .replace("</ul>", "\n")
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n");

    strip_html(&normalized)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| {
            !line.eq_ignore_ascii_case("Minimum:") && !line.eq_ignore_ascii_case("Recommended:")
        })
        .map(String::from)
        .collect()
}

/// Strip tags and decode the entities Steam uses.
pub fn strip_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .replace("\r\n", "\n")
        .trim()
        .to_string()
}

/// Plain text from Steam's store BBCode (`StoreItem.full_description_bbcode`).
///
/// Tags are dropped: `[p]`, `[br]`, headings and list ends become line breaks
/// and `[*]` a bullet. Only lowercase tag names count as tags, so bracketed
/// text such as `[REDACTED]` or `[1]` survives.
pub fn strip_bbcode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let tag = after.find(']').map(|close| &after[..close]);
        match tag.and_then(bbcode_tag_name) {
            Some(name) => {
                match name {
                    "p" | "/p" | "br" | "hr" | "h1" | "/h1" | "h2" | "/h2" | "h3" | "/h3"
                    | "list" | "/list" | "olist" | "/olist" => out.push('\n'),
                    "*" => out.push_str("\n• "),
                    _ => {}
                }
                rest = &after[tag.map_or(0, str::len) + 1..];
            }
            None => {
                out.push('[');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The tag name of `[tag attr=…]` / `[/tag]` contents, if they look like one.
fn bbcode_tag_name(inner: &str) -> Option<&str> {
    let name = inner.split([' ', '=']).next().unwrap_or_default();
    let bare = name.strip_prefix('/').unwrap_or(name);
    let is_tag = bare == "*"
        || (!bare.is_empty()
            && bare.starts_with(|c: char| c.is_ascii_lowercase())
            && bare
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
    is_tag.then_some(name)
}

#[cfg(test)]
mod tests;
