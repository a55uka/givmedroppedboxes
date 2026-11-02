//! Actions the user (or a connect event) triggers: loading listings,
//! starting and retrying downloads, quitting.

use tokio_util::sync::CancellationToken;

use crate::app::App;
use crate::collect::{collect_files, load_dir};
use crate::download::run_downloads;
use crate::jobs::DownloadEvent;
use crate::run::{RunState, Screen};

impl App {
    /// Start a listing for `path` in the background.
    pub(crate) fn start_load(&mut self, path: Vec<String>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        self.model.lock().unwrap().mark_loading(&path);
        let model = self.model.clone();
        let state = self.state.clone();
        let out_root = self.out_root.clone().unwrap_or_default();
        let tx = self.dl_tx.clone();
        tokio::spawn(async move {
            if let Err(e) = load_dir(&model, &client, state.as_ref(), &out_root, &path).await {
                let _ = tx
                    .send(DownloadEvent::LoadFailed {
                        path,
                        error: format!("{e:#}"),
                    })
                    .await;
            }
        });
    }

    /// Clear the selected directory's listing and load it again.
    pub(crate) fn reload_selected(&mut self) {
        let path = self.browser.tree.selected().to_vec();
        self.model.lock().unwrap().clear_children(&path);
        self.start_load(path);
    }

    /// Begin a download run for everything currently selected.
    pub(crate) fn start_download(&mut self) {
        let stats = self.model.lock().unwrap().selected_stats();
        if stats.files == 0 && !stats.incomplete {
            self.browser.status =
                Some("nothing selected — mark files with Space first".to_string());
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(out_root) = self.out_root.clone() else {
            return;
        };

        let (run, cancel) = self.begin_run(RunState::collecting(self.next_run));
        self.screen = Screen::Progress;

        let model = self.model.clone();
        let state = self.state.clone();
        let tx = self.dl_tx.clone();
        let http = client.http();
        let jobs_n = self.jobs_n;
        tokio::spawn(async move {
            match collect_files(
                model,
                client,
                state,
                out_root.clone(),
                run,
                tx.clone(),
                cancel.clone(),
            )
            .await
            {
                Ok(jobs) => run_downloads(http, out_root, jobs, jobs_n, run, tx, cancel).await,
                Err(e) if cancel.is_cancelled() => {
                    let _ = e;
                    let _ = tx.send(DownloadEvent::Done { run }).await;
                }
                Err(e) => {
                    let _ = tx
                        .send(DownloadEvent::Failed {
                            run,
                            idx: usize::MAX,
                            error: format!("collecting file list: {e:#}"),
                        })
                        .await;
                    let _ = tx.send(DownloadEvent::Done { run }).await;
                }
            }
        });
    }

    /// Re-run only the failed jobs of the last run.
    pub(crate) fn retry_failed(&mut self) {
        let failed = self.run.failed_jobs();
        if failed.is_empty() {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(out_root) = self.out_root.clone() else {
            return;
        };

        let (run, cancel) = self.begin_run(RunState::prepare_retry(failed.clone(), self.next_run));
        self.screen = Screen::Progress;

        let tx = self.dl_tx.clone();
        let http = client.http();
        let jobs_n = self.jobs_n;
        tokio::spawn(async move {
            run_downloads(http, out_root, failed, jobs_n, run, tx, cancel).await;
        });
    }

    /// Allocate a run id, reset cancellation, and install `state` as the
    /// current run. Returns the id and its cancellation token.
    fn begin_run(&mut self, mut state: RunState) -> (u32, CancellationToken) {
        let run = self.next_run;
        self.next_run += 1;
        self.cancel = CancellationToken::new();
        state.run = run;
        self.run = state;
        self.summary = None;
        (run, self.cancel.clone())
    }

    pub(crate) fn request_quit(&mut self) {
        self.cancel.cancel();
        self.quit = true;
    }
}
