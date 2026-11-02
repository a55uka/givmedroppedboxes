//! The lazy file tree for one share link plus per-directory load state.
//!
//! Selection semantics: a directory marked [`Sel::On`] is a *promise* that
//! everything under it is selected, including content not loaded yet — its
//! children inherit `On` when they get listed. Turning a directory off
//! clears the whole subtree (and revokes ancestor promises). A directory
//! that itself is off but has any selected descendant *displays* as
//! partial.

use std::collections::{HashMap, HashSet};

use crate::client::Entry;
use crate::tree::{Node, Sel, set_subtree};

/// The whole tree plus transient per-directory load state.
#[derive(Debug, Default)]
pub struct Model {
    /// The share root itself (a folder, or a file for single-file links).
    pub root: Option<Node>,
    /// Path keys of directories currently being listed.
    pub loading: HashSet<String>,
    /// Path keys of directories whose listing failed.
    pub errors: HashMap<String, String>,
}

/// Identifier path joined for use as a map key. Dropbox names cannot
/// contain `/`, so this is unambiguous.
pub fn path_key(path: &[String]) -> String {
    path.join("/")
}

impl Model {
    pub fn new_root(entry: Entry) -> Self {
        Model {
            root: Some(Node::new(entry, Sel::Off, false)),
            ..Default::default()
        }
    }

    /// Look up a node by full identifier path (index 0 is the root marker).
    pub fn node_at(&self, path: &[String]) -> Option<&Node> {
        if path.is_empty() {
            return None;
        }
        let mut cur = self.root.as_ref()?;
        for name in &path[1..] {
            cur = cur
                .children
                .as_ref()?
                .iter()
                .find(|c| c.entry.name == *name)?;
        }
        Some(cur)
    }

    fn node_at_mut(&mut self, path: &[String]) -> Option<&mut Node> {
        if path.is_empty() {
            return None;
        }
        let mut cur = self.root.as_mut()?;
        for name in &path[1..] {
            cur = cur
                .children
                .as_mut()?
                .iter_mut()
                .find(|c| c.entry.name == *name)?;
        }
        Some(cur)
    }

    /// Toggle selection at a path: off (or partially selected) turns the
    /// whole subtree on; on turns it off. Turning a node off also revokes
    /// the "everything selected" promise of every ancestor, otherwise a
    /// deselected subtree would still be collected under an `On` parent.
    pub fn toggle_sel(&mut self, path: &[String]) {
        let turn_on = match self.node_at(path) {
            Some(node) => node.sel != Sel::On,
            None => return,
        };
        if let Some(node) = self.node_at_mut(path) {
            set_subtree(node, turn_on);
        }
        if !turn_on {
            for len in (1..path.len()).rev() {
                if let Some(ancestor) = self.node_at_mut(&path[..len]) {
                    ancestor.sel = Sel::Off;
                }
            }
        }
    }

    pub fn select_all(&mut self, on: bool) {
        if let Some(root) = self.root.as_mut() {
            set_subtree(root, on);
        }
    }

    /// Insert listing results for a directory; children inherit `On` when
    /// the directory itself is selected. Each entry carries its
    /// already-downloaded flag.
    pub fn insert_children(&mut self, path: &[String], entries: Vec<(Entry, bool)>) {
        self.loading.remove(&path_key(path));
        self.errors.remove(&path_key(path));
        let Some(node) = self.node_at_mut(path) else {
            return;
        };
        let inherit = node.sel == Sel::On;
        node.children = Some(
            entries
                .into_iter()
                .map(|(e, downloaded)| {
                    Node::new(e, if inherit { Sel::On } else { Sel::Off }, downloaded)
                })
                .collect(),
        );
    }

    pub fn mark_loading(&mut self, path: &[String]) {
        self.loading.insert(path_key(path));
        self.errors.remove(&path_key(path));
    }

    pub fn mark_error(&mut self, path: &[String], message: String) {
        self.loading.remove(&path_key(path));
        self.errors.insert(path_key(path), message);
    }

    /// Forget a directory's children so it gets listed again on demand.
    pub fn clear_children(&mut self, path: &[String]) {
        if let Some(node) = self.node_at_mut(path) {
            node.children = None;
            node.expanded = true;
        }
    }

    /// Flag a file as downloaded, addressed by its path below the share
    /// root (`"sub/dir/file.txt"`). Handles the single-file root too.
    pub fn mark_file_downloaded(&mut self, rel: &str) {
        let Some(root) = self.root.as_mut() else {
            return;
        };
        if !root.is_dir() {
            if rel == root.entry.name {
                root.downloaded = true;
            }
            return;
        }
        let mut path = vec![root.entry.name.clone()];
        path.extend(rel.split('/').map(str::to_string));
        if let Some(node) = self.node_at_mut(&path) {
            node.downloaded = true;
        }
    }

    /// Selected files known so far (resolved), plus directories that still
    /// need listing to resolve the rest of the selection.
    pub fn collect_pending(&self) -> (Vec<(Vec<String>, Entry)>, Vec<crate::walk::PendingDir>) {
        let mut files = Vec::new();
        let mut dirs = Vec::new();
        if let Some(root) = &self.root {
            crate::walk::collect(root, Vec::new(), false, &mut files, &mut dirs);
        }
        (files, dirs)
    }

    pub fn selected_stats(&self) -> crate::walk::Stats {
        let mut stats = crate::walk::Stats::default();
        if let Some(root) = &self.root {
            crate::walk::accumulate(root, false, &mut stats);
        }
        stats
    }
}
