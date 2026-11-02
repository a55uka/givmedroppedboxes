//! The end-of-run summary with retry hint and failure details.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use crate::app::App;
use crate::run::{RowState, SummaryState};
use crate::ui::{KEYBIND_STYLE, keybind_line};
use crate::util::human_size;

pub(super) fn draw(f: &mut Frame, app: &mut App) {
    let Some(summary) = &app.summary else { return };
    let [body, footer] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(f.area());

    let block = Block::bordered()
        .title(" Summary ")
        .border_style(KEYBIND_STYLE);
    let inner = block.inner(body);
    f.render_widget(block, body);

    let mut lines = result_lines(app, summary);
    let failed_lines = failure_lines(app, inner.height, lines.len());
    lines.extend(failed_lines);
    f.render_widget(Paragraph::new(lines), inner);

    let mut binds: Vec<(&str, &str)> = Vec::new();
    if summary.failed > 0 {
        binds.push(("r", "retry failed"));
    }
    binds.push(("b", "back"));
    binds.push(("q", "quit"));
    f.render_widget(Paragraph::new(keybind_line(&binds)), footer);
}

fn result_lines(app: &App, summary: &SummaryState) -> Vec<Line<'static>> {
    let elapsed = summary.elapsed.as_secs();
    let time = format!("{}:{:02}", elapsed / 60, elapsed % 60);
    let mut lines = vec![
        Line::from(Span::raw(format!(
            "{} of {} files downloaded ({}) in {}",
            summary.done,
            summary.total,
            human_size(summary.bytes),
            time
        ))),
        Line::from(vec![
            Span::styled("Output: ", Style::new().fg(Color::DarkGray)),
            Span::raw(summary.out_root.display().to_string()),
        ]),
    ];
    if summary.skipped > 0 {
        lines.push(Line::from(Span::styled(
            format!(
                "{} entries had no download link and were skipped",
                summary.skipped
            ),
            Style::new().fg(Color::Yellow),
        )));
    }
    if let Some(error) = &app.run.global_error {
        lines.push(Line::from(Span::styled(
            error.clone(),
            Style::new().fg(Color::Red),
        )));
    }
    lines
}

/// Up to `max` detail lines for failed jobs, or nothing when all succeeded.
fn failure_lines(app: &App, height: u16, used: usize) -> Vec<Line<'static>> {
    let failed: Vec<&(crate::jobs::FileJob, RowState)> = app
        .run
        .rows
        .iter()
        .filter(|(_, s)| matches!(s, RowState::Failed { .. }))
        .collect();
    if failed.is_empty() {
        return Vec::new();
    }
    let max = height.saturating_sub((used + 2) as u16) as usize; // header + footer rows
    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("failed ({}):", failed.len()),
            Style::new().fg(Color::Red).bold(),
        )),
    ];
    for (job, state) in failed.iter().take(max) {
        let error = match state {
            RowState::Failed { error } => error.clone(),
            _ => String::new(),
        };
        lines.push(Line::from(Span::styled(
            format!("✗ {} — {}", job.display, error),
            Style::new().fg(Color::Red),
        )));
    }
    lines
}
