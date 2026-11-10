//! The Store overlay (`Y`): a section bar, the shown section's body (price
//! per region, players, news, reviews, similar games) and a key hint.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use libgaki::ops::store::{Listing, Prices};
use libgaki::steam_client::unix_to_ymd;
use libgaki::web::discovery::{NewsItem, ReviewPage};

use crate::app::browser::job::Load;
use crate::app::browser::store::{Section, StoreView};
use crate::keymap::{OverlayAction as O, StoreAction as St, hints, key_of, pair};
use crate::ui::theme;

/// Draw the overlay into `area`.
pub fn draw(frame: &mut Frame, area: Rect, view: &StoreView) {
    let block = theme::panel(format!("Store · {}", view.name));
    let inner = block.inner(area);
    frame.render_widget(Paragraph::new("").block(block).style(theme::base()), area);
    let [bar, body_area, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .areas(inner);

    frame.render_widget(
        Paragraph::new(section_bar(view.section)).style(theme::base()),
        bar,
    );

    view.body_width.set(body_area.width);
    let paragraph = body(view);
    let max = paragraph
        .line_count(body_area.width.max(1))
        .saturating_sub(1);
    let scroll = view.scroll.min(max as u16);
    frame.render_widget(paragraph.scroll((scroll, 0)), body_area);

    frame.render_widget(
        Paragraph::new(footer_lines(view)).style(theme::base()),
        footer,
    );
}

fn section_bar(current: Section) -> Line<'static> {
    let mut spans = Vec::new();
    for section in Section::ALL {
        let label = format!(" {} ", section.label());
        spans.push(if section == current {
            Span::styled(label, theme::selection())
        } else {
            Span::styled(label, theme::dim())
        });
        spans.push(Span::raw(" "));
    }
    Line::from(spans)
}

fn footer_lines(view: &StoreView) -> Vec<Line<'static>> {
    if let Some(text) = &view.regions_input {
        return vec![
            Line::from(vec![
                Span::styled("Regions: ".to_string(), theme::label()),
                Span::styled(text.clone(), theme::accent()),
                Span::styled("_".to_string(), theme::key()),
            ]),
            dim("two-letter codes, e.g. US,DE,JP (empty: your own) · Enter load · Esc cancel"),
        ];
    }
    let mut parts = vec![
        (pair(O::NextTab, O::PreviousTab), "section"),
        (pair(O::Down, O::Up), "scroll"),
    ];
    if view.section == Section::Price {
        parts.push((key_of(St::Regions), "regions"));
    }
    parts.push(("Esc".to_string(), "close"));
    vec![Line::from(""), dim(hints(&parts))]
}

fn dim(text: impl Into<String>) -> Line<'static> {
    Line::from(Span::styled(text.into(), theme::dim()))
}

fn warn(text: impl Into<String>) -> Line<'static> {
    Line::from(Span::styled(text.into(), theme::warning()))
}

/// The body of a section in its load state.
fn loaded<T>(
    load: Option<&Load<T>>,
    what: &str,
    build: impl Fn(&T) -> Vec<Line<'static>>,
) -> Vec<Line<'static>> {
    match load {
        None | Some(Load::Loading(_)) => vec![dim(format!("Loading {what}…"))],
        Some(Load::Failed(e)) => vec![warn(format!("Couldn't load {what}: {e}"))],
        Some(Load::Ready(value)) => build(value),
    }
}

/// The shown section's body, as drawn.
fn body(view: &StoreView) -> Paragraph<'static> {
    Paragraph::new(body_lines(view))
        .style(theme::base())
        .wrap(Wrap { trim: false })
}

/// Rows the shown section's body takes once wrapped at the width it was
/// last drawn at, for clamping the scroll.
pub fn body_height(view: &StoreView) -> usize {
    body(view).line_count(view.body_width.get().max(1))
}

fn body_lines(view: &StoreView) -> Vec<Line<'static>> {
    match view.section {
        Section::Price => loaded(view.price.as_ref(), "prices", price_lines),
        Section::Players => loaded(view.players.as_ref(), "the player count", |n| {
            vec![Line::from(vec![
                Span::styled(thousands(i64::from(*n)), theme::heading()),
                Span::styled(" playing right now".to_string(), theme::value()),
            ])]
        }),
        Section::News => loaded(view.news.as_ref(), "news", |items| news_lines(items)),
        Section::Reviews => loaded(view.reviews.as_ref(), "reviews", review_lines),
        Section::Similar => loaded(view.similar.as_ref(), "similar games", similar_lines),
    }
}

