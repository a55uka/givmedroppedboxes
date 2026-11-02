//! Resolving the selection into concrete file jobs.
//!
//! The tree only knows about directories the user expanded. Starting a
//! download walks the selection, lazily listing any selected-but-unexplored
//! directory until every selected file is accounted for.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Result, anyhow, bail};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::client::{Entry, ScrapeClient};
use crate::jobs::{DownloadEvent, FileJob, SharedState};
use crate::model::Model;
use crate::util::sanitize_component;

/// `/sub/dir` path sent to the listing endpoint for a tree path (`""` for
/// the root).
fn sub_path_for(path: &[String]) -> String {
    if path.len() <= 1 {
        String::new()
    } else {
        format!("/{}", path[1..].join("/"))
    }
}

/// The same path, as displayed and stored below the output root.
fn rel_path_for(path: &[String]) -> String {
    if path.len() <= 1 {
        String::new()
    } else {
        path[1..].join("/")
    }
}

/// Determine the already-downloaded flag for freshly listed entries by
/// combining the recorded history with what actually sits on disk.
async fn flag_downloaded(
    state: Option<&SharedState>,
    out_root: &Path,
    base_rel: &str,
    entries: Vec<Entry>,
) -> Vec<(Entry, bool)> {
    let mut flagged = Vec::with_capacity(entries.len());
    for entry in entries {
        let rel = if base_rel.is_empty() {
            entry.name.clone()
        } else {
            format!("{base_rel}/{}", entry.name)
        };
        let downloaded = if entry.is_dir {
            false
        } else {
            let in_state = state.is_some_and(|s| s.lock().unwrap().is_downloaded(&rel));
            crate::state::file_downloaded(in_state, out_root, &rel, entry.size).await
        };
        flagged.push((entry, downloaded));
    }
    flagged
}

/// Turn resolved file entries into download jobs, counting entries that
/// have no download link.
fn build_jobs(files: Vec<(Vec<String>, Entry)>) -> (Vec<FileJob>, usize) {
    let mut jobs = Vec::with_capacity(files.len());
    let mut skipped = 0usize;
    for (path, entry) in files {
        let Some(href) = entry.href else {
            skipped += 1;
            continue;
        };
        jobs.push(FileJob {
            display: path[1..].join("/"),
            rel_path: path[1..].iter().map(|c| sanitize_component(c)).collect(),
            href,
            size: entry.size,
        });
    }
    (jobs, skipped)
}

/// Walk the selection, listing any selected-but-unexplored directories,
/// until every selected file is resolved into a [`FileJob`].
pub async fn collect_files(
    model: Arc<std::sync::Mutex<Model>>,
    client: Arc<ScrapeClient>,
    state: Option<SharedState>,
    out_root: PathBuf,
    run: u32,
    tx: mpsc::Sender<DownloadEvent>,
    cancel: CancellationToken,
) -> Result<Vec<FileJob>> {
    loop {
        if cancel.is_cancelled() {
            bail!("cancelled");
        }
        let (files, dirs) = {
            let model = model.lock().unwrap();
            model.collect_pending()
        };
        if dirs.is_empty() {
            let (jobs, skipped) = build_jobs(files);
            let _ = tx
                .send(DownloadEvent::Collected {
                    run,
                    jobs: jobs.clone(),
                    skipped,
                })
                .await;
            return Ok(jobs);
        }
        let _ = tx
            .send(DownloadEvent::Collecting {
                run,
                found: files.len(),
            })
            .await;
        for dir in dirs {
            if cancel.is_cancelled() {
                bail!("cancelled");
            }
            let sub_path = sub_path_for(&dir.path);
            let entries = list_dir_checked(&client, &sub_path, dir.hash.as_deref()).await?;
            let base_rel = rel_path_for(&dir.path);
            let flagged = flag_downloaded(state.as_ref(), &out_root, &base_rel, entries).await;
            model.lock().unwrap().insert_children(&dir.path, flagged);
        }
    }
}

/// Load one directory's children into the model (browser path), tagging
/// entries that are already in the output directory.
pub async fn load_dir(
    model: &std::sync::Mutex<Model>,
    client: &ScrapeClient,
    state: Option<&SharedState>,
    out_root: &Path,
    path: &[String],
) -> Result<()> {
    let (sub_path, hash) = {
        let model = model.lock().unwrap();
        let node = model
            .node_at(path)
            .ok_or_else(|| anyhow!("folder disappeared"))?;
        (sub_path_for(path), node.entry.hash.clone())
    };
    let entries = list_dir_checked(client, &sub_path, hash.as_deref()).await?;
    let base_rel = rel_path_for(path);
    let flagged = flag_downloaded(state, out_root, &base_rel, entries).await;
    model.lock().unwrap().insert_children(path, flagged);
    Ok(())
}

async fn list_dir_checked(
    client: &ScrapeClient,
    sub_path: &str,
    hash: Option<&str>,
) -> Result<Vec<Entry>> {
    if sub_path.is_empty() {
        return client.list_dir(sub_path, hash).await;
    }
    let Some(hash) = hash else {
        bail!("cannot list this folder: its entry had no link hash");
    };
    client.list_dir(sub_path, Some(hash)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_paths_are_sanitized() {
        let path: Vec<String> = ["..", "weird/name", "ok.txt"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let rel: PathBuf = path.iter().map(|c| sanitize_component(c)).collect();
        assert_eq!(rel, PathBuf::from("_").join("weird_name").join("ok.txt"));
    }

    #[test]
    fn path_helpers() {
        let root = vec!["share".to_string()];
        assert_eq!(sub_path_for(&root), "");
        assert_eq!(rel_path_for(&root), "");
        let nested = ["share", "sub", "dir"]
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        assert_eq!(sub_path_for(&nested), "/sub/dir");
        assert_eq!(rel_path_for(&nested), "sub/dir");
    }
}
