//! The Steam Store overlay (`J`): a section bar, the shown section's rows
//! (search results, deals, events, the wishlist) and a footer with the
//! search prompt, the wishlist confirm or the key hints.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::ListItem;

use libgaki::steam_client::{StoreEvent, unix_to_ymd};

use crate::app::browser::job::Load;
use crate::app::browser::shop::{GameRow, ShopSection, ShopView, scope_label};
use crate::keymap::{OverlayAction as O, SteamStoreAction as Sh, hints, key_of};
use crate::ui::depots::draw_list;
use crate::ui::theme;
use crate::ui::{empty_list_rows, selection_marker};

/// Draw the overlay into `area`.
pub fn draw(frame: &mut Frame, area: Rect, view: &ShopView) {
    let mut title = String::from("Steam Store ·");
    for section in ShopSection::ALL {
        if section == view.section {
            title.push_str(&format!(" [{}]", section.label()));
        } else {
            title.push_str(&format!(" {}", section.label()));
        }
    }
    let (items, selected) = rows(view);
    draw_list(frame, area, title, items, selected, footer(view));
}

fn dim(text: impl Into<String>) -> Line<'static> {
    Line::from(Span::styled(text.into(), theme::dim()))
}

fn rows(view: &ShopView) -> (Vec<ListItem<'static>>, Option<usize>) {
    let mut header: Vec<ListItem<'static>> = Vec::new();
    let state = match view.section {
        ShopSection::Search => {
            if view.query.is_empty() {
                return (empty_list_rows("Type a title and press Enter."), None);
            }
            header.push(ListItem::new(dim(format!("Results for “{}”", view.query))));
            view.search.as_ref().map(load_state)
        }
        ShopSection::Deals => {
            header.push(ListItem::new(dim(format!(
                "Showing {} ({})",
                scope_label(view.deals_scope),
                hints(&[(key_of(Sh::NextDeals), "next list")])
            ))));
            view.deals.as_ref().map(load_state)
        }
        ShopSection::Events => view.events.as_ref().map(load_state),
        ShopSection::Wishlist => view.wishlist.as_ref().map(load_state),
    };
    match state {
        None | Some(State::Loading) => {
            header.extend(empty_list_rows("Loading…"));
            return (header, None);
        }
        Some(State::Failed(e)) => {
            header.extend(empty_list_rows(format!("Couldn't load: {e}")));
            return (header, None);
        }
        Some(State::Ready) => {}
    }

    let offset = header.len();
    let body: Vec<ListItem<'static>> = if view.section == ShopSection::Events {
        let events = view
            .events
            .as_ref()
            .and_then(Load::ready)
            .map(Vec::as_slice)
            .unwrap_or_default();
        events
            .iter()
            .enumerate()
            .map(|(i, e)| event_row(e, i == view.selected))
            .collect()
    } else {
        view.game_rows()
            .iter()
            .enumerate()
            .map(|(i, r)| game_row(r, i == view.selected))
            .collect()
    };
    if body.is_empty() {
        let message = match view.section {
            ShopSection::Search => "No matches.",
            ShopSection::Deals => "No deals listed.",
            ShopSection::Events => "No sales or events right now.",
            ShopSection::Wishlist => "Your wishlist is empty.",
        };
        header.extend(empty_list_rows(message));
        return (header, None);
    }
    let selected = Some(offset + view.selected);
    header.extend(body);
    (header, selected)
}

enum State {
    Loading,
    Failed(String),
    Ready,
}

fn load_state<T>(load: &Load<T>) -> State {
    match load {
        Load::Loading(_) => State::Loading,
        Load::Failed(e) => State::Failed(e.clone()),
        Load::Ready(_) => State::Ready,
    }
}

fn row_style(selected: bool) -> ratatui::style::Style {
    if selected {
        theme::selection(theme::ACCENT)
    } else {
        theme::value()
    }
}

fn game_row(row: &GameRow, selected: bool) -> ListItem<'static> {
    let style = row_style(selected);
    let price = if row.is_free {
        "Free".to_string()
    } else {
        row.price.clone().unwrap_or_else(|| "—".to_string())
    };
    let mut spans = vec![
        Span::styled(selection_marker(selected).to_string(), style),
        Span::styled(format!("{:<9}", row.app_id), theme::dim()),
        Span::styled(row.name.clone(), style),
        Span::styled(format!("  {price}"), theme::accent()),
    ];
    if row.discount_pct > 0 {
        spans.push(Span::styled(
            format!(" -{}%", row.discount_pct),
            theme::value().fg(theme::ONLINE),
        ));
    }
    if let Some(review) = &row.review {
        spans.push(Span::styled(format!("  {review}"), theme::dim()));
    }
    ListItem::new(Line::from(spans))
}

fn event_row(event: &StoreEvent, selected: bool) -> ListItem<'static> {
    let style = row_style(selected);
    let mut spans = vec![
        Span::styled(selection_marker(selected).to_string(), style),
        Span::styled(event.title.clone(), style),
    ];
    if !event.kind.is_empty() {
        spans.push(Span::styled(format!("  {}", event.kind), theme::dim()));
    }
    if event.start > 0 || event.end > 0 {
        spans.push(Span::styled(
            format!(
                "  {} – {}",
                unix_to_ymd(event.start as i64),
                unix_to_ymd(event.end as i64)
            ),
            theme::dim(),
        ));
    }
    if !event.associated_name.is_empty() {
        spans.push(Span::styled(
            format!("  {}", event.associated_name),
            theme::accent(),
        ));
    }
    ListItem::new(Line::from(spans))
}

fn footer(view: &ShopView) -> Vec<Line<'static>> {
    if let Some(change) = &view.confirm {
        let question = if change.add {
            format!("Add {} to your wishlist?", change.name)
        } else {
            format!("Remove {} from your wishlist?", change.name)
        };
        return vec![
            Line::from(Span::styled(question, theme::value().fg(theme::WARN))),
            dim("[y] yes  [n] no"),
        ];
    }
    if let Some(text) = &view.search_input {
        return vec![
            Line::from(vec![
                Span::styled("Search: ".to_string(), theme::label()),
                Span::styled(text.clone(), theme::accent()),
                Span::styled("_".to_string(), theme::key()),
            ]),
            dim("Enter search · Esc cancel · Tab next section"),
        ];
    }
    let mut lines = Vec::new();
    if !view.status.is_empty() {
        lines.push(Line::from(Span::styled(
            view.status.clone(),
            theme::value().fg(theme::WARN),
        )));
    }
    let mut parts = vec![(key_of(O::NextTab), "section")];
    match view.section {
        ShopSection::Search => parts.push((key_of(Sh::Search), "search")),
        ShopSection::Deals => parts.push((key_of(Sh::NextDeals), "next list")),
        _ => {}
    }
    if view.section != ShopSection::Events {
        parts.push((key_of(Sh::Details), "details"));
        parts.push((key_of(Sh::Wishlist), "wishlist"));
        parts.push((key_of(Sh::StorePage), "store page"));
    }
    parts.push((key_of(Sh::Reload), "reload"));
    parts.push(("Esc".to_string(), "close"));
    lines.push(dim(hints(&parts)));
    lines
}
