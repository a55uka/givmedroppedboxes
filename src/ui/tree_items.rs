//! Building tui-tree-widget items from the tree model, one labelled line
//! per node: checkbox, name, and status/size/downloaded suffixes.

use std::collections::HashSet;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use tui_tree_widget::TreeItem;

use crate::model::{Model, path_key};
use crate::state::DownloadState;
use crate::tree::{DisplaySel, Node};
use crate::ui::KEYBIND_STYLE;
use crate::util::human_size;

pub(super) fn build(
    model: &Model,
    state: Option<&DownloadState>,
) -> Vec<TreeItem<'static, String>> {
    let Some(root) = &model.root else {
        return Vec::new();
    };
    let root_path = vec![root.entry.name.clone()];
    let children = items(
        root.children.as_deref().unwrap_or(&[]),
        model,
        state,
        &root_path,
    );
    let label = label_for(root, model, state, &root_path);
    let item = if root.is_dir() {
        match TreeItem::new(root.entry.name.clone(), label.clone(), children) {
            Ok(item) => item,
            Err(_) => TreeItem::new_leaf(root.entry.name.clone(), label),
        }
    } else {
        TreeItem::new_leaf(root.entry.name.clone(), label)
    };
    vec![item]
}

fn items<'a>(
    children: &[Node],
    model: &Model,
    state: Option<&DownloadState>,
    parent_path: &[String],
) -> Vec<TreeItem<'a, String>> {
    let mut seen: HashSet<String> = HashSet::new();
    children
        .iter()
        .map(|node| {
            // identifiers must be unique among siblings; dropbox enforces
            // unique names per folder, but stay defensive
            let mut id = node.entry.name.clone();
            while !seen.insert(id.clone()) {
                id.push('*');
            }
            let mut path = parent_path.to_vec();
            path.push(id.clone());
            let label = label_for(node, model, state, &path);
            if node.is_dir() {
                let kids = node
                    .children
                    .as_deref()
                    .map(|c| items(c, model, state, &path))
                    .unwrap_or_default();
                match TreeItem::new(id.clone(), label.clone(), kids) {
                    Ok(item) => item,
                    Err(_) => TreeItem::new_leaf(id, label),
                }
            } else {
                TreeItem::new_leaf(id, label)
            }
        })
        .collect()
}

fn checkbox_span(sel: DisplaySel) -> Span<'static> {
    match sel {
        DisplaySel::On => Span::styled("[x]", Style::new().fg(Color::Green).bold()),
        DisplaySel::Partial => Span::styled("[-]", Style::new().fg(Color::Yellow).bold()),
        DisplaySel::Off => Span::styled("[ ]", Style::new().fg(Color::DarkGray)),
    }
}

fn name_span(node: &Node) -> Span<'static> {
    let style = if node.is_dir() {
        Style::new().add_modifier(Modifier::BOLD)
    } else {
        Style::new()
    };
    Span::styled(node.entry.name.clone(), style)
}

/// Load status for a directory: error, in-flight, or known-empty.
fn dir_status_span(node: &Node, model: &Model, path: &[String]) -> Option<Span<'static>> {
    let key = path_key(path);
    if model.errors.contains_key(&key) {
        Some(Span::styled(
            "  · listing failed (r retries)",
            Style::new().fg(Color::Red),
        ))
    } else if model.loading.contains(&key) {
        Some(Span::styled("  · loading…", KEYBIND_STYLE))
    } else if node.children.as_ref().is_some_and(|c| c.is_empty()) {
        Some(Span::styled("  · empty", KEYBIND_STYLE))
    } else {
        None
    }
}

/// How much of this folder is already in the output directory — works for
/// folders that were never listed, too.
fn downloaded_count_span(state: Option<&DownloadState>, path: &[String]) -> Option<Span<'static>> {
    let dir_rel = if path.len() <= 1 {
        String::new()
    } else {
        path[1..].join("/")
    };
    let downloaded = state?.downloaded_under(&dir_rel);
    (downloaded > 0).then(|| {
        Span::styled(
            format!("  · {downloaded} downloaded"),
            Style::new().fg(Color::Green),
        )
    })
}

/// Size and already-downloaded markers for a file row.
fn file_spans(node: &Node) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    if let Some(size) = node.entry.size {
        spans.push(Span::styled(
            format!("  · {}", human_size(size)),
            KEYBIND_STYLE,
        ));
    }
    if node.downloaded {
        spans.push(Span::styled("  ✓", Style::new().fg(Color::Green).bold()));
    }
    spans
}

fn label_for(
    node: &Node,
    model: &Model,
    state: Option<&DownloadState>,
    path: &[String],
) -> Line<'static> {
    let mut spans = vec![
        checkbox_span(node.display_sel()),
        Span::raw(" "),
        name_span(node),
    ];
    if node.is_dir() {
        spans.extend(dir_status_span(node, model, path));
        spans.extend(downloaded_count_span(state, path));
    } else {
        spans.extend(file_spans(node));
    }
    Line::from(spans)
}
