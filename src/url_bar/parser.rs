use crate::config;
use url::Url;

/// Result of parsing URL bar input
pub enum UrlBarInput {
    /// A valid URL to navigate to
    Url(Url),
    /// A search query to send to the search engine
    Search(String),
}

/// Common top-level domains for URL detection
const COMMON_TLDS: &[&str] = &[
    "com", "org", "net", "edu", "gov", "io", "co", "uk", "de", "fr", "jp", "cn",
    "ru", "br", "in", "it", "nl", "au", "es", "pl", "se", "no", "fi", "dk", "be",
    "at", "ch", "cz", "hu", "pt", "gr", "ro", "sk", "bg", "hr", "si", "rs", "ua",
    "me", "dev", "app", "ai", "cloud", "tech", "online", "site", "info", "biz",
];

/// Parse user input from the URL bar
///
/// Rules:
/// 1. If input already has a valid scheme (http://, https://, etc.), use as-is
/// 2. If input looks like a domain (contains dot, no spaces), prepend https://
/// 3. If input is a single word matching common TLD patterns, treat as domain
/// 4. Otherwise, treat as search query
pub fn parse_input(input: &str) -> UrlBarInput {
    let trimmed = input.trim();

    if trimmed.is_empty() {
        return UrlBarInput::Search(String::new());
    }

    // 1. Check if it already has a scheme
    if let Ok(url) = Url::parse(trimmed) {
        if url.scheme() == "http" || url.scheme() == "https" || url.scheme() == "file" {
            return UrlBarInput::Url(url);
        }
    }

    // 2. Check if it looks like a domain (contains dot, no spaces, no special chars that indicate search)
    if !trimmed.contains(' ') && !trimmed.contains('?') && trimmed.contains('.') {
        // Try to parse with https:// prefix
        let with_scheme = format!("https://{}", trimmed);
        if let Ok(url) = Url::parse(&with_scheme) {
            // Verify it has a valid-looking host
            if url.host_str().is_some() {
                return UrlBarInput::Url(url);
            }
        }
    }

    // 3. Check if it's a single word that could be a domain (e.g., "localhost" or "example")
    if !trimmed.contains(' ') && !trimmed.contains('.') {
        // Check for localhost
        if trimmed.eq_ignore_ascii_case("localhost") {
            if let Ok(url) = Url::parse(&format!("http://{}", trimmed)) {
                return UrlBarInput::Url(url);
            }
        }

        // Check if it ends with a known TLD-like pattern (e.g., user typed "google" and we check if "google.com" would work)
        // This is intentionally NOT done - we don't want to guess TLDs
    }

    // 4. Treat as search query
    UrlBarInput::Search(trimmed.to_string())
}

/// Build a search URL for the given query using the default search engine
pub fn build_search_url(query: &str) -> Result<Url, url::ParseError> {
    let encoded = urlencoding::encode(query);
    let url_str = config::DEFAULT_SEARCH_URL.replace("{}", &encoded);
    Url::parse(&url_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_full_url() {
        match parse_input("https://example.com") {
            UrlBarInput::Url(url) => assert_eq!(url.as_str(), "https://example.com/"),
            UrlBarInput::Search(_) => panic!("Expected URL"),
        }
    }

    #[test]
    fn test_domain_without_scheme() {
        match parse_input("google.com") {
            UrlBarInput::Url(url) => assert_eq!(url.as_str(), "https://google.com/"),
            UrlBarInput::Search(_) => panic!("Expected URL"),
        }
    }

    #[test]
    fn test_domain_with_path() {
        match parse_input("github.com/user/repo") {
            UrlBarInput::Url(url) => assert_eq!(url.as_str(), "https://github.com/user/repo"),
            UrlBarInput::Search(_) => panic!("Expected URL"),
        }
    }

    #[test]
    fn test_search_query() {
        match parse_input("rust programming") {
            UrlBarInput::Search(query) => assert_eq!(query, "rust programming"),
            UrlBarInput::Url(_) => panic!("Expected search"),
        }
    }

    #[test]
    fn test_localhost() {
        match parse_input("localhost") {
            UrlBarInput::Url(url) => assert_eq!(url.as_str(), "http://localhost/"),
            UrlBarInput::Search(_) => panic!("Expected URL"),
        }
    }

    #[test]
    fn test_localhost_with_port() {
        match parse_input("localhost:8080") {
            UrlBarInput::Url(url) => assert_eq!(url.as_str(), "https://localhost:8080/"),
            UrlBarInput::Search(_) => panic!("Expected URL"),
        }
    }

    #[test]
    fn test_search_url_building() {
        let url = build_search_url("hello world").unwrap();
        assert!(url.as_str().contains("duckduckgo.com"));
        assert!(url.as_str().contains("hello%20world"));
    }
}
