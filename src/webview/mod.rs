//! WebView module using WebKit2GTK for web rendering.

use gtk4::prelude::*;
use url::Url;
use webkit6::prelude::WebViewExt as WebKitWebViewExt;
use webkit6::{NetworkSession, UserContentManager};

/// Extension trait for WebKitWebView with convenience methods
pub trait WebViewExt {
    fn load_url(&self, url: &Url);
    fn load_url_or_search(&self, input: &str);
    fn current_url(&self) -> Option<Url>;
}

impl WebViewExt for webkit6::WebView {
    fn load_url(&self, url: &Url) {
        self.load_uri(url.as_str());
    }

    fn load_url_or_search(&self, input: &str) {
        use crate::url_bar::{parse_input, build_search_url, UrlBarInput};

        match parse_input(input) {
            UrlBarInput::Url(url) => {
                self.load_uri(url.as_str());
            }
            UrlBarInput::Search(query) => {
                if let Ok(search_url) = build_search_url(&query) {
                    self.load_uri(search_url.as_str());
                }
            }
        }
    }

    fn current_url(&self) -> Option<Url> {
        self.uri().and_then(|uri| Url::parse(&uri).ok())
    }
}

/// Create a new WebKitWebView with profile-specific session and optional content filtering
pub fn create_webview(
    network_session: &NetworkSession,
    user_content_manager: Option<&UserContentManager>,
) -> webkit6::WebView {
    // Create settings
    let settings = webkit6::Settings::new();

    // === Security Settings ===
    // Block automatic popups (require user interaction)
    settings.set_javascript_can_open_windows_automatically(false);

    // Block clipboard access by default (privacy)
    settings.set_javascript_can_access_clipboard(false);

    // Block data: URL navigation (phishing/scam prevention)
    settings.set_allow_top_navigation_to_data_urls(false);

    // Restrict file:// URL access (security)
    settings.set_allow_file_access_from_file_urls(false);
    settings.set_allow_universal_access_from_file_urls(false);

    // === Functionality Settings ===
    // Enable developer tools (can be toggled later)
    settings.set_enable_developer_extras(true);

    // Enable JavaScript
    settings.set_enable_javascript(true);

    // === Performance Settings ===
    // Disable WebKit's smooth scrolling - uses its own kinetic behavior
    // that doesn't match GNOME defaults. Disabling lets GTK handle scrolling more natively.
    settings.set_enable_smooth_scrolling(false);

    // Hardware acceleration - biggest impact on scrolling performance
    settings.set_hardware_acceleration_policy(webkit6::HardwareAccelerationPolicy::Always);

    // Page cache - improves navigation responsiveness
    settings.set_enable_page_cache(true);

    // Set a user agent
    settings.set_user_agent_with_application_details(
        Some(crate::config::APP_NAME),
        Some(crate::config::APP_VERSION),
    );

    // Build the WebView with network session (for cookie persistence) and
    // optionally user content manager (for content blocking)
    let mut builder = webkit6::WebView::builder()
        .settings(&settings)
        .network_session(network_session);

    if let Some(ucm) = user_content_manager {
        builder = builder.user_content_manager(ucm);
    }

    let webview = builder.build();

    // Make it expand to fill space
    webview.set_hexpand(true);
    webview.set_vexpand(true);

    log::debug!("WebView created with security hardening: popups blocked, clipboard restricted, data URLs blocked, file access restricted");

    webview
}
