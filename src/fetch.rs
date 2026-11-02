//! Streaming a single file from Dropbox's direct URLs.
//!
//! `raw=1` serves the bytes directly; `dl=1` forces an attachment. Both
//! may hit an HTML interstitial for flagged files, so the fetch tries one
//! and falls back to the other.

use std::path::Path;

use anyhow::{Context, Result, bail};
use futures::StreamExt;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::jobs::DownloadEvent;
use crate::links::build_download_url;

/// Everything a streaming fetch needs besides the URL.
pub(crate) struct FetchCtx<'a> {
    pub http: &'a reqwest::Client,
    pub cancel: &'a CancellationToken,
    pub tx: &'a mpsc::Sender<DownloadEvent>,
    pub run: u32,
    pub idx: usize,
    pub part: &'a Path,
    pub dest: &'a Path,
}

/// Try the `raw=1` URL, then the `dl=1` URL.
pub(crate) async fn fetch_with_fallback(ctx: &FetchCtx<'_>, href: &str) -> Result<u64> {
    let raw_url = build_download_url(href, true);
    let dl_url = build_download_url(href, false);
    if let Some(bytes) = fetch_to_file(ctx, &raw_url).await? {
        return Ok(bytes);
    }
    if let Some(bytes) = fetch_to_file(ctx, &dl_url).await? {
        return Ok(bytes);
    }
    bail!(
        "dropbox returned a web page instead of the file (the link may be flagged or require a login)"
    )
}

/// Stream one URL to `ctx.part`, then rename onto `ctx.dest`.
///
/// Returns `Ok(None)` when Dropbox answered with an HTML page rather than
/// file bytes (flagged/interstitial) so the caller can try the other URL
/// variant.
async fn fetch_to_file(ctx: &FetchCtx<'_>, url: &str) -> Result<Option<u64>> {
    let resp = tokio::select! {
        r = ctx.http.get(url).send() => r.context("request failed")?,
        _ = ctx.cancel.cancelled() => bail!("cancelled"),
    };
    let resp = resp.error_for_status().context("download request failed")?;

    if is_interstitial(&resp) {
        return Ok(None);
    }

    let total = resp.content_length();
    let mut file = tokio::fs::File::create(ctx.part)
        .await
        .with_context(|| format!("creating {}", ctx.part.display()))?;
    let mut stream = resp.bytes_stream();
    let mut got: u64 = 0;
    let mut since_event: u64 = 0;
    loop {
        let chunk = tokio::select! {
            next = stream.next() => next,
            _ = ctx.cancel.cancelled() => {
                drop(file);
                let _ = tokio::fs::remove_file(ctx.part).await;
                bail!("cancelled");
            }
        };
        let Some(chunk) = chunk else { break };
        let chunk = chunk.context("connection dropped mid-download")?;
        file.write_all(&chunk).await.context("writing download")?;
        got += chunk.len() as u64;
        since_event += chunk.len() as u64;
        if since_event >= 512 * 1024 {
            since_event = 0;
            let _ = ctx
                .tx
                .send(DownloadEvent::Progress {
                    run: ctx.run,
                    idx: ctx.idx,
                    downloaded: got,
                    total,
                })
                .await;
        }
    }
    file.flush().await.context("flushing download")?;
    drop(file);
    if let Some(expected) = total
        && got != expected
    {
        bail!("truncated download ({got} of {expected} bytes)");
    }
    tokio::fs::rename(ctx.part, ctx.dest)
        .await
        .with_context(|| format!("finalizing {}", ctx.dest.display()))?;
    Ok(Some(got))
}

/// True when the response is a Dropbox web page (e.g. a malware flag
/// interstitial) instead of file bytes.
fn is_interstitial(resp: &reqwest::Response) -> bool {
    let header = |name: reqwest::header::HeaderName| {
        resp.headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.to_ascii_lowercase())
    };
    let content_type = header(reqwest::header::CONTENT_TYPE);
    let disposition = header(reqwest::header::CONTENT_DISPOSITION);
    let html_body = content_type.is_some_and(|t| t.contains("text/html"));
    let attachment = disposition.is_some_and(|d| d.contains("attachment"));
    html_body && !attachment
}
