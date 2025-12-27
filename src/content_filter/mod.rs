//! Content filtering module for ad/tracker blocking.
//!
//! Uses WebKit's native UserContentFilter with Safari Content Blocker format,
//! plus uBlock Origin-style scriptlet injection for dynamic ad blocking.

pub mod scriptlets;

use gtk4::gio;
use gtk4::glib;
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use webkit6::{UserContentFilterStore, UserContentManager};

pub use scriptlets::ScriptletManager;

/// Filter list identifiers
const FILTER_EASYLIST: &str = "easylist";
const FILTER_EASYPRIVACY: &str = "easyprivacy";
const FILTER_YOUTUBE_ADS: &str = "youtube-ads";

/// Manages content blocking filters and scriptlet injection
pub struct ContentFilterManager {
    user_content_manager: UserContentManager,
    filter_store: UserContentFilterStore,
    filters_loaded: Cell<bool>,
    data_dir: PathBuf,
    scriptlet_manager: RefCell<Option<Rc<RefCell<ScriptletManager>>>>,
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
            data_dir: data_dir.to_path_buf(),
            scriptlet_manager: RefCell::new(None),
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

        // YouTube ad blocking (network-level, faster than scriptlets)
        self.load_or_compile_filter(
            FILTER_YOUTUBE_ADS,
            "/dev/myyc/qayeq/filters/youtube-ads.json",
        );

        self.filters_loaded.set(true);
    }

    /// Initialize scriptlet injection for YouTube ad blocking
    ///
    /// This loads bundled scriptlets immediately, then fetches updates in background.
    pub fn initialize_scriptlets(&self) {
        let mut manager = ScriptletManager::new(&self.data_dir);

        // Load bundled filters synchronously for immediate protection
        manager.initialize_bundled();

        // Add scripts to user content manager
        manager.add_to_content_manager(&self.user_content_manager);

        let manager = Rc::new(RefCell::new(manager));
        *self.scriptlet_manager.borrow_mut() = Some(Rc::clone(&manager));

        // Fetch updates in background
        glib::spawn_future_local(async move {
            let mut mgr = manager.borrow_mut();
            match mgr.initialize().await {
                Ok(()) => {
                    log::info!("Scriptlets initialized with remote filters");
                }
                Err(e) => {
                    log::warn!("Failed to fetch remote scriptlets: {}", e);
                    // Bundled scripts are already loaded, so we're still protected
                }
            }
        });

        // Schedule periodic updates (every 24 hours)
        self.schedule_scriptlet_updates();
    }

    /// Schedule periodic scriptlet updates
    fn schedule_scriptlet_updates(&self) {
        let manager = match self.scriptlet_manager.borrow().clone() {
            Some(m) => m,
            None => return,
        };

        glib::timeout_add_local(
            std::time::Duration::from_secs(24 * 60 * 60),
            move || {
                let manager = Rc::clone(&manager);
                glib::spawn_future_local(async move {
                    let mut mgr = manager.borrow_mut();
                    match mgr.check_for_updates().await {
                        Ok(true) => log::info!("Scriptlets updated"),
                        Ok(false) => log::debug!("Scriptlets up to date"),
                        Err(e) => log::warn!("Failed to update scriptlets: {}", e),
                    }
                });
                glib::ControlFlow::Continue
            },
        );
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
