//! The download progress screen: aggregate gauge plus per-file rows.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, LineGauge, Paragraph};

use crate::app::App;
use crate::jobs::FileJob;
use crate::run::RowState;
use crate::ui::{KEYBIND_STYLE, SPINNER, centered, keybind_line, out_label};
use crate::util::human_size;

pub(super) fn draw(f: &mut Frame, app: &mut App) {
    let out = out_label(app);
    let [body, footer] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(f.area());

    let block = Block::bordered()
        .title(format!(" Downloading → {out} "))
        .border_style(KEYBIND_STYLE);
    let inner = block.inner(body);
    f.render_widget(block, body);

    if app.run.collecting {
        draw_collecting(f, inner, app);
    } else {
        draw_rows(f, inner, app);
    }

    f.render_widget(
        Paragraph::new(keybind_line(&[("c", "cancel"), ("q", "quit")])),
        footer,
    );
}

fn draw_collecting(f: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let spinner = SPINNER[app.spinner % SPINNER.len()];
    let lines = vec![
        Line::from(Span::styled(
            format!(
                "{spinner} collecting file list… {} found so far",
                app.run.found
            ),
            Style::new().fg(Color::Yellow),
        )),
        Line::from(""),
        Line::from(Span::styled("c — cancel", KEYBIND_STYLE)),
    ];
    let area = centered(area, 60, 5);
    f.render_widget(Paragraph::new(lines), area);
}

fn draw_rows(f: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let rows = &app.run.rows;
    let done = app.run.files_done();
    let total = rows.len();
    let bytes = app.run.aggregate_bytes();
    let [gauge_area, list_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
    f.render_widget(
        LineGauge::default()
            .label(gauge_label(app, done, total, bytes))
            .ratio(gauge_ratio(app, done, total))
            .filled_style(Style::new().fg(Color::Green)),
        gauge_area,
    );

    // render rows as plain lines, auto-following the latest activity
    let max_rows = list_area.height.saturating_sub(1) as usize;
    let start = rows.len().saturating_sub(max_rows);
    let lines: Vec<Line> = rows[start..]
        .iter()
        .map(|(job, state)| row_line(job, state))
        .collect();
    f.render_widget(Paragraph::new(lines), list_area);
}

fn gauge_label(app: &App, done: usize, total: usize, bytes: u64) -> String {
    let total_text = if app.run.has_unknown_total {
        format!("of ≥{}", human_size(app.run.total_known))
    } else {
        format!("of {}", human_size(app.run.total_known))
    };
    format!(
        "{}/{} files · {} {} · {}/s",
        done,
        total,
        human_size(bytes),
        total_text,
        human_size(app.run.speed_bps as u64)
    )
}

fn gauge_ratio(app: &App, done: usize, total: usize) -> f64 {
    if app.run.total_known > 0 {
        (app.run.aggregate_bytes() as f64 / app.run.total_known as f64).clamp(0.0, 1.0)
    } else if total > 0 && done == total {
        1.0
    } else {
        0.0
    }
}

fn row_line(job: &FileJob, state: &RowState) -> Line<'static> {
    let name = job.display.clone();
    match state {
        RowState::Pending => Line::from(vec![
            Span::styled("… ", KEYBIND_STYLE),
            Span::styled(name, Style::new().fg(Color::DarkGray)),
        ]),
        RowState::Active { got, total } => Line::from(vec![
            Span::styled("▶ ", Style::new().fg(Color::Cyan)),
            Span::raw(name),
            Span::styled(active_detail(*got, *total), Style::new().fg(Color::Cyan)),
        ]),
        RowState::Done { bytes } => Line::from(vec![
            Span::styled("✓ ", Style::new().fg(Color::Green)),
            Span::raw(name),
            Span::styled(
                format!(" · {}", human_size(*bytes)),
                Style::new().fg(Color::DarkGray),
            ),
        ]),
        RowState::Failed { error } => Line::from(vec![
            Span::styled("✗ ", Style::new().fg(Color::Red)),
            Span::styled(name, Style::new().fg(Color::Red)),
            Span::styled(format!(" · {error}"), Style::new().fg(Color::Red)),
        ]),
    }
}

fn active_detail(got: u64, total: Option<u64>) -> String {
    match total {
        Some(t) if t > 0 => {
            let pct = (got as f64 / t as f64 * 100.0).clamp(0.0, 100.0);
            format!(
                " · {}% · {} / {}",
                pct as u64,
                human_size(got),
                human_size(t)
            )
        }
        _ => format!(" · {}", human_size(got)),
    }
}
