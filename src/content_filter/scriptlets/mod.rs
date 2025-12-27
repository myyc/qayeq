//! Scriptlet injection system for uBlock Origin-style ad blocking
//!
//! This module provides the ability to inject JavaScript scriptlets into web pages
//! to block ads and trackers, similar to uBlock Origin.

mod builtin;
mod fetcher;
mod parser;

pub use fetcher::FilterFetcher;
pub use parser::ScriptletRule;

use std::path::Path;
use webkit6::{UserContentInjectedFrames, UserContentManager, UserScript, UserScriptInjectionTime};

/// Manages scriptlet injection
pub struct ScriptletManager {
    fetcher: FilterFetcher,
    rules: Vec<ScriptletRule>,
    scripts: Vec<UserScript>,
}

impl ScriptletManager {
    /// Create a new ScriptletManager with the given data directory for caching
    pub fn new(data_dir: &Path) -> Self {
        let cache_dir = data_dir.join("scriptlets");
        if let Err(e) = std::fs::create_dir_all(&cache_dir) {
            log::warn!("Failed to create scriptlets cache directory: {}", e);
        }

        Self {
            fetcher: FilterFetcher::new(&cache_dir),
            rules: Vec::new(),
            scripts: Vec::new(),
        }
    }

    /// Initialize scriptlets - load from cache or fetch from remote
    ///
    /// This should be called during application startup.
    pub async fn initialize(&mut self) -> Result<(), ScriptletError> {
        log::info!("Initializing scriptlet manager...");

        // Try to load from cache first
        let filters = match self.fetcher.load_cached() {
            Ok(cached) => {
                log::info!("Loaded scriptlets from cache");
                // Check for updates in background if cache is stale
                if self.fetcher.needs_update() {
                    log::info!("Cache is stale, fetching updates...");
                    match self.fetcher.fetch_filters().await {
                        Ok(fresh) => fresh,
                        Err(e) => {
                            log::warn!("Failed to fetch updates, using cache: {}", e);
                            cached
                        }
                    }
                } else {
                    cached
                }
            }
            Err(_) => {
                // No cache, must fetch
                log::info!("No cache found, fetching scriptlets...");
                self.fetcher.fetch_filters().await?
            }
        };

        self.parse_and_compile(&filters);
        log::info!(
            "Scriptlet manager initialized with {} rules, {} scripts",
            self.rules.len(),
            self.scripts.len()
        );

        Ok(())
    }

    /// Initialize with bundled filters only (synchronous, no network)
    pub fn initialize_bundled(&mut self) {
        log::info!("Initializing scriptlet manager with bundled filters...");

        let bundled = self.fetcher.load_bundled();
        self.parse_and_compile(&[bundled]);

        log::info!(
            "Scriptlet manager initialized with {} bundled rules",
            self.rules.len()
        );
    }

    /// Parse filter text and compile to UserScripts
    fn parse_and_compile(&mut self, filters: &[String]) {
        self.rules.clear();
        self.scripts.clear();

        for filter_text in filters {
            for line in filter_text.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('!') {
                    continue;
                }

                if let Some(rule) = ScriptletRule::parse(line) {
                    // Generate JavaScript for this rule
                    if let Some(js) = builtin::generate_script(&rule) {
                        let allow_list = rule.webkit_allow_list();
                        let allow_list_refs: Vec<&str> =
                            allow_list.iter().map(|s| s.as_str()).collect();

                        let user_script = UserScript::new(
                            &js,
                            UserContentInjectedFrames::AllFrames,
                            UserScriptInjectionTime::Start, // Inject at document start
                            &allow_list_refs,
                            &[], // No block list
                        );

                        self.scripts.push(user_script);
                        self.rules.push(rule);
                    }
                }
            }
        }

        log::debug!(
            "Compiled {} scriptlet rules into {} scripts",
            self.rules.len(),
            self.scripts.len()
        );
    }

    /// Add all compiled scripts to a UserContentManager
    pub fn add_to_content_manager(&self, ucm: &UserContentManager) {
        for script in &self.scripts {
            ucm.add_script(script);
        }
        log::info!("Added {} user scripts to content manager", self.scripts.len());
    }

    /// Check for and apply filter updates
    ///
    /// Returns true if updates were applied.
    pub async fn check_for_updates(&mut self) -> Result<bool, ScriptletError> {
        if !self.fetcher.needs_update() {
            return Ok(false);
        }

        log::info!("Checking for scriptlet updates...");
        let filters = self.fetcher.fetch_filters().await?;
        self.parse_and_compile(&filters);
        log::info!("Scriptlets updated: {} rules", self.rules.len());

        Ok(true)
    }

    /// Get the number of loaded rules
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// Get the number of compiled scripts
    pub fn script_count(&self) -> usize {
        self.scripts.len()
    }
}

/// Errors that can occur during scriptlet operations
#[derive(Debug)]
pub enum ScriptletError {
    /// Failed to fetch filters from remote
    FetchError(String),
    /// Failed to read/write cache
    CacheError(String),
    /// Failed to parse filters
    ParseError(String),
}

impl std::fmt::Display for ScriptletError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScriptletError::FetchError(msg) => write!(f, "Fetch error: {}", msg),
            ScriptletError::CacheError(msg) => write!(f, "Cache error: {}", msg),
            ScriptletError::ParseError(msg) => write!(f, "Parse error: {}", msg),
        }
    }
}

impl std::error::Error for ScriptletError {}
