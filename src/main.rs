mod actions;
mod app;
mod cli;
mod client;
mod collect;
mod connect;
mod download;
mod events;
mod fetch;
mod input;
mod jobs;
mod links;
mod listing;
mod model;
mod run;
mod state;
mod tree;
mod ui;
mod util;
mod walk;

use std::io;

use anyhow::Result;
use clap::Parser;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{DisableBracketedPaste, EnableBracketedPaste};
use ratatui::crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};

type Tty = Terminal<CrosstermBackend<io::Stdout>>;

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::Args::parse();
    let mut terminal = setup_terminal()?;
    let result = app::App::run(args, &mut terminal).await;
    restore_terminal();
    result
}

fn setup_terminal() -> io::Result<Tty> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableBracketedPaste)?;
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        previous_hook(info);
    }));
    Terminal::new(CrosstermBackend::new(io::stdout()))
}

fn restore_terminal() {
    let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableBracketedPaste);
    let _ = disable_raw_mode();
}
