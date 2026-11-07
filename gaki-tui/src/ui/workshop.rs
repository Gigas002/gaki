//! The Workshop overlay (modal popup). Two modes:
//!
//! * the status list (default): the selected game's installed and subscribed
//!   Workshop items with update badges, a highlight, and install, update and
//!   uninstall; and
//! * a browse/search pane (toggled with `b`): a query input over a list of
//!   discoverable Workshop items, with subscribe/unsubscribe on the highlight,
//!   and a comments sub-pane that can post a comment.
//!
//! Both are pure builders from `app::browser::Browser` + `theme`.

use crate::utils::bytes::convert;

use ratatui::style::Style;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Paragraph;

use crate::app::browser::Browser;
use crate::ui::theme;

/// Build the Workshop overlay content, `rows` tall inside its border.
/// Dispatches to the browse pane or the status list depending on the mode.
pub fn workshop(browser: &Browser, rows: usize) -> Paragraph<'static> {
    if browser.workshop_browse {
        if browser.workshop_comments_open {
            comments(browser)
        } else {
            browse(browser)
        }
    } else {
        status_list(browser, rows)
    }
}

/// The comments sub-pane: the item id in the title, a status/spinner line, then
/// each comment as an author header followed by its body. Scrolled by
/// `browser.workshop_comments_scroll`.
fn comments(browser: &Browser) -> Paragraph<'static> {
    let title = format!(
        "Comments — item {} (Esc: back)",
        browser.workshop_comments_id()
    );
    let mut lines: Vec<Line<'static>> = Vec::new();

    if browser.workshop_comments_loading {
        lines.push(Line::from(Span::styled(
            "Loading comments…",
            theme::accent(),
        )));
    } else if !browser.workshop_comments_status.is_empty() {
        lines.push(Line::from(Span::styled(
            browser.workshop_comments_status.clone(),
            theme::dim(),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            format!("{} comment(s)", browser.workshop_comments.len()),
            theme::dim(),
        )));
    }
    match &browser.workshop_tasks.comment {
        Some(draft) if draft.confirming => {
            lines.push(Line::from(Span::styled(
                format!("Post “{}”? It is public.", draft.text.trim()),
                theme::value().fg(theme::WARN),
            )));
            lines.push(Line::from(Span::styled(
                "[y] post  [n] keep editing",
                theme::dim(),
            )));
        }
        Some(draft) => {
            lines.push(Line::from(vec![
                Span::styled("Comment: ", theme::label()),
                Span::styled(draft.text.clone(), theme::accent()),
                Span::styled("_", theme::key()),
            ]));
            lines.push(Line::from(Span::styled(
                "Enter post · Esc cancel",
                theme::dim(),
            )));
        }
        None => lines.push(Line::from(Span::styled(
            "[c] write a comment  [j/k] scroll  [Esc] back",
            theme::dim(),
        ))),
    }
    lines.push(Line::from(""));

    for comment in browser
        .workshop_comments
        .iter()
        .skip(browser.workshop_comments_scroll)
    {
        lines.push(Line::from(Span::styled(
            comment.display_author(),
            theme::accent(),
        )));
        lines.push(Line::from(Span::styled(
            comment.message.clone(),
            theme::value(),
        )));
        lines.push(Line::from(""));
    }

    Paragraph::new(Text::from(lines))
        .block(theme::panel(title))
        .style(theme::base())
}

