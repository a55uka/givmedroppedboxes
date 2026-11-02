//! Unit tests for share-link parsing and download-URL building.

use super::*;

#[test]
fn parse_scl_folder_link() {
    let link =
        parse_link("https://www.dropbox.com/scl/fo/abc123/def456?rlkey=xyz789&dl=0").unwrap();
    assert!(link.is_folder);
    assert_eq!(link.link_key, "abc123");
    assert_eq!(link.secure_hash, "def456");
    assert_eq!(link.rlkey.as_deref(), Some("xyz789"));
    assert!(link.url.starts_with("https://www.dropbox.com/scl/fo/"));
}

#[test]
fn parse_link_without_scheme() {
    let link = parse_link("www.dropbox.com/scl/fo/k/h?rlkey=r").unwrap();
    assert!(link.url.starts_with("https://"));
    assert!(link.is_folder);
}

#[test]
fn parse_scl_file_link() {
    let link = parse_link("https://www.dropbox.com/scl/fi/key123/hash9?rlkey=rl1").unwrap();
    assert!(!link.is_folder);
}

#[test]
fn parse_legacy_links() {
    let folder = parse_link("https://www.dropbox.com/sh/abc123/xyzABC?dl=0").unwrap();
    assert!(folder.is_folder);
    assert_eq!(folder.link_key, "abc123");
    assert_eq!(folder.rlkey, None);

    let file = parse_link("https://www.dropbox.com/s/abc123/report.pdf?dl=0").unwrap();
    assert!(!file.is_folder);
}

#[test]
fn reject_non_dropbox() {
    assert!(parse_link("https://example.com/scl/fo/a/b").is_err());
    assert!(parse_link("").is_err());
    assert!(parse_link("https://www.dropbox.com/home/Documents").is_err());
}

#[test]
fn item_hash_extraction() {
    let href = "https://www.dropbox.com/scl/fo/abc123/itemhash?rlkey=xyz&e=1";
    assert_eq!(
        item_hash_from_href(href, "abc123").as_deref(),
        Some("itemhash")
    );
    let legacy = "https://www.dropbox.com/sh/abc123/itemhash/file.txt";
    assert_eq!(
        item_hash_from_href(legacy, "abc123").as_deref(),
        Some("itemhash")
    );
    assert_eq!(item_hash_from_href(href, "otherkey"), None);
}

#[test]
fn download_urls() {
    let href = "https://www.dropbox.com/scl/fo/k/h/item?rlkey=r&dl=0";
    let raw = build_download_url(href, true);
    assert!(raw.contains("raw=1") && !raw.contains("dl="));
    assert!(raw.contains("rlkey=r"));
    let dl = build_download_url(href, false);
    assert!(dl.contains("dl=1") && !dl.contains("raw="));
    assert!(dl.contains("rlkey=r"));
}

#[test]
fn titles() {
    assert_eq!(
        extract_title("<html><title>Photos - Dropbox</title></html>").as_deref(),
        Some("Photos")
    );
    assert_eq!(
        extract_title("<title>A &amp; B</title>").as_deref(),
        Some("A & B")
    );
    assert_eq!(extract_title("<title>   </title>"), None);
    assert_eq!(extract_title("no title"), None);
}

#[test]
fn file_entry_name_decoding() {
    let link = parse_link("https://www.dropbox.com/scl/fi/k/h/my%20report.pdf?rlkey=r").unwrap();
    let entry = file_entry_from_link(&link);
    assert_eq!(entry.name, "my report.pdf");
    assert!(!entry.is_dir);
}
