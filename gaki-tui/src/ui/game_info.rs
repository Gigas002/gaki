//! The Game info overlay (`n`): a tab bar over the selected game's store
//! record (Details), its DRM tickets and files on disk (Checks), and its
//! artwork with the folder it saves to (Artwork).

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::browser::game_info::{GameInfoView, InfoTab};
use crate::app::browser::job::Load;
use crate::app::steam::{CheckLine, GameInfo};
use crate::ui::theme;

fn dim(text: impl Into<String>) -> Line<'static> {
    Line::from(Span::styled(text.into(), theme::dim()))
}

fn warn(text: impl Into<String>) -> Line<'static> {
    Line::from(Span::styled(text.into(), theme::value().fg(theme::WARN)))
}

fn heading(text: &str) -> Line<'static> {
    Line::from(Span::styled(
        text.to_string(),
        theme::label().add_modifier(Modifier::BOLD),
    ))
}

/// `label  value`, skipped by the callers when the value is empty.
fn field(label: &str, value: impl Into<String>) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<18}"), theme::label()),
        Span::styled(value.into(), theme::value()),
    ])
}

fn push_list(lines: &mut Vec<Line<'static>>, label: &str, items: &[String]) {
    if !items.is_empty() {
        lines.push(field(label, items.join(", ")));
    }
}

/// Collapse whitespace (store text carries raw line breaks).
fn one_paragraph(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn tab_bar(current: InfoTab) -> Line<'static> {
    let mut spans = Vec::new();
    for tab in InfoTab::ALL {
        let label = format!(" {} ", tab.label());
        spans.push(if tab == current {
            Span::styled(label, theme::selection(theme::ACCENT))
        } else {
            Span::styled(label, theme::dim())
        });
        spans.push(Span::raw(" "));
    }
    Line::from(spans)
}

/// Draw the overlay into `area`.
pub fn draw(frame: &mut Frame, area: Rect, view: &GameInfoView) {
    let block = theme::panel(format!("Game info · {}", view.name));
    let inner = block.inner(area);
    frame.render_widget(Paragraph::new("").block(block).style(theme::base()), area);
    let [bar, _, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .areas(inner);
    frame.render_widget(Paragraph::new(tab_bar(view.tab)).style(theme::base()), bar);

    let lines = match view.tab {
        InfoTab::Details => loaded(&view.details, "the store record", details_lines),
        InfoTab::Checks => checks_lines(view),
        InfoTab::Artwork => artwork_lines(view),
    };
    let paragraph = Paragraph::new(lines)
        .style(theme::base())
        .wrap(Wrap { trim: false });
    let max = paragraph.line_count(body.width.max(1)).saturating_sub(1);
    frame.render_widget(paragraph.scroll((view.scroll.min(max as u16), 0)), body);
    frame.render_widget(
        Paragraph::new(footer_lines(view))
            .style(theme::base())
            .wrap(Wrap { trim: false }),
        footer,
    );
}

fn footer_lines(view: &GameInfoView) -> Vec<Line<'static>> {
    if let Some(text) = &view.dir_input {
        return vec![
            Line::from(vec![
                Span::styled("Save to: ".to_string(), theme::label()),
                Span::styled(text.clone(), theme::accent()),
                Span::styled("_".to_string(), theme::key()),
            ]),
            dim("Enter keep · Esc cancel"),
        ];
    }
    let mut hint = String::from("[Tab] tab  [j/k PgUp/PgDn] scroll  ");
    match view.tab {
        InfoTab::Checks => hint.push_str("[r] check again  "),
        InfoTab::Artwork => hint.push_str("[s] save  [o] folder  "),
        InfoTab::Details => {}
    }
    hint.push_str("[Esc] close");
    vec![Line::from(""), dim(hint)]
}

fn loaded<T>(
    load: &Load<T>,
    what: &str,
    build: impl Fn(&T) -> Vec<Line<'static>>,
) -> Vec<Line<'static>> {
    match load {
        Load::Loading(_) => vec![dim(format!("Loading {what}…"))],
        Load::Failed(e) => vec![warn(format!("Couldn't load {what}: {e}"))],
        Load::Ready(value) => build(value),
    }
}

