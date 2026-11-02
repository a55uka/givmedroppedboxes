//! Persistent record of what has been downloaded, stored in the output
//! directory so the browser can mark previously downloaded files.
//!
//! One file per output directory (`.givmedroppedboxes.json`) may hold the
//! history of several share links, keyed by their link key.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::util::sanitize_component;

pub const STATE_FILE_NAME: &str = ".givmedroppedboxes.json";

#[derive(Serialize, Deserialize)]
struct StateFile {
    version: u32,
    #[serde(default)]
    shares: HashMap<String, ShareRecord>,
}

impl Default for StateFile {
    fn default() -> Self {
        StateFile {
            version: 1,
            shares: HashMap::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
struct ShareRecord {
    /// Relative display path below the share root -> downloaded bytes.
    #[serde(default)]
    files: HashMap<String, u64>,
}

/// Download history for one share link in one output directory.
///
/// Cheap to share: wrap in `Arc<Mutex<…>>`. Mutators set the dirty flag;
/// call [`DownloadState::save`] to persist (it is a no-op when clean).
pub struct DownloadState {
    link_key: String,
    doc: StateFile,
    /// Directory-relative path -> number of downloaded files under it.
    /// `""` is the share root. Derived, not serialized.
    dir_counts: HashMap<String, usize>,
    dirty: bool,
}

impl DownloadState {
    /// Load the state file for `link_key`; missing or corrupt files start
    /// an empty history (the file is rewritten on the next save).
    pub fn load(out_root: &Path, link_key: &str) -> Self {
        let doc = std::fs::read_to_string(out_root.join(STATE_FILE_NAME))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        let mut state = Self {
            link_key: link_key.to_string(),
            doc,
            dir_counts: HashMap::new(),
            dirty: false,
        };
        state.rebuild_counts();
        state
    }

    fn record(&self) -> Option<&ShareRecord> {
        self.doc.shares.get(&self.link_key)
    }

    fn record_mut(&mut self) -> &mut ShareRecord {
        self.doc.shares.entry(self.link_key.clone()).or_default()
    }

    fn rebuild_counts(&mut self) {
        self.dir_counts.clear();
        let rels: Vec<String> = match self.record() {
            Some(record) => record.files.keys().cloned().collect(),
            None => return,
        };
        for rel in rels {
            bump_counts(&mut self.dir_counts, &rel);
        }
    }

    pub fn is_downloaded(&self, rel: &str) -> bool {
        self.record().is_some_and(|r| r.files.contains_key(rel))
    }

    /// Number of downloaded files under a directory (`""` = share root).
    pub fn downloaded_under(&self, dir_rel: &str) -> usize {
        self.dir_counts.get(dir_rel).copied().unwrap_or(0)
    }

    pub fn downloaded_count(&self) -> usize {
        self.record().map_or(0, |r| r.files.len())
    }

    pub fn mark_downloaded(&mut self, rel: &str, bytes: u64) {
        self.record_mut().files.insert(rel.to_string(), bytes);
        bump_counts(&mut self.dir_counts, rel);
        self.dirty = true;
    }

    /// Write the history file; a no-op when nothing changed since the last
    /// successful save.
    pub fn save(&mut self, out_root: &Path) {
        if !self.dirty {
            return;
        }
        if std::fs::create_dir_all(out_root).is_ok() {
            let json = serde_json::to_string_pretty(&self.doc).unwrap_or_default();
            if std::fs::write(out_root.join(STATE_FILE_NAME), json).is_ok() {
                self.dirty = false;
            }
        }
    }
}

fn bump_counts(dir_counts: &mut HashMap<String, usize>, file_rel: &str) {
    *dir_counts.entry(String::new()).or_default() += 1;
    let components: Vec<&str> = file_rel.split('/').collect();
    let mut prefix = String::new();
    for component in &components[..components.len() - 1] {
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(component);
        *dir_counts.entry(prefix.clone()).or_default() += 1;
    }
}

/// True when the local copy at `out_root/<rel>` looks like the shared file:
/// either recorded as downloaded and still present, or present with the
/// expected size (covers files fetched before this state file existed).
pub async fn file_downloaded(
    in_state: bool,
    out_root: &Path,
    rel: &str,
    size: Option<u64>,
) -> bool {
    if !in_state && size.is_none() {
        return false;
    }
    let dest: PathBuf = rel.split('/').map(sanitize_component).collect();
    let Ok(meta) = tokio::fs::metadata(out_root.join(dest)).await else {
        return false;
    };
    meta.is_file()
        && (in_state || size.is_some_and(|expected| expected > 0 && meta.len() == expected))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "gmb-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn empty_when_missing() {
        let dir = tmpdir();
        let state = DownloadState::load(&dir, "key1");
        assert!(!state.is_downloaded("a.txt"));
        assert_eq!(state.downloaded_under(""), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn mark_and_count_including_root() {
        let mut state = DownloadState::load(&tmpdir(), "key1");
        state.mark_downloaded("a.txt", 10);
        state.mark_downloaded("sub/b.txt", 20);
        state.mark_downloaded("sub/deep/c.txt", 30);
        assert!(state.is_downloaded("sub/b.txt"));
        assert_eq!(state.downloaded_count(), 3);
        assert_eq!(state.downloaded_under(""), 3);
        assert_eq!(state.downloaded_under("sub"), 2);
        assert_eq!(state.downloaded_under("sub/deep"), 1);
        assert_eq!(state.downloaded_under("other"), 0);
    }

    #[test]
    fn save_load_roundtrip_and_shares_are_isolated() {
        let dir = tmpdir();
        let mut a = DownloadState::load(&dir, "share-a");
        a.mark_downloaded("x.txt", 5);
        a.save(&dir);

        let mut b = DownloadState::load(&dir, "share-b");
        b.mark_downloaded("y.txt", 6);
        b.save(&dir);

        // share a's history survives share b's save
        let again = DownloadState::load(&dir, "share-a");
        assert!(again.is_downloaded("x.txt"));
        assert!(!again.is_downloaded("y.txt"));
        let again_b = DownloadState::load(&dir, "share-b");
        assert!(again_b.is_downloaded("y.txt"));
        assert_eq!(again_b.downloaded_under(""), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn save_creates_missing_output_dir() {
        let dir = tmpdir().join("nested/inner");
        let mut state = DownloadState::load(&dir, "k");
        state.mark_downloaded("f.txt", 1);
        state.save(&dir);
        assert!(dir.join(STATE_FILE_NAME).exists());
        std::fs::remove_dir_all(dir.ancestors().nth(2).unwrap()).unwrap();
    }
}
