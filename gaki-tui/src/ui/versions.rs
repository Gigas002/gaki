//! The versions & pinning overlay (modal popup): the selected game's per-depot
//! current manifests, its pin state, and a prompt to downgrade the highlighted
//! depot to a specific (usually older) manifest id.

use crate::keymap::{VersionsAction as V, hints, key_of};
use crate::utils::bytes::convert;

use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::browser::Browser;
use crate::ui::theme;
use crate::ui::{paneled_paragraph, selection_marker};

/// Build the versions overlay from the browser's open versions state. Shows the
/// pin banner, install path, one row per depot (name, current manifest, size,
/// and any pinned manifest), and either the downgrade prompt or a status/hint
/// line.
pub fn versions(browser: &Browser) -> Paragraph<'static> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    // Pin banner + install path.
    let pin_span = if browser.is_pinned() {
        Span::styled(
            "PINNED (updates held)".to_string(),
            theme::value().fg(theme::WARN).add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled("not pinned".to_string(), theme::dim())
    };
    lines.push(Line::from(vec![
        Span::styled("Version pin: ".to_string(), theme::label()),
        pin_span,
    ]));
    if let Some(av) = &browser.versions_available
        && let Some(path) = &av.install_path
    {
        lines.push(Line::from(vec![
            Span::styled("Installed at: ".to_string(), theme::label()),
            Span::styled(path.clone(), theme::dim()),
        ]));
    }
    lines.push(Line::from(""));

    // Depot rows.
    if browser.versions_manifests.is_empty() {
        lines.push(Line::from(Span::styled(
            "No depot manifests (the game may not be installed or owned).".to_string(),
            theme::dim(),
        )));
    } else {
        let pinned_map = browser
            .versions_available
            .as_ref()
            .map(|a| &a.pinned_manifests);
        for (i, m) in browser.versions_manifests.iter().enumerate() {
            let selected = i == browser.versions_index;
            let style = if selected {
                theme::selection(theme::ACCENT)
            } else {
                theme::value()
            };
            let name = match m.depot_name.as_deref() {
                Some(n) if !n.is_empty() => format!("{} ({})", n, m.depot_id),
                _ => m.depot_id.to_string(),
            };
            // Note if this depot is pinned, and to which manifest.
            let pin_note = pinned_map
                .and_then(|pm| pm.get(&m.depot_id.to_string()))
                .map(|mid| {
                    if *mid == m.manifest_id {
                        " · pinned".to_string()
                    } else {
                        format!(" · pinned {mid}")
                    }
                })
                .unwrap_or_default();
            lines.push(Line::from(vec![
                Span::styled(selection_marker(selected).to_string(), style),
                Span::styled(name, style),
                Span::raw("  ".to_string()),
                Span::styled(format!("manifest {}", m.manifest_id), theme::dim()),
                Span::raw("  ".to_string()),
                Span::styled(convert(m.size as f64), theme::dim()),
                Span::styled(pin_note, theme::value().fg(theme::WARN)),
            ]));
        }
    }

    lines.push(Line::from(""));

    // Downgrade prompt or status/hint line.
    if let Some(input) = &browser.versions_input {
        lines.push(Line::from(vec![
            Span::styled(
                "Downgrade highlighted depot to manifest id: ".to_string(),
                theme::label(),
            ),
            Span::styled(input.clone(), theme::accent()),
            Span::styled("_".to_string(), theme::key()),
        ]));
        lines.push(Line::from(Span::styled(
            "Enter to start · Esc to cancel".to_string(),
            theme::dim(),
        )));
        if let Some(m) = browser.selected_manifest() {
            lines.push(Line::from(Span::styled(
                format!(
                    "Older ids: {}",
                    libgaki::core::steam_urls::steamdb_depot_url(m.depot_id)
                ),
                theme::dim(),
            )));
        }
    } else {
        let status = if !browser.versions_status.is_empty() {
            browser.versions_status.clone()
        } else if let Some(busy) = browser.downgrade_busy_text() {
            busy
        } else {
            hints(&[
                (key_of(V::Pin), "pin"),
                (key_of(V::Unpin), "unpin"),
                (key_of(V::Downgrade), "downgrade selected"),
                (key_of(V::SteamDb), "SteamDB"),
                ("Esc".to_string(), "close"),
            ])
        };
        lines.push(Line::from(Span::styled(status, theme::dim())));
    }

    paneled_paragraph(lines, "Versions & pinning".to_string())
}
