//! The listing protocol: talking to Dropbox's undocumented
//! `list_shared_link_folder_entries` endpoint.
//!
//! It is not an official API and may change without notice; failure modes
//! are surfaced to the user. 400/403/404 responses are transient throttle
//! or expired-session signals and are retried with backoff.

use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use serde::Deserialize;

use crate::client::{Entry, REQUEST_TIMEOUT, ScrapeClient};
use crate::links::item_hash_from_href;

const LISTING_URL: &str = "https://www.dropbox.com/list_shared_link_folder_entries";
const LISTING_ATTEMPTS: u32 = 4;

#[derive(Deserialize)]
struct RawListing {
    entries: Option<Vec<RawEntry>>,
    #[serde(default)]
    has_more_entries: bool,
    next_request_voucher: Option<String>,
}

#[derive(Deserialize)]
struct RawEntry {
    filename: Option<String>,
    #[serde(default)]
    is_dir: bool,
    bytes: Option<u64>,
    href: Option<String>,
}

/// How a single listing attempt failed: throttling and network hiccups are
/// retried, everything else surfaces immediately.
enum ListingFailure {
    Retryable(anyhow::Error),
    Fatal(anyhow::Error),
}

impl ScrapeClient {
    /// List a directory relative to the share root (`""` for the root),
    /// following pagination to the end.
    pub async fn list_dir(&self, sub_path: &str, hash: Option<&str>) -> Result<Vec<Entry>> {
        let mut entries = Vec::new();
        let mut voucher: Option<String> = None;
        loop {
            let (mut page, has_more, next_voucher) =
                self.list_page(sub_path, hash, voucher.as_deref()).await?;
            entries.append(&mut page);
            if !has_more {
                return Ok(entries);
            }
            let Some(v) = next_voucher else {
                bail!("dropbox stopped listing early without a pagination voucher");
            };
            voucher = Some(v);
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    /// One paginated listing request, with retries for the transient
    /// throttle responses Dropbox serves to anonymous scrapers.
    async fn list_page(
        &self,
        sub_path: &str,
        hash: Option<&str>,
        voucher: Option<&str>,
    ) -> Result<(Vec<Entry>, bool, Option<String>)> {
        let hash = hash.unwrap_or(&self.link.secure_hash);
        let mut last_err: Option<anyhow::Error> = None;
        for attempt in 0..LISTING_ATTEMPTS {
            if attempt > 0 {
                // exponential backoff, then a fresh session: 400/403/404
                // from this endpoint mean throttle or expired cookies.
                tokio::time::sleep(Duration::from_secs(1 << (attempt - 1))).await;
                let _ = self.refresh_session().await;
            }
            let csrf = self.csrf.lock().unwrap().clone();
            let form = self.listing_form(&csrf, sub_path, hash, voucher);
            match self.post_listing(&form).await {
                Ok(body) => return parse_listing(&body, &self.link.link_key),
                Err(ListingFailure::Retryable(e)) => last_err = Some(e),
                Err(ListingFailure::Fatal(e)) => return Err(e),
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow!("listing failed")))
    }

    fn listing_form<'a>(
        &'a self,
        csrf: &'a str,
        sub_path: &'a str,
        hash: &'a str,
        voucher: Option<&'a str>,
    ) -> Vec<(&'a str, &'a str)> {
        let mut form = vec![
            ("t", csrf),
            ("link_key", self.link.link_key.as_str()),
            ("secure_hash", hash),
            ("rlkey", self.link.rlkey.as_deref().unwrap_or("")),
            ("sub_path", sub_path),
            ("link_type", "s"),
        ];
        if let Some(v) = voucher {
            form.push(("voucher", v));
        }
        form
    }

    async fn post_listing(
        &self,
        form: &[(&str, &str)],
    ) -> std::result::Result<String, ListingFailure> {
        let resp = self
            .http
            .post(LISTING_URL)
            .header("X-Requested-With", "XMLHttpRequest")
            .timeout(REQUEST_TIMEOUT)
            .form(form)
            .send()
            .await;
        let resp = match resp {
            Ok(r) => r,
            Err(e) => {
                return Err(ListingFailure::Retryable(
                    anyhow!(e).context("network error while listing"),
                ));
            }
        };
        let status = resp.status();
        if matches!(status.as_u16(), 400 | 403 | 404 | 429) {
            return Err(ListingFailure::Retryable(anyhow!(
                "listing throttled by dropbox (HTTP {status})"
            )));
        }
        let body = resp.text().await;
        match body {
            Ok(body) => Ok(body),
            Err(e) => Err(ListingFailure::Fatal(
                anyhow!(e).context("failed to read listing response"),
            )),
        }
    }
}

/// Parse a successful listing response body into entries plus pagination
/// state.
fn parse_listing(body: &str, link_key: &str) -> Result<(Vec<Entry>, bool, Option<String>)> {
    let raw: RawListing = serde_json::from_str(body).map_err(|e| {
        anyhow!("unexpected listing response (link may require a login or password, or the site changed): {e}")
    })?;
    let Some(raw_entries) = raw.entries else {
        bail!("listing response had no entries (the site may have changed)");
    };
    let entries = raw_entries
        .into_iter()
        .filter_map(|re| {
            let name = re.filename?;
            Some(Entry {
                hash: re
                    .href
                    .as_deref()
                    .and_then(|h| item_hash_from_href(h, link_key)),
                name,
                is_dir: re.is_dir,
                size: if re.is_dir { None } else { re.bytes },
                href: re.href,
            })
        })
        .collect();
    Ok((entries, raw.has_more_entries, raw.next_request_voucher))
}
