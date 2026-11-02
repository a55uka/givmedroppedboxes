//! Establishing a scraping session for a share link and building the
//! initial tree model for it.

use std::path::PathBuf;
use std::sync::Arc;

use crate::client::{Entry, ScrapeClient};
use crate::jobs::DownloadEvent;
use crate::links::{ParsedLink, file_entry_from_link};
use crate::model::Model;
use crate::state::{DownloadState, file_downloaded};
use crate::tree::Sel;
use crate::util::sanitize_component;

/// Connect to a share link and produce the `Connected` (or
/// `ConnectFailed`) event the app loop dispatches on.
pub async fn establish(parsed: ParsedLink, out_override: Option<PathBuf>) -> DownloadEvent {
    match ScrapeClient::connect(parsed).await {
        Ok(client) => connected_event(client, out_override).await,
        Err(e) => DownloadEvent::ConnectFailed {
            error: format!("{e:#}"),
        },
    }
}

async fn connected_event(
    client: Arc<ScrapeClient>,
    out_override: Option<PathBuf>,
) -> DownloadEvent {
    let is_folder = client.link().is_folder;
    let title = client.title();
    let root = if is_folder {
        folder_root(&client, &title)
    } else {
        file_entry_from_link(client.link())
    };
    let out_root = out_override.unwrap_or_else(|| PathBuf::from(sanitize_component(&title)));
    let state = Arc::new(std::sync::Mutex::new(DownloadState::load(
        &out_root,
        &client.link().link_key,
    )));
    let mut model = Model::new_root(root);
    if !is_folder {
        mark_single_file_root(&mut model, &state, &out_root).await;
    }
    DownloadEvent::Connected {
        client,
        model: Box::new(model),
        out_root,
        state,
    }
}

fn folder_root(client: &ScrapeClient, title: &str) -> Entry {
    Entry {
        name: title.to_string(),
        is_dir: true,
        size: None,
        href: Some(client.link().url.clone()),
        hash: Some(client.link().secure_hash.clone()),
    }
}

/// Single-file links start pre-selected; flag them as downloaded when the
/// file is already on disk.
async fn mark_single_file_root(
    model: &mut Model,
    state: &Arc<std::sync::Mutex<DownloadState>>,
    out_root: &std::path::Path,
) {
    let root = model.root.as_mut().expect("root");
    root.sel = Sel::On;
    let name = root.entry.name.clone();
    let size = root.entry.size;
    let in_state = state.lock().unwrap().is_downloaded(&name);
    let downloaded = file_downloaded(in_state, out_root, &name, size).await;
    model.root.as_mut().expect("root").downloaded = downloaded;
}
