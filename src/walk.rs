//! Tree traversal: resolving the selection into files and aggregate
//! numbers.
//!
//! Both walks share one rule: a node counts as selected when it (or any
//! ancestor) carries [`Sel::On`].

use crate::client::Entry;
use crate::tree::{Node, Sel};

/// Aggregate numbers for the header/summary.
#[derive(Debug, Default, Clone, Copy)]
pub struct Stats {
    pub files: usize,
    pub bytes: u64,
    /// At least one selected directory hasn't been listed yet, or a file's
    /// size is unknown — the numbers are lower bounds.
    pub incomplete: bool,
}

/// A directory that must be listed before its selected files are known.
#[derive(Debug, Clone)]
pub struct PendingDir {
    /// Full identifier path (root marker at index 0).
    pub path: Vec<String>,
    pub hash: Option<String>,
}

pub fn collect(
    node: &Node,
    path: Vec<String>,
    inherited_on: bool,
    files: &mut Vec<(Vec<String>, Entry)>,
    dirs: &mut Vec<PendingDir>,
) {
    let on = inherited_on || node.sel == Sel::On;
    let path = {
        let mut p = path;
        p.push(node.entry.name.clone());
        p
    };
    if !node.is_dir() {
        if on {
            files.push((path, node.entry.clone()));
        }
        return;
    }
    match &node.children {
        Some(children) => {
            for child in children {
                collect(child, path.clone(), on, files, dirs);
            }
        }
        None if on => dirs.push(PendingDir {
            path,
            hash: node.entry.hash.clone(),
        }),
        None => {}
    }
}

pub fn accumulate(node: &Node, inherited_on: bool, stats: &mut Stats) {
    let on = inherited_on || node.sel == Sel::On;
    if !node.is_dir() {
        if on {
            stats.files += 1;
            match node.entry.size {
                Some(bytes) => stats.bytes = stats.bytes.saturating_add(bytes),
                None => stats.incomplete = true,
            }
        }
        return;
    }
    match &node.children {
        Some(children) => {
            for child in children {
                accumulate(child, on, stats);
            }
        }
        None if on => stats.incomplete = true,
        None => {}
    }
}
