//! The user profile overlay: the looked-up user's public profile, the
//! fields `gaki-cli user` prints, with the lookup prompt or the key hints
//! in the footer.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use libgaki::ops::social::{persona_state_label, visibility_label};
use libgaki::steam_client::{UserProfile, unix_to_ymd};

use crate::app::browser::job::Load;
use crate::app::browser::user_profile::ProfileView;
use crate::ui::theme;

fn dim(text: impl Into<String>) -> Line<'static> {
    Line::from(Span::styled(text.into(), theme::dim()))
}

fn warn(text: impl Into<String>) -> Line<'static> {
    Line::from(Span::styled(text.into(), theme::value().fg(theme::WARN)))
}

fn field(label: &str, value: impl Into<String>) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<11}"), theme::label()),
        Span::styled(value.into(), theme::value()),
    ])
}

fn date(secs: u64) -> String {
    unix_to_ymd(secs as i64)
}

/// The profile as lines, in `gaki-cli user`'s order.
pub fn profile_lines(profile: &UserProfile) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                profile
                    .persona_name
                    .clone()
                    .unwrap_or_else(|| "(no persona name)".to_string()),
                theme::accent(),
            ),
            Span::styled(format!("  ({})", profile.steam_id), theme::dim()),
        ]),
        Line::from(""),
        field("Profile", profile.profile_url.clone()),
    ];
    if let Some(url) = &profile.avatar_url {
        lines.push(field("Avatar", url.clone()));
    }
    let limited = if profile.is_limited {
        ", limited account"
    } else {
        ""
    };
    lines.push(field(
        "Privacy",
        format!("{}{limited}", visibility_label(profile.visibility)),
    ));
    if let Some(name) = &profile.real_name {
        lines.push(field("Name", name.clone()));
    }
    if let Some(place) = &profile.location {
        lines.push(field("Location", place.clone()));
    }
    if let Some(t) = profile.created {
        lines.push(field("Created", date(t)));
    }
    lines.push(field("Status", persona_state_label(profile.persona_state)));
    match (&profile.game_name, profile.game_app_id) {
        (Some(game), _) => lines.push(field("Playing", game.clone())),
        (None, Some(id)) => lines.push(field("Playing", format!("app {id}"))),
        _ => {}
    }
    if let Some(t) = profile.last_logoff {
        lines.push(field("Last seen", date(t)));
    }
    if let Some(headline) = &profile.headline {
        lines.push(field("Headline", headline.clone()));
    }
    if let Some(summary) = &profile.summary {
        lines.push(Line::from(""));
        for line in libgaki::web::store::strip_html(summary).lines() {
            lines.push(Line::from(Span::styled(line.to_string(), theme::value())));
        }
    }
    if let Some(t) = profile.ban_expires {
        lines.push(Line::from(""));
        lines.push(warn(format!("Community ban until {}", date(t))));
    }
    lines
}

fn footer(view: &ProfileView) -> Vec<Line<'static>> {
    if let Some(text) = &view.input {
        return vec![
            Line::from(vec![
                Span::styled("Look up: ".to_string(), theme::label()),
                Span::styled(text.clone(), theme::accent()),
                Span::styled("_".to_string(), theme::key()),
            ]),
            dim("SteamID64, profile URL, friend name or `me` · Enter look up · Esc cancel"),
        ];
    }
    let mut hint = String::from("[j/k] scroll  [/] look someone up  ");
    if view.loaded().is_some_and(|p| !p.profile_url.is_empty()) {
        hint.push_str("[E] profile page  ");
    }
    hint.push_str("[Esc] close");
    vec![dim(hint)]
}

/// Draw the overlay into `area`.
pub fn draw(frame: &mut Frame, area: Rect, view: &ProfileView) {
    let title = match view.loaded().and_then(|p| p.persona_name.clone()) {
        Some(name) => format!("Profile · {name}"),
        None => "Profile".to_string(),
    };
    let block = theme::panel(title);
    let inner = block.inner(area);
    frame.render_widget(Paragraph::new("").block(block).style(theme::base()), area);
    let footer = footer(view);
    let [body, _, bottom] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(footer.len() as u16),
    ])
    .areas(inner);
    frame.render_widget(Paragraph::new(footer).style(theme::base()), bottom);

    let lines = match &view.profile {
        None => vec![dim("Type who to look up.")],
        Some(Load::Loading(_)) => vec![dim(format!("Looking up {}…", view.query))],
        Some(Load::Failed(e)) => vec![warn(format!("Couldn't look up {}: {e}", view.query))],
        Some(Load::Ready(profile)) => profile_lines(profile),
    };
    let paragraph = Paragraph::new(lines)
        .style(theme::base())
        .wrap(Wrap { trim: false });
    let max = paragraph.line_count(body.width.max(1)).saturating_sub(1);
    frame.render_widget(paragraph.scroll((view.scroll.min(max as u16), 0)), body);
}
