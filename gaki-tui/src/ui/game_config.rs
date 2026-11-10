//! The per-game settings overlay (modal popup): a game's `games.toml`
//! settings — profile, platform, Steam integration, each a row Enter
//! cycles; launch arguments, variables and shader cache, each a row Enter
//! types — plus its wrappers (read-only).

use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::browser::Browser;
use crate::app::browser::game_config::{GameConfigRow, join_words};
use crate::app::steam::GameSettings;
use crate::keymap::{GameSettingsAction as Gs, hints, key_of};
use crate::ui::theme;
use crate::ui::{paneled_paragraph, selection_marker};

/// Build the game-settings overlay from the browser's open game-config state.
/// Each row shows the entry's value, or what an unset one falls back to; the
/// highlighted row gets a selection bar. The wrappers and a prompt or a
/// status/hint line close it.
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
    let args = match cfg {
        Some(c) if !c.args.is_empty() => join_words(&c.args),
        _ => "(none)".to_string(),
    };
    let env = match cfg {
        Some(c) if !c.env.is_empty() => {
            let own: Vec<String> = c.env.iter().map(|(k, v)| format!("{k}={v}")).collect();
            join_words(&own)
        }
        Some(c) if !c.effective_env.is_empty() => format!(
            "(profile: {})",
            c.effective_env
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(" ")
        ),
        _ => "(none)".to_string(),
    };
    let shader_cache = match cfg {
        Some(GameSettings {
            shader_cache: Some(s),
            ..
        }) => s.clone(),
        Some(GameSettings {
            effective_shader_cache: Some(dir),
            ..
        }) => format!("(profile: {})", dir.display()),
        _ => "(profile: off)".to_string(),
    };
    let rows: Vec<(GameConfigRow, String)> = GameConfigRow::ALL
        .into_iter()
        .zip([profile, platform, steam, args, env, shader_cache])
        .collect();

    let mut lines: Vec<Line<'static>> = Vec::new();
    for (i, (row, value)) in rows.iter().enumerate() {
        let label = row.label();
        let selected = i == browser.game_config_index;
        let style = if selected {
            theme::selection()
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
    if let Some(input) = &browser.game_config_input {
        let prompt = match browser.game_config_row() {
            GameConfigRow::Args => "Arguments: ",
            GameConfigRow::Env => "Variables (KEY=VALUE): ",
            _ => "Shader cache (directory or off): ",
        };
        lines.push(Line::from(vec![
            Span::styled(prompt.to_string(), theme::label()),
            Span::styled(input.clone(), theme::accent()),
            Span::styled("_".to_string(), theme::key()),
        ]));
        let hint = if browser.game_config_status.is_empty() {
            "Enter to save (empty clears) · Esc to cancel · quote words with spaces".to_string()
        } else {
            browser.game_config_status.clone()
        };
        lines.push(Line::from(Span::styled(hint, theme::dim())));
        return paneled_paragraph(lines, "Game settings".to_string());
    }
    let status = if !browser.game_config_status.is_empty() {
        browser.game_config_status.clone()
    } else {
        hints(&[(key_of(Gs::Edit), "change"), ("Esc".to_string(), "close")])
    };
    lines.push(Line::from(Span::styled(status, theme::dim())));

    paneled_paragraph(lines, "Game settings".to_string())
}
