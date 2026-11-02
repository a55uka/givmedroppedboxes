//! The link paste screen.

use ratatui::Frame;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};

use crate::app::App;
use crate::ui::{KEYBIND_STYLE, SPINNER, centered};

pub(super) fn draw(f: &mut Frame, app: &mut App) {
    let area = centered(f.area(), 72, 12);
    f.render_widget(Clear, area);

    let block = Block::bordered()
        .title(" givmedroppedboxes ")
        .title_style(Style::new().bold())
        .border_style(KEYBIND_STYLE);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let spinner = SPINNER[app.spinner % SPINNER.len()];
    let status_line = if app.link.connecting {
        Line::from(Span::styled(
            format!("{spinner} connecting to dropbox…"),
            Style::new().fg(Color::Yellow),
        ))
    } else if let Some(error) = &app.link.error {
        Line::from(Span::styled(error.clone(), Style::new().fg(Color::Red)))
    } else {
        Line::from(Span::styled("Enter — open · Esc — quit", KEYBIND_STYLE))
    };

    let mut input = app.link.text.clone();
    input.push('█');
    let lines = vec![
        Line::from("Paste a public Dropbox share link:"),
        Line::from(""),
        Line::from(input),
        Line::from(""),
        status_line,
    ];
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}
