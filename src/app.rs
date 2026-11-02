//! Application state and the async event loop.
//!
//! The behavior lives in sibling modules that all `impl App`:
//! `input` (keys), `actions` (downloads/listings), `events` (worker
//! events), plus `run` (per-run state) and `connect` (session setup).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use crossterm::event::{Event, EventStream};
use futures::StreamExt;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tui_tree_widget::TreeState;

use crate::cli::Args;
use crate::client::ScrapeClient;
use crate::jobs::{DownloadEvent, SharedState};
use crate::links::parse_link;
use crate::model::Model;
use crate::run::{RunState, Screen};
use crate::ui;

pub struct LinkState {
    pub text: String,
    pub error: Option<String>,
    pub connecting: bool,
}

pub struct BrowserState {
    pub tree: TreeState<String>,
    pub status: Option<String>,
}

pub struct App {
    pub quit: bool,
    pub jobs_n: usize,
    pub out_override: Option<PathBuf>,
    pub screen: Screen,
    pub link: LinkState,
    pub browser: BrowserState,
    pub run: RunState,
    pub summary: Option<crate::run::SummaryState>,
    pub client: Option<Arc<ScrapeClient>>,
    pub model: Arc<std::sync::Mutex<Model>>,
    pub out_root: Option<PathBuf>,
    /// Persistent "already downloaded" history for the output directory.
    pub state: Option<SharedState>,
    pub spinner: usize,
    pub(crate) dl_tx: mpsc::Sender<DownloadEvent>,
    pub(crate) cancel: CancellationToken,
    pub(crate) next_run: u32,
    last_state_save: Instant,
}

impl App {
    #[cfg(test)]
    pub fn for_tests(dl_tx: mpsc::Sender<DownloadEvent>) -> Self {
        Self::new(
            Args {
                link: None,
                output: None,
                jobs: 3,
            },
            dl_tx,
        )
    }

    fn new(args: Args, dl_tx: mpsc::Sender<DownloadEvent>) -> Self {
        let text = args.link.unwrap_or_default();
        App {
            quit: false,
            jobs_n: args.jobs,
            out_override: args.output,
            screen: Screen::Link,
            link: LinkState {
                text,
                error: None,
                connecting: false,
            },
            browser: BrowserState {
                tree: TreeState::default(),
                status: None,
            },
            run: RunState::new(0),
            summary: None,
            client: None,
            model: Arc::new(std::sync::Mutex::new(Model::default())),
            out_root: None,
            state: None,
            spinner: 0,
            dl_tx,
            cancel: CancellationToken::new(),
            next_run: 1,
            last_state_save: Instant::now(),
        }
    }

    pub async fn run(
        args: Args,
        terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    ) -> Result<()> {
        let (dl_tx, mut dl_rx) = mpsc::channel(1024);
        let mut app = App::new(args, dl_tx);
        if !app.link.text.is_empty() {
            app.submit_link();
        }
        let mut events = EventStream::new();
        let mut tick = tokio::time::interval(Duration::from_millis(200));
        let mut tick_count: u64 = 0;

        while !app.quit {
            terminal.draw(|f| ui::draw(f, &mut app))?;
            tokio::select! {
                _ = tick.tick() => {
                    app.on_tick();
                    tick_count += 1;
                    // Every ~1s force the next frame into a full repaint:
                    // terminals can swallow output written while the pane is
                    // starting up, and diff-based rendering would otherwise
                    // never rewrite an unchanged screen. Resetting the back
                    // buffer is pure memory — unlike Terminal::clear(), it
                    // issues no blocking cursor-position query.
                    if tick_count.is_multiple_of(5) {
                        terminal.current_buffer_mut().reset();
                    }
                }
                ev = events.next() => {
                    let Some(ev) = ev else { bail!("terminal event stream ended") };
                    match ev? {
                        Event::Key(key) if key.kind == crossterm::event::KeyEventKind::Press => app.on_key(key),
                        Event::Paste(text) => app.on_paste(&text),
                        _ => {}
                    }
                }
                Some(event) = dl_rx.recv() => app.on_dl_event(event),
                _ = tokio::signal::ctrl_c() => app.request_quit(),
            }
        }
        app.save_state();
        Ok(())
    }

    fn on_tick(&mut self) {
        self.spinner = self.spinner.wrapping_add(1);
        if self.screen == Screen::Progress && !self.run.collecting {
            self.run.update_speed();
        }
        // persist download history at most every ~2s while downloading
        if self.last_state_save.elapsed() >= Duration::from_secs(2) {
            self.save_state();
        }
    }

    /// Write the download history to `<out_root>/.givmedroppedboxes.json`
    /// if it changed. Cheap no-op when clean.
    pub(crate) fn save_state(&mut self) {
        self.last_state_save = Instant::now();
        if let (Some(state), Some(out_root)) = (&self.state, &self.out_root) {
            state.lock().unwrap().save(out_root);
        }
    }

    /// Parse the pasted link and connect in the background.
    pub(crate) fn submit_link(&mut self) {
        match parse_link(&self.link.text) {
            Err(e) => self.link.error = Some(format!("{e}")),
            Ok(parsed) => {
                self.link.connecting = true;
                self.link.error = None;
                let tx = self.dl_tx.clone();
                let out_override = self.out_override.clone();
                tokio::spawn(async move {
                    let event = crate::connect::establish(parsed, out_override).await;
                    let _ = tx.send(event).await;
                });
            }
        }
    }
}
