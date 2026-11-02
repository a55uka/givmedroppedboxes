//! Applying [`DownloadEvent`]s from worker tasks to the application state.

use std::sync::Arc;

use crate::app::App;
use crate::client::ScrapeClient;
use crate::jobs::{DownloadEvent, SharedState};
use crate::run::Screen;

impl App {
    pub(crate) fn on_dl_event(&mut self, event: DownloadEvent) {
        match event {
            DownloadEvent::Connected {
                client,
                model,
                out_root,
                state,
            } => self.on_connected(model, client, out_root, state),
            DownloadEvent::ConnectFailed { error } => {
                self.link.connecting = false;
                self.link.error = Some(error);
            }
            DownloadEvent::LoadFailed { path, error } => {
                self.model.lock().unwrap().mark_error(&path, error.clone());
                self.browser.status = Some(error);
            }
            DownloadEvent::Collecting { run, found } => {
                if self.run.is_current(run) {
                    self.run.on_collecting(found);
                }
            }
            DownloadEvent::Collected { run, jobs, skipped } => {
                if self.run.is_current(run) {
                    self.run.on_collected(jobs, skipped);
                }
            }
            DownloadEvent::Started { run, idx } => {
                if self.run.is_current(run) {
                    self.run.on_started(idx);
                }
            }
            DownloadEvent::Progress {
                run,
                idx,
                downloaded,
                total,
            } => {
                if self.run.is_current(run) {
                    self.run.on_progress(idx, downloaded, total);
                }
            }
            DownloadEvent::Finished { run, idx, bytes } => {
                if !self.run.is_current(run) {
                    return;
                }
                if let Some(display) = self.run.on_finished(idx, bytes) {
                    self.record_downloaded(&display, bytes);
                }
            }
            DownloadEvent::Failed { run, idx, error } => {
                if self.run.is_current(run) {
                    self.run.on_failed(idx, error);
                }
            }
            DownloadEvent::Done { run } => {
                if !self.run.is_current(run) || self.screen != Screen::Progress {
                    return;
                }
                let out_root = self.out_root.clone().unwrap_or_default();
                self.summary = Some(self.run.summarize(&out_root));
                self.screen = Screen::Summary;
                self.save_state();
            }
        }
    }

    /// Record a finished file in the persistent history and in the tree
    /// markers.
    fn record_downloaded(&mut self, display: &str, bytes: u64) {
        if let Some(history) = &self.state {
            history.lock().unwrap().mark_downloaded(display, bytes);
        }
        self.model.lock().unwrap().mark_file_downloaded(display);
    }

    /// Switch into the browser for a newly connected share.
    fn on_connected(
        &mut self,
        model: Box<crate::model::Model>,
        client: Arc<ScrapeClient>,
        out_root: std::path::PathBuf,
        state: SharedState,
    ) {
        let title = client.title();
        let is_folder = client.link().is_folder;
        self.out_root = Some(out_root);
        self.client = Some(client);
        self.model = Arc::new(std::sync::Mutex::new(*model));
        self.state = Some(state);
        self.link.connecting = false;
        self.screen = Screen::Browser;
        if is_folder {
            self.browser.tree.open(vec![title.clone()]);
            self.start_load(vec![title]);
        }
    }
}
