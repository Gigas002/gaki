//! The per-game settings overlay (modal popup): a game's `games.toml`
//! settings — profile, platform, Steam integration — each a selectable row
//! that Enter cycles, plus its wrappers (read-only).

use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::browser::Browser;
use crate::app::steam::GameSettings;
use crate::ui::theme;
use crate::ui::{paneled_paragraph, selection_marker};

/// Build the game-settings overlay from the browser's open game-config state.
/// Three rows (Profile, Platform, Steam) show the entry's value, or what an
/// unset one falls back to; the highlighted row gets a selection bar. The
/// wrappers and a status/hint line close it.
pub fn game_config(browser: &Browser) -> Paragraph<'static> {
    let cfg = browser.game_config.as_ref();
    let profile = match cfg {
        Some(GameSettings {
            profile: Some(p), ..
        }) => p.clone(),
        Some(GameSettings {
            effective_profile: Some(p),
            ..
        }) => format!("(default: {p})"),
        _ => "(none)".to_string(),
    };
    let platform = cfg
        .and_then(|c| c.platform.clone())
        .unwrap_or_else(|| "(auto)".to_string());
    let steam = match cfg {
        Some(c) => c
            .steam
            .clone()
            .unwrap_or_else(|| format!("(profile: {})", c.effective_steam)),
        None => "(auto)".to_string(),
    };
    let rows: [(&str, String); 3] = [
        ("Profile", profile),
        ("Platform", platform),
        ("Steam", steam),
    ];

    let mut lines: Vec<Line<'static>> = Vec::new();
    for (i, (label, value)) in rows.iter().enumerate() {
        let selected = i == browser.game_config_index;
        let style = if selected {
            theme::selection(theme::ACCENT)
        } else {
            theme::value()
        };
        let label_style = if selected { style } else { theme::label() };
        lines.push(Line::from(vec![
            Span::styled(selection_marker(selected).to_string(), style),
            Span::styled(format!("{label}: "), label_style),
            Span::styled(value.clone(), style),
        ]));
    }

    let wrappers = cfg.map(|c| c.wrappers.join(" ")).unwrap_or_default();
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled("Wrappers: ", theme::label()),
        Span::styled(
            if wrappers.is_empty() {
                "(none)".to_string()
            } else {
                wrappers
            },
            theme::value(),
        ),
    ]));
    lines.push(Line::from(Span::styled(
        "  edit with: gaki-cli config game <id> --wrapper ...".to_string(),
        theme::dim(),
    )));

    lines.push(Line::from(""));
    let status = if !browser.game_config_status.is_empty() {
        browser.game_config_status.clone()
    } else {
        "[Enter] change  [Esc] close".to_string()
    };
    lines.push(Line::from(Span::styled(status, theme::dim())));

    paneled_paragraph(lines, "Game settings".to_string())
}
