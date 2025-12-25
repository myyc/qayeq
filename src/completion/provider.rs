//! Completion item types and providers.

use gtk4::gdk_pixbuf::Pixbuf;

/// Type of completion item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionType {
    /// From browsing history.
    History,
    /// From search engine suggestions.
    SearchSuggestion,
}

/// A single completion item.
#[derive(Debug, Clone)]
pub struct CompletionItem {
    /// Type of completion.
    pub completion_type: CompletionType,
    /// Title or suggestion text.
    pub title: String,
    /// URL for history items, None for search suggestions.
    pub url: Option<String>,
    /// Favicon for history items.
    pub favicon: Option<Pixbuf>,
    /// Visit count for history items (for sorting).
    pub visit_count: u32,
}

impl CompletionItem {
    /// Create a new history completion item.
    pub fn from_history(title: String, url: String, favicon: Option<Pixbuf>, visit_count: u32) -> Self {
        Self {
            completion_type: CompletionType::History,
            title,
            url: Some(url),
            favicon,
            visit_count,
        }
    }

    /// Create a new search suggestion item.
    pub fn from_suggestion(suggestion: String) -> Self {
        Self {
            completion_type: CompletionType::SearchSuggestion,
            title: suggestion,
            url: None,
            favicon: None,
            visit_count: 0,
        }
    }

    /// Get the display text for this item.
    pub fn display_text(&self) -> &str {
        &self.title
    }

    /// Get the action URL - either the URL for history or search URL for suggestions.
    pub fn action_url(&self, search_url_template: &str) -> String {
        match &self.url {
            Some(url) => url.clone(),
            None => {
                // Build search URL from suggestion
                let encoded = urlencoding::encode(&self.title);
                search_url_template.replace("{}", &encoded)
            }
        }
    }
}
