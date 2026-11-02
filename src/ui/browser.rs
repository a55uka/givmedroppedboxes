//! The share browser: header stats, tree, and keybind footer.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use tui_tree_widget::Tree;

use crate::app::App;
use crate::ui::{KEYBIND_STYLE, keybind_line, out_label, tree_items};
use crate::util::human_size;

pub(super) fn draw(f: &mut Frame, app: &mut App) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(5),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(f.area());

    let out = out_label(app);
    let (title, marked) = {
        // scope keeps the state/model guards from spanning the &mut draws
        let state_guard = app.state.as_ref().map(|s| s.lock().unwrap());
        let already = state_guard.as_deref().map_or(0, |s| s.downloaded_count());
        let model = app.model.lock().unwrap();
        let title = model
            .root
            .as_ref()
            .map(|r| r.entry.name.clone())
            .unwrap_or_default();
        let marked = marked_label(model.selected_stats(), already);
        (title, marked)
    };

    draw_header(f, header, &title, &out, marked);
    draw_tree(f, body, app, &title);
    draw_footer(f, footer, app);
}

fn draw_header(f: &mut Frame, area: ratatui::layout::Rect, title: &str, out: &str, marked: String) {
    let marked_text = marked;
    let block = Block::bordered()
        .title(" givmedroppedboxes ")
        .border_style(KEYBIND_STYLE);
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled("Share:  ", Style::new().fg(Color::DarkGray)),
                Span::styled(title.to_string(), Style::new().bold()),
            ]),
            Line::from(vec![
                Span::styled("Output: ", Style::new().fg(Color::DarkGray)),
                Span::styled(out, Style::new()),
            ]),
            Line::from(vec![
                Span::styled("Marked: ", Style::new().fg(Color::DarkGray)),
                Span::styled(marked_text, Style::new().fg(Color::Green)),
            ]),
        ]),
        inner,
    );
}

fn draw_tree(f: &mut Frame, area: ratatui::layout::Rect, app: &mut App, title: &str) {
    let items = {
        let state_guard = app.state.as_ref().map(|s| s.lock().unwrap());
        let model = app.model.lock().unwrap();
        tree_items::build(&model, state_guard.as_deref())
    };
    // the guards above are dropped before the widget takes &mut app
    let breadcrumb = {
        let path = app.browser.tree.selected();
        if path.len() <= 1 {
            title.to_string()
        } else {
            format!("{} / {}", title, path[1..].join(" / "))
        }
    };
    match Tree::new(&items) {
        Ok(tree) => {
            let widget = tree
                .block(
                    Block::bordered()
                        .title(format!(" {breadcrumb} "))
                        .border_style(KEYBIND_STYLE),
                )
                .highlight_symbol("▌ ")
                .highlight_style(
                    Style::new()
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                );
            f.render_stateful_widget(widget, area, &mut app.browser.tree);
        }
        Err(e) => {
            f.render_widget(Paragraph::new(format!("could not render tree: {e}")), area);
        }
    }
}

fn draw_footer(f: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let footer_line = if let Some(status) = &app.browser.status {
        Line::from(Span::styled(status.clone(), Style::new().fg(Color::Yellow)))
    } else {
        keybind_line(&[
            ("↑↓/jk", "move"),
            ("←→/hl", "expand"),
            ("Space", "select"),
            ("a", "all"),
            ("n", "none"),
            ("r", "reload"),
            ("d", "download"),
            ("q", "quit"),
        ])
    };
    f.render_widget(Paragraph::new(footer_line), area);
}

/// `"<n> files (<size>)"` plus a lower-bound marker when incomplete, and
/// the already-downloaded total when there is one.
fn marked_label(stats: crate::walk::Stats, already: usize) -> String {
    let file_word = if stats.files == 1 {
        "1 file".to_string()
    } else {
        format!("{} files", stats.files)
    };
    let size_text = if stats.incomplete {
        format!("{}+ ({})", human_size(stats.bytes), file_word)
    } else {
        format!("{} ({})", file_word, human_size(stats.bytes))
    };
    if already > 0 {
        format!("{size_text} · {already} already downloaded")
    } else {
        size_text
    }
}
