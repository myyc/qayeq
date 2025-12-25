//! History-based completion provider.

use crate::completion::provider::CompletionItem;
use crate::config;
use crate::history::HistoryDatabase;
use url::Url;

/// Provide completions from browsing history.
/// Results are sorted by relevance:
/// 1. URLs whose host starts with the query (e.g., "goo" matches "google.com")
/// 2. Higher visit count
/// 3. More recent visits
pub fn get_history_completions(
    history: &HistoryDatabase,
    query: &str,
) -> Vec<CompletionItem> {
    if query.is_empty() {
        return Vec::new();
    }

    match history.search(query, config::HISTORY_COMPLETION_LIMIT * 2) {
        Ok(mut entries) => {
            let query_lower = query.to_lowercase();

            // Sort by relevance
            entries.sort_by(|a, b| {
                // Check if host starts with query (highest priority)
                let a_host_prefix = url_host_starts_with(&a.url, &query_lower);
                let b_host_prefix = url_host_starts_with(&b.url, &query_lower);

                // Host prefix match wins
                b_host_prefix.cmp(&a_host_prefix)
                    // Then by visit count
                    .then_with(|| b.visit_count.cmp(&a.visit_count))
                    // Then by recency
                    .then_with(|| b.last_visit_time.cmp(&a.last_visit_time))
            });

            // Take only the limit after sorting
            entries.truncate(config::HISTORY_COMPLETION_LIMIT);

            entries
                .into_iter()
                .map(|entry| {
                    CompletionItem::from_history(
                        entry.title.unwrap_or_else(|| entry.url.clone()),
                        entry.url,
                        None, // TODO: Add favicon support
                        entry.visit_count as u32,
                    )
                })
                .collect()
        }
        Err(e) => {
            log::warn!("Failed to search history for completion: {}", e);
            Vec::new()
        }
    }
}

/// Check if a URL's host starts with the given prefix
fn url_host_starts_with(url_str: &str, prefix: &str) -> bool {
    if let Ok(url) = Url::parse(url_str) {
        if let Some(host) = url.host_str() {
            // Strip "www." prefix for matching
            let host_clean = host.strip_prefix("www.").unwrap_or(host);
            return host_clean.to_lowercase().starts_with(prefix);
        }
    }
    false
}
