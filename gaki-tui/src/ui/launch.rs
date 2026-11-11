//! The launch-options overlay (modal popup): the different ways Steam can start
//! the selected game (a normal launch, a level editor, an OS-specific binary,
//! ...), scrollable.

use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::browser::Browser;
use crate::ui::paneled_paragraph;
use crate::ui::theme;

/// Build the launch-options overlay content from the browse state. Each option
/// renders as its description, the command (executable + arguments), and an OS
/// tag. The list is scrolled by `browser.launch_scroll`.
pub fn launch(browser: &Browser) -> Paragraph<'static> {
    let items = &browser.launch_options;
    let title = format!("Launch options ({})", items.len());

    if items.is_empty() {
        return paneled_paragraph(Span::styled("No launch options.", theme::dim()), title);
    }

    let lines: Vec<Line<'static>> = items
        .iter()
        .skip(browser.launch_scroll)
        .map(|o| {
            let name = o.display_name();
            let mut spans = vec![Span::styled(name.clone(), theme::value())];
            // Without a description the name is the executable already, so
            // only its arguments follow.
            let command = if name == o.executable {
                o.arguments.trim().to_string()
            } else {
                o.command()
            };
            if !command.is_empty() {
                spans.push(Span::styled(format!("  {}", command), theme::dim()));
            }
            spans.push(Span::styled(format!("  [{}]", o.os_tag()), theme::accent()));
            Line::from(spans)
        })
        .collect();

    paneled_paragraph(lines, title)
}
