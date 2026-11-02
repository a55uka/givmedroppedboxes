//! Tree node primitives: selection state and tri-state display logic.
//!
//! See [`model`](crate::model) for the tree container and
//! [`walk`](crate::walk) for traversal.

use crate::client::Entry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sel {
    Off,
    On,
}

/// What the checkbox shows for a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplaySel {
    Off,
    On,
    Partial,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub entry: Entry,
    /// `None` while not listed yet.
    pub children: Option<Vec<Node>>,
    pub expanded: bool,
    pub sel: Sel,
    /// The file is already in the output directory (download history
    /// and/or a matching local file). Never set for directories.
    pub downloaded: bool,
}

impl Node {
    pub fn new(entry: Entry, sel: Sel, downloaded: bool) -> Self {
        Node {
            entry,
            children: None,
            expanded: false,
            sel,
            downloaded,
        }
    }

    pub fn is_dir(&self) -> bool {
        self.entry.is_dir
    }

    pub fn display_sel(&self) -> DisplaySel {
        if self.sel == Sel::On {
            DisplaySel::On
        } else if self.has_selected_descendant() {
            DisplaySel::Partial
        } else {
            DisplaySel::Off
        }
    }

    fn has_selected_descendant(&self) -> bool {
        match &self.children {
            None => false,
            Some(children) => children
                .iter()
                .any(|c| c.sel == Sel::On || c.has_selected_descendant()),
        }
    }
}

/// Set the selection of a whole subtree (loaded children only).
pub fn set_subtree(node: &mut Node, on: bool) {
    node.sel = if on { Sel::On } else { Sel::Off };
    if let Some(children) = &mut node.children {
        for child in children {
            set_subtree(child, on);
        }
    }
}

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;
