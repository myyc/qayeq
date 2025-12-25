//! Search suggestion provider using DuckDuckGo API.

use crate::completion::provider::CompletionItem;
use crate::config;
use gtk4::gio;
use soup::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Fetch search suggestions from DuckDuckGo.
pub fn fetch_search_suggestions<F>(
    query: &str,
    _cancellable: Option<&gio::Cancellable>,
    callback: F,
) where
    F: FnOnce(Vec<CompletionItem>) + 'static,
{
    if query.is_empty() {
        callback(Vec::new());
        return;
    }

    let encoded = urlencoding::encode(query);
    let url = config::DUCKDUCKGO_SUGGESTIONS_URL.replace("{}", &encoded);

    let callback = Rc::new(RefCell::new(Some(callback)));

    // Use soup for HTTP request
    let session = soup::Session::new();
    let message = soup::Message::new("GET", &url).expect("Invalid URL");

    session.send_and_read_async(
        &message,
        soup::glib::Priority::DEFAULT,
        soup::gio::Cancellable::NONE,
        move |result| {
            let items = match result {
                Ok(bytes) => {
                    parse_suggestions(&bytes)
                }
                Err(e) => {
                    log::warn!("Failed to fetch suggestions: {}", e);
                    Vec::new()
                }
            };

            if let Some(cb) = callback.borrow_mut().take() {
                cb(items);
            }
        },
    );
}

/// Parse DuckDuckGo suggestion response.
/// Format: ["query", ["suggestion1", "suggestion2", ...]]
fn parse_suggestions(bytes: &[u8]) -> Vec<CompletionItem> {
    let json_str = match std::str::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    // Parse as JSON array
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(json_str);
    let Ok(value) = parsed else {
        return Vec::new();
    };

    // DuckDuckGo returns: ["query", ["sug1", "sug2", ...]]
    let Some(array) = value.as_array() else {
        return Vec::new();
    };

    if array.len() < 2 {
        return Vec::new();
    }

    let Some(suggestions) = array[1].as_array() else {
        return Vec::new();
    };

    suggestions
        .iter()
        .take(config::SEARCH_SUGGESTIONS_LIMIT)
        .filter_map(|v| v.as_str())
        .map(|s| CompletionItem::from_suggestion(s.to_string()))
        .collect()
}