fn details_lines(info: &GameInfo) -> Vec<Line<'static>> {
    let d = &info.details;
    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled(d.name.clone(), theme::value().add_modifier(Modifier::BOLD)),
        Span::styled(format!("  {} · app {}", d.app_type, d.app_id), theme::dim()),
    ]));
    if !d.short_description.is_empty() {
        lines.push(Line::from(Span::styled(
            one_paragraph(&d.short_description),
            theme::value(),
        )));
    }
    lines.push(Line::from(""));

    push_list(&mut lines, "Developers", &d.developers);
    push_list(&mut lines, "Publishers", &d.publishers);
    push_list(&mut lines, "Franchises", &d.franchises);
    let mut release = d.release_date.clone().unwrap_or_default();
    if d.coming_soon {
        release.push_str(" (coming soon)");
    }
    if d.is_early_access {
        release.push_str(" (early access)");
    }
    if !release.trim().is_empty() {
        lines.push(field("Release", release.trim().to_string()));
    }
    let price = if d.is_free {
        "Free".to_string()
    } else {
        match (&d.price, d.discount_pct, &d.original_price) {
            (Some(p), pct, Some(orig)) if pct > 0 => format!("{p} (-{pct}%, was {orig})"),
            (Some(p), _, _) => p.clone(),
            (None, _, _) => String::new(),
        }
    };
    if !price.is_empty() {
        lines.push(field("Price", format!("{price} [{}]", d.country)));
    }
    if d.region_locked {
        lines.push(field("Region", "not sold in your region"));
    }
    push_list(&mut lines, "Platforms", &d.platforms);
    if let Some(reviews) = &d.review_summary {
        lines.push(field("Reviews", reviews.clone()));
    }
    if let Some(r) = &d.rating {
        let mut text = format!("{} {}", r.system.to_uppercase(), r.rating);
        if r.required_age > 0 {
            text.push_str(&format!(", {}+", r.required_age));
        }
        if !r.descriptors.is_empty() {
            text.push_str(&format!(" ({})", r.descriptors.join(", ")));
        }
        lines.push(field("Rating", text));
    }
    let tags: Vec<String> = d.tags.iter().filter_map(|t| t.name.clone()).collect();
    push_list(&mut lines, "Tags", &tags);
    if info.dlc_count > 0 {
        lines.push(field("DLC", info.dlc_count.to_string()));
    }
    lines.push(field(
        "Media",
        format!(
            "{} screenshots, {} trailers",
            d.screenshots.len(),
            d.trailers.len()
        ),
    ));

    if let Some(web) = &info.extended {
        push_list(&mut lines, "Genres", &web.genres);
        push_list(&mut lines, "Categories", &web.categories);
        if let Some(score) = web.metacritic {
            let url = web.metacritic_url.clone().unwrap_or_default();
            lines.push(field(
                "Metacritic",
                format!("{score}  {url}").trim().to_string(),
            ));
        }
        if let Some(site) = &web.website {
            lines.push(field("Website", site.clone()));
        }
    }

    if !d.links.is_empty() {
        lines.push(Line::from(""));
        lines.push(heading("Links"));
        for link in &d.links {
            lines.push(field(&format!("  {}", link.kind), link.url.clone()));
        }
    }
    if !d.purchase_options.is_empty() {
        lines.push(Line::from(""));
        lines.push(heading("Sold as"));
        for o in &d.purchase_options {
            let mut text = o.name.clone();
            if let Some(p) = &o.price {
                text.push_str(&format!("  {p}"));
            }
            if o.discount_pct > 0 {
                text.push_str(&format!(" (-{}%)", o.discount_pct));
            }
            lines.push(Line::from(Span::styled(
                format!("  {text}"),
                theme::value(),
            )));
        }
    }
    if let Some(web) = &info.extended {
        for (title, reqs) in [
            ("Minimum requirements", &web.requirements_minimum),
            ("Recommended requirements", &web.requirements_recommended),
        ] {
            if !reqs.is_empty() {
                lines.push(Line::from(""));
                lines.push(heading(title));
                for r in reqs {
                    lines.push(Line::from(Span::styled(format!("  {r}"), theme::value())));
                }
            }
        }
    }
    if !d.full_description.is_empty() {
        lines.push(Line::from(""));
        lines.push(heading("About"));
        lines.push(Line::from(Span::styled(
            one_paragraph(&d.full_description),
            theme::value(),
        )));
    }
    lines
}

