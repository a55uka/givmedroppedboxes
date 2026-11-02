//! Session establishment for anonymous access to public Dropbox share
//! links.
//!
//! The listing protocol itself lives in [`crate::listing`].

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use reqwest::cookie::{CookieStore, Jar};
use url::Url;

use crate::links::{ParsedLink, extract_title};

pub(crate) const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";
pub(crate) const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// One entry in a shared folder listing.
#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    /// `None` for directories (and the rare file without a known size).
    pub size: Option<u64>,
    /// Per-item URL, used for downloads and to extract the item's hash.
    pub href: Option<String>,
    /// The item's own `secure_hash` from its href — required to list this
    /// directory's contents (the parent's hash is rejected for subfolders).
    pub hash: Option<String>,
}

/// Client for one share link. Cheap to clone via [`Arc`]; safe to share
/// across tasks (interior state is behind locks).
pub struct ScrapeClient {
    pub(crate) http: reqwest::Client,
    pub(crate) jar: Arc<Jar>,
    pub(crate) link: ParsedLink,
    pub(crate) csrf: std::sync::Mutex<String>,
    title: std::sync::Mutex<String>,
}

impl ScrapeClient {
    /// Create a client: opens the share page once to obtain the session
    /// cookies (including the CSRF token the listing endpoint requires).
    pub async fn connect(link: ParsedLink) -> Result<Arc<Self>> {
        let jar = Arc::new(Jar::default());
        let http = reqwest::Client::builder()
            .cookie_provider(jar.clone())
            .user_agent(USER_AGENT)
            .connect_timeout(Duration::from_secs(15))
            .build()
            .context("building HTTP client")?;
        let client = Self {
            http,
            jar,
            title: std::sync::Mutex::new(format!("dropbox-{}", link.link_key)),
            link,
            csrf: std::sync::Mutex::new(String::new()),
        };
        if let Some(title) = client.refresh_session().await? {
            *client.title.lock().unwrap() = title;
        }
        Ok(Arc::new(client))
    }

    pub fn http(&self) -> reqwest::Client {
        self.http.clone()
    }

    pub fn link(&self) -> &ParsedLink {
        &self.link
    }

    pub fn title(&self) -> String {
        self.title.lock().unwrap().clone()
    }

    /// GET the share page to (re)establish the cookie session; returns the
    /// page title when the body was fetched successfully.
    pub(crate) async fn refresh_session(&self) -> Result<Option<String>> {
        let resp = self
            .http
            .get(&self.link.url)
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
            .context("failed to reach the share page")?
            .error_for_status()
            .context("the share page returned an error")?;
        let body = resp.text().await.unwrap_or_default();

        let cookies = self
            .jar
            .cookies(&Url::parse("https://www.dropbox.com").expect("static url"))
            .and_then(|v| v.to_str().ok().map(str::to_string))
            .unwrap_or_default();
        let csrf = cookies
            .split(';')
            .find_map(|c| c.trim().strip_prefix("__Host-js_csrf="))
            .ok_or_else(|| {
                anyhow!("the share page did not set the expected session cookie (link may require a login or password)")
            })?
            .to_string();
        *self.csrf.lock().unwrap() = csrf;
        Ok(extract_title(&body))
    }
}
