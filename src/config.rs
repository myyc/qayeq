/// Application ID (reverse domain notation)
pub const APP_ID: &str = "dev.myyc.qayeq";

/// Application name
pub const APP_NAME: &str = "qayeq";

/// Application version
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// User agent string for web requests
pub const USER_AGENT: &str = concat!(
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko) qayeq/",
    env!("CARGO_PKG_VERSION")
);

/// Default search engine URL template
/// Use {} as placeholder for the search query
pub const DEFAULT_SEARCH_URL: &str = "https://duckduckgo.com/?q={}";

/// Default homepage
pub const DEFAULT_HOMEPAGE: &str = "about:blank";

/// Default new tab page
pub const NEW_TAB_URL: &str = "about:blank";

/// Profile directory name
pub const PROFILES_DIR: &str = "profiles";

/// Default profile name
pub const DEFAULT_PROFILE: &str = "default";

/// History database filename
pub const HISTORY_DB: &str = "history.db";

/// Minimum sidebar width in pixels
pub const MIN_SIDEBAR_WIDTH: i32 = 180;

/// Maximum sidebar width in pixels
pub const MAX_SIDEBAR_WIDTH: i32 = 400;

/// Default sidebar width in pixels
pub const DEFAULT_SIDEBAR_WIDTH: i32 = 250;

// ============================================================================
// Search Engines
// ============================================================================

/// Available search engines: (id, display_name, url_template)
pub const SEARCH_ENGINES: &[(&str, &str, &str)] = &[
    ("duckduckgo", "DuckDuckGo", "https://duckduckgo.com/?q={}"),
    ("google", "Google", "https://www.google.com/search?q={}"),
    ("bing", "Bing", "https://www.bing.com/search?q={}"),
    ("ecosia", "Ecosia", "https://www.ecosia.org/search?q={}"),
    ("custom", "Custom", ""),
];

/// DuckDuckGo autocomplete API URL
pub const DUCKDUCKGO_SUGGESTIONS_URL: &str = "https://duckduckgo.com/ac/?q={}&type=list";

// ============================================================================
// URL Completion
// ============================================================================

/// Debounce delay for URL completion queries (milliseconds)
pub const COMPLETION_DEBOUNCE_MS: u64 = 300;

/// Maximum total completion results to show
pub const COMPLETION_MAX_RESULTS: usize = 8;

/// Maximum history results in completion
pub const HISTORY_COMPLETION_LIMIT: usize = 5;

/// Maximum search suggestion results
pub const SEARCH_SUGGESTIONS_LIMIT: usize = 5;

// ============================================================================
// Containers (Profile-based isolation)
// ============================================================================

/// Container definition: (id, display_name, color_css)
/// Colors are CSS color values for the container indicator
pub const DEFAULT_CONTAINERS: &[(&str, &str, &str)] = &[
    ("personal", "Personal", "#33d17a"),  // Green
    ("work", "Work", "#3584e4"),          // Blue
    ("shopping", "Shopping", "#ff7800"),  // Orange
    ("banking", "Banking", "#9141ac"),    // Purple
];

/// Default container for new tabs
pub const DEFAULT_CONTAINER: &str = "personal";