fn check(label: &str, line: &CheckLine) -> Line<'static> {
    let (mark, style) = if line.ok {
        ("✓", theme::value().fg(theme::ONLINE))
    } else {
        ("✗", theme::value().fg(theme::WARN))
    };
    Line::from(vec![
        Span::styled(format!("{mark} "), style),
        Span::styled(format!("{label:<22}"), theme::label()),
        Span::styled(line.text.clone(), theme::value()),
    ])
}

fn checks_lines(view: &GameInfoView) -> Vec<Line<'static>> {
    let mut lines = vec![heading("DRM tickets")];
    match &view.drm {
        None | Some(Load::Loading(_)) => lines.push(dim("Asking Steam…")),
        Some(Load::Failed(e)) => lines.push(warn(format!("Couldn't check: {e}"))),
        Some(Load::Ready(drm)) => {
            lines.push(check("Ownership ticket", &drm.ownership));
            lines.push(check("Encrypted app ticket", &drm.encrypted));
        }
    }
    #[cfg(feature = "install")]
    {
        lines.push(Line::from(""));
        lines.push(heading("Files"));
        match &view.files {
            None | Some(Load::Loading(_)) => lines.push(dim("Looking…")),
            Some(Load::Failed(e)) => lines.push(warn(format!("Couldn't check: {e}"))),
            Some(Load::Ready(files)) => {
                let text = if files.available {
                    "installed, files present".to_string()
                } else {
                    "not installed, or files missing".to_string()
                };
                lines.push(check(
                    "Available",
                    &CheckLine {
                        ok: files.available,
                        text,
                    },
                ));
                if let Some(path) = &files.install_path {
                    lines.push(field("  Path", path.clone()));
                }
                if files.pinned {
                    lines.push(field(
                        "  Pinned",
                        format!("{} depot(s) held at their version", files.pinned_depots),
                    ));
                }
            }
        }
    }
    lines
}

fn artwork_lines(view: &GameInfoView) -> Vec<Line<'static>> {
    let mut lines = vec![field("Save to", view.save_dir.clone()), Line::from("")];
    lines.extend(loaded(&view.details, "the artwork list", |info| {
        let a = &info.details.assets;
        let mut rows = Vec::new();
        for (kind, url) in [
            ("header", &a.header),
            ("capsule", &a.capsule),
            ("hero", &a.hero),
            ("background", &a.background),
            ("logo", &a.logo),
        ] {
            rows.push(match url {
                Some(url) => field(kind, url.clone()),
                None => Line::from(vec![
                    Span::styled(format!("{kind:<18}"), theme::label()),
                    Span::styled("none".to_string(), theme::dim()),
                ]),
            });
        }
        rows
    }));
    match &view.saving {
        None => {}
        Some(Load::Loading(_)) => {
            lines.push(Line::from(""));
            lines.push(dim("Saving…"));
        }
        Some(Load::Failed(e)) => {
            lines.push(Line::from(""));
            lines.push(warn(format!("Couldn't save: {e}")));
        }
        Some(Load::Ready(saved)) => {
            lines.push(Line::from(""));
            lines.push(heading("Saved"));
            if saved.is_empty() {
                lines.push(dim("The store lists no artwork for this game."));
            }
            for image in saved {
                lines.push(check(
                    image.kind,
                    &CheckLine {
                        ok: image.result.is_ok(),
                        text: match &image.result {
                            Ok(path) => path.clone(),
                            Err(e) => e.clone(),
                        },
                    },
                ));
            }
        }
    }
    lines
}