/// The status list: a status line, the rows (● installed, ○ subscribed
/// only, with an update badge), and the key hints or the uninstall confirm.
/// The highlight is kept within the `rows` the overlay has.
fn status_list(browser: &Browser, rows: usize) -> Paragraph<'static> {
    let items = &browser.workshop;
    let tasks = &browser.workshop_tasks;
    let title = format!("Workshop ({})", items.len());
    let mut lines: Vec<Line<'static>> = Vec::new();

    let status = tasks.status_line();
    lines.push(if tasks.is_loading() {
        Line::from(Span::styled("Loading…", theme::accent()))
    } else if let Some(e) = &tasks.list_error {
        Line::from(Span::styled(
            format!("Couldn't load: {e}"),
            theme::value().fg(theme::WARN),
        ))
    } else if !status.is_empty() {
        Line::from(Span::styled(status, theme::accent()))
    } else {
        let outdated = items.iter().filter(|i| i.update_available).count();
        Line::from(Span::styled(
            format!("{outdated} update(s) available"),
            theme::dim(),
        ))
    });
    lines.push(Line::from(""));

    let footer = match &tasks.confirm {
        Some(confirm) => vec![
            Line::from(Span::styled(
                format!("Uninstall {}? Its files are deleted.", confirm.title),
                theme::value().fg(theme::WARN),
            )),
            Line::from(Span::styled("[y] yes  [n] no", theme::dim())),
        ],
        None => vec![Line::from(Span::styled(
            "[i] install/update  [u] uninstall  [U] update all  [r] reload  [b] browse  [Esc] close",
            theme::dim(),
        ))],
    };

    if items.is_empty() && !tasks.is_loading() {
        lines.push(Line::from(Span::styled(
            "Nothing installed or subscribed.",
            theme::base(),
        )));
        lines.push(Line::from(Span::styled(
            "Press 'b' to browse and subscribe.",
            theme::dim(),
        )));
    }

    // Rows left for the list once the status line and footer are in.
    let room = rows.saturating_sub(lines.len() + footer.len() + 1).max(1);
    let skip = browser.workshop_selected.saturating_sub(room - 1);
    for (i, item) in items.iter().enumerate().skip(skip).take(room) {
        let selected = i == browser.workshop_selected;
        let (glyph, glyph_style) = if item.installed {
            ("● ", Style::default().fg(theme::ONLINE))
        } else {
            ("○ ", theme::dim())
        };
        let title_style = if selected {
            theme::selection(theme::ACCENT)
        } else {
            theme::value()
        };
        let mut spans = vec![
            Span::styled(if selected { "> " } else { "  " }, theme::accent()),
            Span::styled(glyph, glyph_style),
            Span::styled(item.display_title(), title_style),
        ];
        if let Some(id) = item.id {
            spans.push(Span::styled(format!("  {id}"), theme::dim()));
        }
        if item.update_available {
            spans.push(Span::styled(
                "  update available",
                Style::default().fg(theme::WARN),
            ));
        }
        if !item.subscribed {
            spans.push(Span::styled("  not subscribed", theme::dim()));
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::from(""));
    lines.extend(footer);

    Paragraph::new(Text::from(lines))
        .block(theme::panel(title))
        .style(theme::base())
}

/// The browse/search pane: a query input line, a status/hint line, then the
/// result rows with the highlight marked and each row tagged subscribed/not.
fn browse(browser: &Browser) -> Paragraph<'static> {
    let title =
        "Workshop browse — Enter: search, Tab: sub, F1/F2: rate, F3: comments, F4: install, Esc: back"
            .to_string();
    let mut lines: Vec<Line<'static>> = Vec::new();

    // Query input line.
    lines.push(Line::from(vec![
        Span::styled("Search: ", theme::dim()),
        Span::styled(browser.workshop_query.clone(), theme::value()),
        Span::styled("_", theme::accent()),
    ]));

    // Status / progress / hint line.
    let job = browser.workshop_tasks.status_line();
    let status = if browser.workshop_searching {
        Span::styled("Searching…", theme::accent())
    } else if browser.workshop_tasks.job.is_some() {
        Span::styled(job, theme::accent())
    } else if !browser.workshop_status.is_empty() {
        Span::styled(browser.workshop_status.clone(), theme::dim())
    } else {
        Span::styled(
            format!("{} result(s)", browser.workshop_results.len()),
            theme::dim(),
        )
    };
    lines.push(Line::from(status));
    lines.push(Line::from(""));

    if browser.workshop_results.is_empty() {
        lines.push(Line::from(Span::styled(
            "Type a query and press Enter to search the Workshop.",
            theme::dim(),
        )));
    } else {
        for (i, item) in browser.workshop_results.iter().enumerate() {
            let selected = i == browser.workshop_index;
            let marker = if selected { "> " } else { "  " };
            let (tag, tag_style) = if item.subscribed {
                ("[subscribed] ", Style::default().fg(theme::ONLINE))
            } else {
                ("[          ] ", theme::dim())
            };
            let title_style = if selected {
                theme::accent()
            } else {
                theme::value()
            };
            let mut spans = vec![
                Span::styled(marker, theme::accent()),
                Span::styled(tag, tag_style),
                Span::styled(item.display_title(), title_style),
            ];
            if item.file_size > 0 {
                spans.push(Span::styled(
                    format!(" ({})", convert(item.file_size as f64)),
                    theme::dim(),
                ));
            }
            lines.push(Line::from(spans));
        }
    }

    Paragraph::new(Text::from(lines))
        .block(theme::panel(title))
        .style(theme::base())
}
