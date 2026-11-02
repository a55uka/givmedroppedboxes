//! Running resolved [`FileJob`]s with bounded parallelism.
//!
//! Each file streams to a `*.part` file and is renamed into place on
//! success ([`crate::fetch`]); per-file failures retry with exponential
//! backoff before being reported.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tokio::sync::{Semaphore, mpsc};
use tokio_util::sync::CancellationToken;

use crate::fetch::{FetchCtx, fetch_with_fallback};
use crate::jobs::{DownloadEvent, FileJob};

/// Run all jobs with bounded parallelism, streaming each file to disk.
pub async fn run_downloads(
    http: reqwest::Client,
    out_root: PathBuf,
    jobs: Vec<FileJob>,
    parallel: usize,
    run: u32,
    tx: mpsc::Sender<DownloadEvent>,
    cancel: CancellationToken,
) {
    if let Err(e) = tokio::fs::create_dir_all(&out_root).await {
        let error = format!(
            "cannot create output directory {}: {e:#}",
            out_root.display()
        );
        for idx in 0..jobs.len() {
            let _ = tx
                .send(DownloadEvent::Failed {
                    run,
                    idx,
                    error: error.clone(),
                })
                .await;
        }
        let _ = tx.send(DownloadEvent::Done { run }).await;
        return;
    }

    let semaphore = Arc::new(Semaphore::new(parallel.max(1)));
    let mut handles = Vec::with_capacity(jobs.len());
    for (idx, job) in jobs.into_iter().enumerate() {
        let (semaphore, http, out_root, tx, cancel) = (
            semaphore.clone(),
            http.clone(),
            out_root.clone(),
            tx.clone(),
            cancel.clone(),
        );
        handles.push(tokio::spawn(async move {
            let _permit = semaphore
                .acquire_owned()
                .await
                .expect("semaphore never closed");
            if cancel.is_cancelled() {
                return;
            }
            let _ = tx.send(DownloadEvent::Started { run, idx }).await;
            match download_one(&http, &job, &out_root, &cancel, &tx, run, idx).await {
                Ok(bytes) => {
                    let _ = tx.send(DownloadEvent::Finished { run, idx, bytes }).await;
                }
                Err(e) => {
                    let _ = tx
                        .send(DownloadEvent::Failed {
                            run,
                            idx,
                            error: format!("{e:#}"),
                        })
                        .await;
                }
            }
        }));
    }
    for handle in handles {
        let _ = handle.await;
    }
    let _ = tx.send(DownloadEvent::Done { run }).await;
}

async fn download_one(
    http: &reqwest::Client,
    job: &FileJob,
    out_root: &Path,
    cancel: &CancellationToken,
    tx: &mpsc::Sender<DownloadEvent>,
    run: u32,
    idx: usize,
) -> Result<u64> {
    let dest = out_root.join(&job.rel_path);
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("creating directory {}", parent.display()))?;
    }
    let stem = dest
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file")
        .to_string();
    let part = dest.with_file_name(format!("{stem}.{run}.part"));
    let ctx = FetchCtx {
        http,
        cancel,
        tx,
        run,
        idx,
        part: &part,
        dest: &dest,
    };

    for attempt in 1..=3u32 {
        if cancel.is_cancelled() {
            bail!("cancelled");
        }
        match fetch_with_fallback(&ctx, &job.href).await {
            Ok(bytes) => return Ok(bytes),
            Err(e) => {
                let _ = tokio::fs::remove_file(&part).await;
                if cancel.is_cancelled() || attempt == 3 {
                    return Err(e);
                }
                let delay = Duration::from_secs(1 << (attempt - 1));
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {}
                    _ = cancel.cancelled() => {}
                }
            }
        }
    }
    unreachable!("retry loop returns")
}
