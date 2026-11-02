//! Parsing Dropbox share links and building per-item download URLs.
//!
//! Supported formats: `scl/fo` and legacy `sh` folder links, `scl/fi` and
//! legacy `s` single-file links. `rlkey` is required for `scl` links
//! opened by non-owners.

use std::sync::LazyLock;

use anyhow::{Context, Result, bail};
use regex::Regex;
use url::Url;
use url::form_urlencoded;

use crate::client::Entry;

static LINK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?:https?://)?(?:www\.)?dropbox\.com/(?P<prefix>scl/fo|scl/fi|sh|s)/(?P<key>[^/?#]+)/(?P<hash>[^/?#]+)",
    )
    .expect("static regex")
});

/// A parsed Dropbox share link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedLink {
    /// Normalized full URL (scheme added if missing).
    pub url: String,
    /// `true` for folder links (`scl/fo`, legacy `sh`), `false` for single files.
    pub is_folder: bool,
    pub link_key: String,
    pub secure_hash: String,
    /// Required for `scl` links opened by non-owners; absent on legacy links.
    pub rlkey: Option<String>,
}

pub fn parse_link(input: &str) -> Result<ParsedLink> {
    let trimmed = input.trim();
    let Some(caps) = LINK_RE.captures(trimmed) else {
        bail!(
            "not a Dropbox share link (expected something like https://www.dropbox.com/scl/fo/…)"
        );
    };
    let is_folder = matches!(&caps["prefix"], "scl/fo" | "sh");

    let with_scheme = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let url = Url::parse(&with_scheme).context("malformed URL")?;
    let rlkey = url
        .query_pairs()
        .find(|(k, _)| k == "rlkey")
        .map(|(_, v)| v.into_owned());

    Ok(ParsedLink {
        url: url.to_string(),
        is_folder,
        link_key: caps["key"].to_string(),
        secure_hash: caps["hash"].to_string(),
        rlkey,
    })
}

/// Extract the per-item `secure_hash` segment from an entry's href.
pub fn item_hash_from_href(href: &str, link_key: &str) -> Option<String> {
    let after_scheme = href.split_once("://")?.1;
    let path = after_scheme.split_once('/')?.1;
    let path = path.split(['?', '#']).next().unwrap_or(path);
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let key_index = segments.iter().position(|s| *s == link_key)?;
    segments.get(key_index + 1).map(|s| (*s).to_string())
}

/// Build a direct download URL for an entry href.
///
/// `raw=1` serves the bytes directly (what the maintained scrapers use);
/// `dl=1` forces an attachment. Both may hit an HTML interstitial for
/// flagged files, so the downloader tries the other as a fallback.
pub fn build_download_url(href: &str, raw: bool) -> String {
    let Ok(mut url) = Url::parse(href) else {
        return format!(
            "{href}{}={}",
            if href.contains('?') { "&" } else { "?" },
            if raw { "raw" } else { "dl" }
        );
    };
    let mut pairs: Vec<(String, String)> = url
        .query_pairs()
        .into_owned()
        .filter(|(k, _)| k != "dl" && k != "raw")
        .collect();
    pairs.push(if raw {
        ("raw".into(), "1".into())
    } else {
        ("dl".into(), "1".into())
    });
    let query = form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs)
        .finish();
    url.set_query(Some(&query));
    url.to_string()
}

/// Pull the page title out of the share page HTML (used for the default
/// output directory and the root tree label).
pub fn extract_title(html: &str) -> Option<String> {
    let start = html.find("<title>")? + "<title>".len();
    let end = start + html[start..].find("</title>")?;
    let mut title = html[start..end].trim().to_string();
    for (from, to) in [
        ("&amp;", "&"),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&quot;", "\""),
        ("&#39;", "'"),
    ] {
        title = title.replace(from, to);
    }
    if let Some(stripped) = title.strip_suffix(" - Dropbox") {
        title = stripped.to_string();
    }
    let title = title.trim();
    // the modern /scl pages render a generic client-side title only
    if title.is_empty()
        || title.eq_ignore_ascii_case("dropbox")
        || title.eq_ignore_ascii_case("dropbox.com")
    {
        None
    } else {
        Some(title.to_string())
    }
}

fn percent_decode(s: &str) -> String {
    percent_encoding::percent_decode_str(s)
        .decode_utf8_lossy()
        .into_owned()
}

/// Synthetic entry for single-file links (`scl/fi`, legacy `s`).
pub fn file_entry_from_link(link: &ParsedLink) -> Entry {
    let name = Url::parse(&link.url)
        .ok()
        .and_then(|u| {
            u.path_segments()
                .and_then(|mut segs| segs.next_back().map(percent_decode))
        })
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "file".to_string());
    Entry {
        name,
        is_dir: false,
        size: None,
        href: Some(link.url.clone()),
        hash: None,
    }
}

#[cfg(test)]
#[path = "link_tests.rs"]
mod tests;
