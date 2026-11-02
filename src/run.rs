//! State of a download run and the download-event handlers that mutate it.
//!
//! All events are tagged with a run id; handlers expect the caller
//! ([`crate::events`]) to have checked the id already.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::jobs::FileJob;

/// Which screen the UI is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Link,
    Browser,
    Progress,
    Summary,
}

/// Per-file progress inside one run.
#[derive(Debug, Clone)]
pub enum RowState {
    Pending,
    Active { got: u64, total: Option<u64> },
    Done { bytes: u64 },
    Failed { error: String },
}

/// State of the current (or most recent) download run.
pub struct RunState {
    pub run: u32,
    pub collecting: bool,
    pub found: usize,
    pub rows: Vec<(FileJob, RowState)>,
    pub skipped: usize,
    pub total_known: u64,
    pub has_unknown_total: bool,
    pub global_error: Option<String>,
    pub started: Instant,
    pub speed_bps: f64,
    last_agg: (u64, Instant),
}

/// End-of-run numbers for the summary screen.
pub struct SummaryState {
    pub total: usize,
    pub done: usize,
    pub failed: usize,
    pub bytes: u64,
    pub elapsed: Duration,
    pub skipped: usize,
    pub out_root: PathBuf,
}

impl RunState {
    pub(crate) fn new(run: u32) -> Self {
        RunState {
            run,
            collecting: false,
            found: 0,
            rows: Vec::new(),
            skipped: 0,
            total_known: 0,
            has_unknown_total: false,
            global_error: None,
            started: Instant::now(),
            speed_bps: 0.0,
            last_agg: (0, Instant::now()),
        }
    }

    /// A fresh run re-attempting exactly these previously failed jobs.
    pub(crate) fn prepare_retry(jobs: Vec<FileJob>, run: u32) -> Self {
        let mut state = RunState::new(run);
        state.rows = jobs
            .iter()
            .cloned()
            .map(|j| (j, RowState::Pending))
            .collect();
        state.total_known = jobs.iter().filter_map(|j| j.size).sum();
        state.has_unknown_total = jobs.iter().any(|j| j.size.is_none());
        state
    }

    /// A fresh run that starts by resolving the selection.
    pub(crate) fn collecting(run: u32) -> Self {
        let mut state = RunState::new(run);
        state.collecting = true;
        state
    }

    /// Failed jobs from this run, for the retry screen action.
    pub fn failed_jobs(&self) -> Vec<FileJob> {
        self.rows
            .iter()
            .filter(|(_, state)| matches!(state, RowState::Failed { .. }))
            .map(|(job, _)| job.clone())
            .collect()
    }

    pub fn aggregate_bytes(&self) -> u64 {
        self.rows
            .iter()
            .map(|(_, state)| match state {
                RowState::Done { bytes } => *bytes,
                RowState::Active { got, .. } => *got,
                _ => 0,
            })
            .sum()
    }

    pub fn files_done(&self) -> usize {
        self.rows
            .iter()
            .filter(|(_, s)| matches!(s, RowState::Done { .. }))
            .count()
    }

    /// Smooth the displayed transfer speed (called on each UI tick).
    pub fn update_speed(&mut self) {
        let agg = self.aggregate_bytes();
        let now = Instant::now();
        let dt = now.duration_since(self.last_agg.1).as_secs_f64();
        if dt > 0.05 {
            let instant = agg.saturating_sub(self.last_agg.0) as f64 / dt;
            self.speed_bps = if self.speed_bps == 0.0 {
                instant
            } else {
                self.speed_bps * 0.7 + instant * 0.3
            };
            self.last_agg = (agg, now);
        }
    }

    pub(crate) fn is_current(&self, run: u32) -> bool {
        run == self.run
    }

    pub(crate) fn on_collecting(&mut self, found: usize) {
        self.found = found;
    }

    pub(crate) fn on_collected(&mut self, jobs: Vec<FileJob>, skipped: usize) {
        self.collecting = false;
        self.skipped = skipped;
        self.total_known = jobs.iter().filter_map(|j| j.size).sum();
        self.has_unknown_total = jobs.iter().any(|j| j.size.is_none());
        self.rows = jobs.into_iter().map(|j| (j, RowState::Pending)).collect();
        self.last_agg = (0, Instant::now());
    }

    pub(crate) fn on_started(&mut self, idx: usize) {
        if let Some((job, state)) = self.rows.get_mut(idx) {
            *state = RowState::Active {
                got: 0,
                total: job.size,
            };
        }
    }

    pub(crate) fn on_progress(&mut self, idx: usize, downloaded: u64, total: Option<u64>) {
        if let Some((_, state)) = self.rows.get_mut(idx)
            && matches!(state, RowState::Active { .. })
        {
            *state = RowState::Active {
                got: downloaded,
                total,
            };
        }
    }

    /// Mark a file done; returns its display path so the caller can update
    /// the persistent history and the tree markers.
    pub(crate) fn on_finished(&mut self, idx: usize, bytes: u64) -> Option<String> {
        let (job, state) = self.rows.get_mut(idx)?;
        *state = RowState::Done { bytes };
        Some(job.display.clone())
    }

    /// `idx == usize::MAX` marks a run-wide failure (e.g. collecting the
    /// file list failed).
    pub(crate) fn on_failed(&mut self, idx: usize, error: String) {
        if idx == usize::MAX {
            self.global_error = Some(error);
        } else if let Some((_, state)) = self.rows.get_mut(idx) {
            *state = RowState::Failed { error };
        }
    }

    /// Final numbers for the summary screen.
    pub fn summarize(&self, out_root: &Path) -> SummaryState {
        let done = self.files_done();
        SummaryState {
            total: self.rows.len(),
            done,
            failed: self.rows.len() - done,
            bytes: self.aggregate_bytes(),
            elapsed: self.started.elapsed(),
            skipped: self.skipped,
            out_root: out_root.to_path_buf(),
        }
    }
}
