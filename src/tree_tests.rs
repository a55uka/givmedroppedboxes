//! Unit tests for the tree model: selection semantics, walks, stats, and
//! downloaded-flag propagation.

use super::*;
use crate::client::Entry;
use crate::model::{Model, path_key};
use crate::walk::Stats;

fn file(name: &str, size: Option<u64>) -> Entry {
    Entry {
        name: name.into(),
        is_dir: false,
        size,
        href: Some(format!("https://x/{name}")),
        hash: None,
    }
}

fn dir(name: &str) -> Entry {
    Entry {
        name: name.into(),
        is_dir: true,
        size: None,
        href: Some(format!("https://x/{name}")),
        hash: Some("h".into()),
    }
}

fn sample_model() -> Model {
    let mut model = Model::new_root(dir("root"));
    let root = model.root.as_mut().unwrap();
    root.children = Some(vec![
        Node::new(file("a.txt", Some(10)), Sel::Off, false),
        {
            let mut sub = Node::new(dir("sub"), Sel::Off, false);
            sub.children = Some(vec![Node::new(file("b.bin", Some(20)), Sel::Off, false)]);
            sub
        },
        Node::new(dir("unloaded"), Sel::Off, false),
    ]);
    model
}

fn p(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}

#[test]
fn toggle_file() {
    let mut model = sample_model();
    let path = p(&["", "a.txt"]);
    model.toggle_sel(&path);
    assert_eq!(model.node_at(&path).unwrap().display_sel(), DisplaySel::On);
    model.toggle_sel(&path);
    assert_eq!(model.node_at(&path).unwrap().display_sel(), DisplaySel::Off);
}

#[test]
fn toggle_empty_path_is_noop() {
    let mut model = sample_model();
    model.toggle_sel(&[]);
    assert_eq!(model.selected_stats().files, 0);
}

#[test]
fn toggle_dir_selects_subtree_and_future_children() {
    let mut model = sample_model();
    let path = p(&["", "sub"]);
    model.toggle_sel(&path);
    assert_eq!(model.node_at(&path).unwrap().display_sel(), DisplaySel::On);
    assert_eq!(
        model
            .node_at(&p(&["", "sub", "b.bin"]))
            .unwrap()
            .display_sel(),
        DisplaySel::On
    );

    // newly loaded children inherit the promise
    model.insert_children(&p(&["", "unloaded"]), vec![(file("c.txt", Some(5)), false)]);
    assert_eq!(
        model
            .node_at(&p(&["", "unloaded", "c.txt"]))
            .unwrap()
            .display_sel(),
        DisplaySel::Off
    );
    model.toggle_sel(&path); // sub off again
    assert_eq!(
        model
            .node_at(&p(&["", "sub", "b.bin"]))
            .unwrap()
            .display_sel(),
        DisplaySel::Off
    );
}

#[test]
fn partial_display() {
    let mut model = sample_model();
    model.toggle_sel(&p(&["", "sub", "b.bin"]));
    assert_eq!(
        model.node_at(&p(&["", "sub"])).unwrap().display_sel(),
        DisplaySel::Partial
    );
    assert_eq!(
        model.node_at(&p(&[""])).unwrap().display_sel(),
        DisplaySel::Partial
    );
    // selecting a partial dir turns everything under it on
    model.toggle_sel(&p(&["", "sub"]));
    assert_eq!(
        model.node_at(&p(&["", "sub"])).unwrap().display_sel(),
        DisplaySel::On
    );
    assert_eq!(
        model
            .node_at(&p(&["", "sub", "b.bin"]))
            .unwrap()
            .display_sel(),
        DisplaySel::On
    );
}

#[test]
fn stats_and_incompleteness() {
    let mut model = sample_model();
    assert_eq!(model.selected_stats().files, 0);

    model.toggle_sel(&p(&["", "a.txt"]));
    let stats: Stats = model.selected_stats();
    assert_eq!((stats.files, stats.bytes), (1, 10));
    assert!(!stats.incomplete);

    model.toggle_sel(&p(&["", "unloaded"]));
    let stats = model.selected_stats();
    assert_eq!(stats.files, 1);
    assert!(
        stats.incomplete,
        "unloaded selected dir must flag incompleteness"
    );

    model.toggle_sel(&p(&["", "a.txt"]));
    let stats = model.selected_stats();
    assert_eq!(stats.files, 0);
    assert!(stats.incomplete);
}

#[test]
fn collect_pending_resolves_dirs() {
    let mut model = sample_model();
    model.toggle_sel(&p(&["", "sub"]));
    model.toggle_sel(&p(&["", "a.txt"]));
    model.toggle_sel(&p(&["", "unloaded"]));

    let (files, dirs) = model.collect_pending();
    assert_eq!(files.len(), 2); // a.txt and sub/b.bin
    assert_eq!(dirs.len(), 1);
    assert_eq!(dirs[0].path, p(&["root", "unloaded"]));

    model.insert_children(&dirs[0].path, vec![(file("c.txt", Some(5)), false)]);
    let (files, dirs) = model.collect_pending();
    assert!(dirs.is_empty());
    assert_eq!(files.len(), 3);
    assert_eq!(files[2].0, p(&["root", "unloaded", "c.txt"]));
}

#[test]
fn deselect_dir_clears_subtree() {
    let mut model = sample_model();
    model.select_all(true);
    assert_eq!(model.selected_stats().files, 2);
    model.toggle_sel(&p(&["", "sub"]));
    let (files, _) = model.collect_pending();
    assert_eq!(files.len(), 1); // only a.txt
}

#[test]
fn insert_children_carries_downloaded_flag() {
    let mut model = sample_model();
    model.insert_children(
        &p(&["", "unloaded"]),
        vec![(file("c.txt", Some(5)), true), (file("d.txt", None), false)],
    );
    assert!(
        model
            .node_at(&p(&["", "unloaded", "c.txt"]))
            .unwrap()
            .downloaded
    );
    assert!(
        !model
            .node_at(&p(&["", "unloaded", "d.txt"]))
            .unwrap()
            .downloaded
    );
}

#[test]
fn mark_file_downloaded_by_rel_path() {
    let mut model = sample_model();
    model.mark_file_downloaded("sub/b.bin");
    assert!(model.node_at(&p(&["", "sub", "b.bin"])).unwrap().downloaded);
    assert!(!model.node_at(&p(&["", "a.txt"])).unwrap().downloaded);
    // unknown paths are silently ignored
    model.mark_file_downloaded("nope/nothing.txt");
}

#[test]
fn mark_file_downloaded_single_file_root() {
    let mut model = Model::new_root(file("only.txt", Some(3)));
    model.mark_file_downloaded("only.txt");
    assert!(model.root.as_ref().unwrap().downloaded);

    // a different rel path must not mark the root
    let mut model = Model::new_root(file("only.txt", Some(3)));
    model.mark_file_downloaded("other.txt");
    assert!(!model.root.as_ref().unwrap().downloaded);
}

#[test]
fn path_key_joins() {
    assert_eq!(path_key(&p(&["a", "b"])), "a/b");
    assert_eq!(path_key(&[]), "");
}