/// `1234567` as `1,234,567`.
pub(crate) fn thousands(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

fn price_lines(prices: &Prices) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(
        format!(
            "{:<8}{:<16}{:<9}{:<16}{}",
            "Region", "Price", "Discount", "Full price", "Sale ends"
        ),
        theme::label(),
    ))];
    for q in &prices.regions {
        let row = if let Some(e) = &q.error {
            warn(format!("{:<8}couldn't load: {e}", q.country))
        } else if q.is_free {
            Line::from(Span::styled(
                format!("{:<8}Free", q.country),
                theme::value(),
            ))
        } else if !q.available {
            dim(format!("{:<8}not sold here", q.country))
        } else {
            let price = q.price.clone().unwrap_or_default();
            let (discount, full) = if q.discount_pct > 0 {
                (
                    format!("-{}%", q.discount_pct),
                    q.original_price.clone().unwrap_or_default(),
                )
            } else {
                (String::new(), String::new())
            };
            let ends = q.discount_end_date.clone().unwrap_or_default();
            Line::from(vec![
                Span::styled(format!("{:<8}", q.country), theme::value()),
                Span::styled(format!("{price:<16}"), theme::accent()),
                Span::styled(format!("{discount:<9}"), theme::good()),
                Span::styled(format!("{full:<16}"), theme::dim()),
                Span::styled(ends, theme::dim()),
            ])
        };
        lines.push(row);
    }
    lines
}

/// Collapse a news body's whitespace (feeds carry raw line breaks).
fn one_paragraph(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn news_lines(items: &[NewsItem]) -> Vec<Line<'static>> {
    if items.is_empty() {
        return vec![dim("No news.")];
    }
    let mut lines = Vec::new();
    for item in items {
        lines.push(Line::from(vec![
            Span::styled(format!("{}  ", unix_to_ymd(item.date as i64)), theme::dim()),
            Span::styled(item.title.clone(), theme::emphasis()),
        ]));
        let source = [item.feed.as_str(), item.author.as_str()]
            .iter()
            .filter(|s| !s.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(" · ");
        if !source.is_empty() {
            lines.push(dim(source));
        }
        let body = one_paragraph(&item.contents);
        if !body.is_empty() {
            lines.push(Line::from(Span::styled(body, theme::value())));
        }
        lines.push(dim(item.url.clone()));
        lines.push(Line::from(""));
    }
    lines
}

fn review_lines(page: &ReviewPage) -> Vec<Line<'static>> {
    let s = &page.summary;
    let mut lines = Vec::new();
    if s.total_reviews > 0 {
        let pct = s.total_positive * 100 / s.total_reviews.max(1);
        lines.push(Line::from(vec![
            Span::styled(s.label.clone(), theme::heading()),
            Span::styled(
                format!("  {pct}% of {} positive", thousands(s.total_reviews)),
                theme::value(),
            ),
        ]));
    } else {
        lines.push(dim("No reviews yet."));
    }
    lines.push(Line::from(""));
    for r in &page.reviews {
        let (verdict, style) = if r.voted_up {
            ("▲ Recommended", theme::good())
        } else {
            ("▼ Not recommended", theme::warning())
        };
        lines.push(Line::from(vec![
            Span::styled(verdict.to_string(), style),
            Span::styled(
                format!(
                    "  {}  {:.1} h  {} helpful  {}",
                    r.author,
                    r.playtime_at_review_hours,
                    r.votes_up,
                    unix_to_ymd(r.created as i64)
                ),
                theme::dim(),
            ),
        ]));
        lines.push(Line::from(Span::styled(
            one_paragraph(&r.text),
            theme::value(),
        )));
        lines.push(Line::from(""));
    }
    lines
}

fn similar_lines(listing: &Listing) -> Vec<Line<'static>> {
    if listing.records.is_empty() {
        return vec![dim("Nothing similar listed.")];
    }
    listing
        .records
        .iter()
        .map(|r| {
            let price = if r.is_free {
                "Free".to_string()
            } else {
                r.price.clone().unwrap_or_else(|| "—".to_string())
            };
            let mut spans = vec![
                Span::styled(format!("{:<10}", r.app_id), theme::dim()),
                Span::styled(r.name.clone(), theme::value()),
                Span::styled(format!("  {price}"), theme::accent()),
            ];
            if r.discount_pct > 0 {
                spans.push(Span::styled(
                    format!(" -{}%", r.discount_pct),
                    theme::good(),
                ));
            }
            if let Some(summary) = &r.review_summary {
                spans.push(Span::styled(format!("  {summary}"), theme::dim()));
            }
            Line::from(spans)
        })
        .collect()
}
