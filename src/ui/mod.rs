//! All drawing. Pure rendering: reads state, never blocks on the network.
//!
//! One submodule per screen, plus shared styling helpers here.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::app::App;
use crate::run::Screen;

mod browser;
mod link;
mod progress;
mod summary;
mod tree_items;

pub(crate) const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

pub(crate) const KEYBIND_STYLE: Style = Style::new().fg(Color::DarkGray);
pub(crate) const HINT_STYLE: Style = Style::new().fg(Color::Cyan);

pub fn draw(f: &mut Frame, app: &mut App) {
    match app.screen {
        Screen::Link => link::draw(f, app),
        Screen::Browser => browser::draw(f, app),
        Screen::Progress => progress::draw(f, app),
        Screen::Summary => summary::draw(f, app),
    }
}

/// Render area centered inside `area`, clamped to fit.
pub(crate) fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    }
}

/// One `key action` pair; joined with `·` separators.
pub(crate) fn keybind_line<'a>(binds: &[(&'a str, &'a str)]) -> Line<'a> {
    let mut spans = Vec::with_capacity(binds.len() * 4);
    for (i, (key, action)) in binds.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" · ", KEYBIND_STYLE));
        }
        spans.push(Span::styled((*key).to_string(), HINT_STYLE));
        spans.push(Span::styled(format!(" {action}"), KEYBIND_STYLE));
    }
    Line::from(spans)
}

/// The output directory as a display string.
pub(crate) fn out_label(app: &App) -> String {
    app.out_root
        .as_deref()
        .map(|p| p.display().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer_text(terminal: &mut ratatui::Terminal<ratatui::backend::TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol().to_string())
            .collect()
    }

    fn test_app() -> App {
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        App::for_tests(tx)
    }

    #[test]
    fn link_screen_renders() {
        let mut app = test_app();
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let text = buffer_text(&mut terminal);
        assert!(
            text.contains("Paste a public Dropbox share link"),
            "got: {text:?}"
        );
        assert!(text.contains("givmedroppedboxes"));
    }

    #[test]
    fn centered_clamps_to_area() {
        let area = Rect::new(0, 0, 40, 10);
        assert_eq!(centered(area, 72, 12), Rect::new(0, 0, 40, 10));
        assert_eq!(centered(area, 20, 6), Rect::new(10, 2, 20, 6));
    }
}
