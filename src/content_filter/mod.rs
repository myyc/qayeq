//! Content filtering module for ad/tracker blocking.
//!
//! Uses WebKit's native UserContentFilter with Safari Content Blocker format.

use gtk4::gio;
use std::cell::Cell;
use std::path::Path;
use webkit6::{UserContentFilterStore, UserContentManager};

/// Filter list identifiers
const FILTER_EASYLIST: &str = "easylist";
const FILTER_EASYPRIVACY: &str = "easyprivacy";

/// Manages content blocking filters
pub struct ContentFilterManager {
    user_content_manager: UserContentManager,
    filter_store: UserContentFilterStore,
    filters_loaded: Cell<bool>,
}

impl ContentFilterManager {
    /// Create a new content filter manager
    pub fn new(data_dir: &Path) -> Self {
        let filters_path = data_dir.join("filters");
        std::fs::create_dir_all(&filters_path).ok();

        let filter_store = UserContentFilterStore::new(
            filters_path.to_str().expect("valid UTF-8 path"),
        );
        let user_content_manager = UserContentManager::new();

        Self {
            user_content_manager,
            filter_store,
            filters_loaded: Cell::new(false),
        }
    }

    /// Get the user content manager for use with WebViews
    pub fn user_content_manager(&self) -> &UserContentManager {
        &self.user_content_manager
    }

    /// Initialize filters - loads from store or compiles from bundled JSON
    pub fn initialize_filters(&self) {
        if self.filters_loaded.get() {
            return;
        }

        // Try to load EasyList
        self.load_or_compile_filter(
            FILTER_EASYLIST,
            "/dev/myyc/qayeq/filters/easylist.json",
        );

        // Try to load EasyPrivacy
        self.load_or_compile_filter(
            FILTER_EASYPRIVACY,
            "/dev/myyc/qayeq/filters/easyprivacy.json",
        );

        self.filters_loaded.set(true);
    }

    fn load_or_compile_filter(&self, identifier: &str, resource_path: &str) {
        let ucm = self.user_content_manager.clone();
        let store = self.filter_store.clone();
        let store_for_compile = store.clone();
        let id = identifier.to_string();
        let resource = resource_path.to_string();

        // First, try to load existing compiled filter
        store.load(
            identifier,
            gio::Cancellable::NONE,
            move |result| {
                match result {
                    Ok(filter) => {
                        log::info!("Loaded compiled filter: {}", id);
                        ucm.add_filter(&filter);
                    }
                    Err(_) => {
                        // Filter not compiled yet, compile from bundled JSON
                        Self::compile_filter_from_resource(&store_for_compile, &ucm, &id, &resource);
                    }
                }
            },
        );
    }

    fn compile_filter_from_resource(
        store: &UserContentFilterStore,
        ucm: &UserContentManager,
        identifier: &str,
        resource_path: &str,
    ) {
        // Load JSON from GResource
        let bytes = match gio::resources_lookup_data(resource_path, gio::ResourceLookupFlags::NONE) {
            Ok(bytes) => bytes,
            Err(e) => {
                log::warn!("Filter resource not found: {} - {}", resource_path, e);
                return;
            }
        };

        let ucm = ucm.clone();
        let id = identifier.to_string();

        store.save(
            identifier,
            &bytes,
            gio::Cancellable::NONE,
            move |result| {
                match result {
                    Ok(filter) => {
                        log::info!("Compiled and saved filter: {}", id);
                        ucm.add_filter(&filter);
                    }
                    Err(e) => {
                        log::error!("Failed to compile filter {}: {}", id, e);
                    }
                }
            },
        );
    }

    /// Remove all filters (for settings toggle)
    #[allow(dead_code)]
    pub fn disable_filters(&self) {
        self.user_content_manager.remove_all_filters();
    }

    /// Re-enable filters
    #[allow(dead_code)]
    pub fn enable_filters(&self) {
        self.filters_loaded.set(false);
        self.initialize_filters();
    }
}
