//! Filter fetching and caching
//!
//! Fetches uBlock Origin filter lists from GitHub and caches them locally.

use super::ScriptletError;
use gtk4::gio;
use soup::prelude::*;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// How often to check for updates (in seconds)
const UPDATE_INTERVAL_SECS: u64 = 24 * 60 * 60; // 24 hours

/// Remote filter sources
const FILTER_SOURCES: &[(&str, &str)] = &[(
    "ublock-quick-fixes",
    "https://raw.githubusercontent.com/uBlockOrigin/uAssets/master/filters/quick-fixes.txt",
)];

/// Bundled filter resource path
const BUNDLED_FILTERS: &str = "/dev/myyc/qayeq/filters/youtube-scriptlets.txt";

/// Manages fetching and caching of filter lists
pub struct FilterFetcher {
    cache_dir: PathBuf,
}

impl FilterFetcher {
    /// Create a new FilterFetcher with the given cache directory
    pub fn new(cache_dir: &Path) -> Self {
        Self {
            cache_dir: cache_dir.to_path_buf(),
        }
    }

    /// Load filters from cache
    pub fn load_cached(&self) -> Result<Vec<String>, ScriptletError> {
        let mut filters = Vec::new();

        // Always include bundled filters
        filters.push(self.load_bundled());

        // Load cached remote filters
        for (name, _url) in FILTER_SOURCES {
            let cache_path = self.cache_dir.join(format!("{}.txt", name));
            if cache_path.exists() {
                match std::fs::read_to_string(&cache_path) {
                    Ok(content) => {
                        log::debug!("Loaded cached filter: {}", name);
                        filters.push(content);
                    }
                    Err(e) => {
                        log::warn!("Failed to read cached filter {}: {}", name, e);
                    }
                }
            }
        }

        if filters.is_empty() {
            return Err(ScriptletError::CacheError("No cached filters found".into()));
        }

        Ok(filters)
    }

    /// Load bundled filters from GResource
    pub fn load_bundled(&self) -> String {
        match gio::resources_lookup_data(BUNDLED_FILTERS, gio::ResourceLookupFlags::NONE) {
            Ok(bytes) => {
                String::from_utf8_lossy(&bytes).to_string()
            }
            Err(e) => {
                log::warn!("Failed to load bundled filters: {}", e);
                // Return minimal YouTube filters as fallback
                Self::fallback_filters()
            }
        }
    }

    /// Fallback filters if bundled resource is not available
    fn fallback_filters() -> String {
        r#"! Fallback YouTube ad blocking rules
youtube.com,m.youtube.com,music.youtube.com##+js(set-constant, ytInitialPlayerResponse.playerAds, undefined)
youtube.com,m.youtube.com,music.youtube.com##+js(set-constant, ytInitialPlayerResponse.adPlacements, undefined)
youtube.com,m.youtube.com,music.youtube.com##+js(json-prune, playerResponse.adPlacements adPlacements playerAds)
"#
        .to_string()
    }

    /// Check if cached filters need updating
    pub fn needs_update(&self) -> bool {
        let metadata_path = self.cache_dir.join("metadata.json");

        if !metadata_path.exists() {
            return true;
        }

        match std::fs::metadata(&metadata_path) {
            Ok(meta) => {
                if let Ok(modified) = meta.modified() {
                    let age = SystemTime::now()
                        .duration_since(modified)
                        .unwrap_or(Duration::MAX);
                    age > Duration::from_secs(UPDATE_INTERVAL_SECS)
                } else {
                    true
                }
            }
            Err(_) => true,
        }
    }

    /// Fetch filters from remote sources
    pub async fn fetch_filters(&self) -> Result<Vec<String>, ScriptletError> {
        let mut filters = Vec::new();

        // Always include bundled filters
        filters.push(self.load_bundled());

        // Fetch from each remote source
        for (name, url) in FILTER_SOURCES {
            log::info!("Fetching filter: {} from {}", name, url);

            match self.fetch_url(url).await {
                Ok(content) => {
                    // Cache the fetched content
                    let cache_path = self.cache_dir.join(format!("{}.txt", name));
                    if let Err(e) = std::fs::write(&cache_path, &content) {
                        log::warn!("Failed to cache filter {}: {}", name, e);
                    }
                    filters.push(content);
                }
                Err(e) => {
                    log::warn!("Failed to fetch {}: {}", name, e);
                    // Try to use cached version
                    let cache_path = self.cache_dir.join(format!("{}.txt", name));
                    if let Ok(cached) = std::fs::read_to_string(&cache_path) {
                        log::info!("Using cached version of {}", name);
                        filters.push(cached);
                    }
                }
            }
        }

        // Update metadata timestamp
        self.update_metadata();

        Ok(filters)
    }

    /// Fetch content from a URL using soup
    async fn fetch_url(&self, url: &str) -> Result<String, ScriptletError> {
        let session = soup::Session::new();
        let message = soup::Message::new("GET", url)
            .map_err(|e| ScriptletError::FetchError(format!("Invalid URL: {}", e)))?;

        // Send request asynchronously
        let bytes = session
            .send_and_read_future(&message, soup::glib::Priority::DEFAULT)
            .await
            .map_err(|e| ScriptletError::FetchError(format!("Request failed: {}", e)))?;

        // Check status code
        let status = message.status();
        if status != soup::Status::Ok {
            return Err(ScriptletError::FetchError(format!(
                "HTTP error: {:?}",
                status
            )));
        }

        String::from_utf8(bytes.to_vec())
            .map_err(|e| ScriptletError::FetchError(format!("Invalid UTF-8: {}", e)))
    }

    /// Update the metadata file timestamp
    fn update_metadata(&self) {
        let metadata_path = self.cache_dir.join("metadata.json");
        let metadata = serde_json::json!({
            "last_updated": SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            "version": 1
        });

        if let Err(e) = std::fs::write(&metadata_path, metadata.to_string()) {
            log::warn!("Failed to update metadata: {}", e);
        }
    }
}
