//! Shared types for the download pipeline: one resolved file job and the
//! event stream all worker/reporting code communicates through.
//!
//! Every [`DownloadEvent`] from a run is tagged with a run id so the UI
//! can ignore stale events from cancelled runs.

use std::path::PathBuf;
use std::sync::Arc;

use crate::client::ScrapeClient;
use crate::model::Model;
use crate::state::DownloadState;

/// Shared handle to the persistent download history.
pub type SharedState = Arc<std::sync::Mutex<DownloadState>>;

/// One file to download.
#[derive(Debug, Clone)]
pub struct FileJob {
    /// Original path below the share root, for display.
    pub display: String,
    /// Sanitized local path below the output root.
    pub rel_path: PathBuf,
    pub href: String,
    pub size: Option<u64>,
}

pub enum DownloadEvent {
    Connected {
        client: Arc<ScrapeClient>,
        model: Box<Model>,
        out_root: PathBuf,
        state: SharedState,
    },
    ConnectFailed {
        error: String,
    },
    LoadFailed {
        path: Vec<String>,
        error: String,
    },
    Collecting {
        run: u32,
        found: usize,
    },
    Collected {
        run: u32,
        jobs: Vec<FileJob>,
        skipped: usize,
    },
    Started {
        run: u32,
        idx: usize,
    },
    Progress {
        run: u32,
        idx: usize,
        downloaded: u64,
        total: Option<u64>,
    },
    Finished {
        run: u32,
        idx: usize,
        bytes: u64,
    },
    Failed {
        run: u32,
        idx: usize,
        error: String,
    },
    Done {
        run: u32,
    },
}
