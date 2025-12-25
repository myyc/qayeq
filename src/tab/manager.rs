use crate::application::QayeqApplication;
use crate::download;
use crate::recently_closed::ClosedTab;
use crate::tab::TabRow;
use crate::webview::{self, WebViewExt as QayeqWebViewExt};
use gtk4::prelude::*;
use gtk4::glib;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::SystemTime;
use url::Url;
use webkit6::prelude::*;
use webkit6::{NetworkSession, UserContentManager};

/// Manages browser tabs
pub struct TabManager {
    tabs_list: gtk4::ListBox,
    content_stack: gtk4::Stack,
    tabs: RefCell<HashMap<u32, Tab>>,
    active_tab_id: Cell<Option<u32>>,
    next_tab_id: Cell<u32>,
    /// URL entry to update when URL changes
    url_entry: gtk4::Entry,
    /// Progress bar for page load progress
    load_progress: gtk4::ProgressBar,
    /// Window reference for title updates
    window: glib::WeakRef<libadwaita::ApplicationWindow>,
    /// Back button to update sensitivity
    back_button: gtk4::Button,
    /// Forward button to update sensitivity
    forward_button: gtk4::Button,
    /// Application reference for history recording
    app: glib::WeakRef<QayeqApplication>,
    /// Network session for cookie/storage persistence
    network_session: NetworkSession,
    /// Content manager for ad blocking (optional)
    user_content_manager: Option<UserContentManager>,
    /// Callback for tab count changes
    on_tab_count_changed: RefCell<Option<Box<dyn Fn(usize)>>>,
    /// Callback for when webview gets focus (user clicked on web content)
    on_webview_focus: RefCell<Option<Box<dyn Fn()>>>,
    /// Callback for session changes (tab added/closed, navigation finished)
    on_session_changed: RefCell<Option<Box<dyn Fn()>>>,
}

/// Represents a single tab
struct Tab {
    id: u32,
    row: TabRow,
    webview: webkit6::WebView,
    /// The container/profile this tab belongs to
    profile_id: String,
    /// The tab that opened this tab (for returning when closed)
    parent_tab_id: Option<u32>,
}

impl TabManager {
    pub fn new(
        tabs_list: gtk4::ListBox,
        content_stack: gtk4::Stack,
        url_entry: gtk4::Entry,
        load_progress: gtk4::ProgressBar,
        _window: &libadwaita::ApplicationWindow,
        back_button: gtk4::Button,
        forward_button: gtk4::Button,
        app: &QayeqApplication,
        network_session: NetworkSession,
        user_content_manager: Option<UserContentManager>,
    ) -> Rc<Self> {
        let app_weak = glib::WeakRef::new();
        app_weak.set(Some(app));

        Rc::new(Self {
            tabs_list,
            content_stack,
            tabs: RefCell::new(HashMap::new()),
            active_tab_id: Cell::new(None),
            next_tab_id: Cell::new(1),
            url_entry,
            load_progress,
            window: glib::WeakRef::new(),
            back_button,
            forward_button,
            app: app_weak,
            network_session,
            user_content_manager,
            on_tab_count_changed: RefCell::new(None),
            on_webview_focus: RefCell::new(None),
            on_session_changed: RefCell::new(None),
        })
    }

    pub fn set_window(&self, window: &libadwaita::ApplicationWindow) {
        self.window.set(Some(window));
    }

    pub fn set_on_tab_count_changed<F: Fn(usize) + 'static>(&self, callback: F) {
        *self.on_tab_count_changed.borrow_mut() = Some(Box::new(callback));
    }

    pub fn set_on_webview_focus<F: Fn() + 'static>(&self, callback: F) {
        *self.on_webview_focus.borrow_mut() = Some(Box::new(callback));
    }

    pub fn set_on_session_changed<F: Fn() + 'static>(&self, callback: F) {
        *self.on_session_changed.borrow_mut() = Some(Box::new(callback));
    }

    fn notify_tab_count_changed(&self) {
        let count = self.tabs.borrow().len();
        if let Some(callback) = self.on_tab_count_changed.borrow().as_ref() {
            callback(count);
        }
    }

    fn notify_webview_focus(&self) {
        if let Some(callback) = self.on_webview_focus.borrow().as_ref() {
            callback();
        }
    }

    fn notify_session_changed(&self) {
        if let Some(callback) = self.on_session_changed.borrow().as_ref() {
            callback();
        }
    }

    /// Create a new tab, optionally in a specific container
    /// If container is None, uses the default container from ProfileManager
    /// If parent_tab_id is Some, closing this tab will return to the parent tab
    pub fn new_tab(self: &Rc<Self>, url: Option<&Url>, container: Option<&str>) -> u32 {
        self.new_tab_with_parent(url, container, None)
    }

    /// Create a new tab with a parent tab reference
    fn new_tab_with_parent(self: &Rc<Self>, url: Option<&Url>, container: Option<&str>, parent_tab_id: Option<u32>) -> u32 {
        let tab_id = self.next_tab_id.get();
        self.next_tab_id.set(tab_id + 1);

        // Determine which container/profile to use
        let (profile_id, network_session) = if let Some(app) = self.app.upgrade() {
            if let Some(pm) = app.profile_manager() {
                let container_name = container.unwrap_or_else(|| pm.default_container());
                if let Some(profile) = pm.get_profile(container_name) {
                    (container_name.to_string(), profile.network_session().clone())
                } else {
                    log::warn!("Container '{}' not loaded, using default network session", container_name);
                    (container_name.to_string(), self.network_session.clone())
                }
            } else {
                (crate::config::DEFAULT_CONTAINER.to_string(), self.network_session.clone())
            }
        } else {
            (crate::config::DEFAULT_CONTAINER.to_string(), self.network_session.clone())
        };

        // Create tab row for sidebar (TabRow extends ListBoxRow directly)
        let tab_row = TabRow::new(tab_id);
        tab_row.set_container(&profile_id);

        // Create WebKit webview with the container's network session
        let webview = webview::create_webview(
            &network_session,
            self.user_content_manager.as_ref(),
        );

        // Connect WebKit signals
        self.setup_webview_signals(tab_id, &webview, &tab_row, &profile_id);

        // Set up close button handler
        let manager = Rc::clone(self);
        tab_row.connect_close_clicked(move |row| {
            manager.close_tab(row.tab_id());
        });

        // Add to content stack first
        self.content_stack.add_named(&webview, Some(&tab_id.to_string()));

        // Store tab before appending to list (to avoid borrow issues when selection triggers)
        let tab = Tab {
            id: tab_id,
            row: tab_row.clone(),
            webview,
            profile_id: profile_id.clone(),
            parent_tab_id,
        };
        self.tabs.borrow_mut().insert(tab_id, tab);
        log::debug!("Created tab {} in container '{}' (parent: {:?})", tab_id, profile_id, parent_tab_id);

        // Now add to tabs list (this may trigger selection signal)
        self.tabs_list.append(&tab_row);

        // Select the new tab
        self.tabs_list.select_row(Some(&tab_row));
        self.switch_to_tab(tab_id);

        // Navigate to URL if provided
        if let Some(url) = url {
            self.navigate_to_in_tab(tab_id, url);
        }

        self.notify_tab_count_changed();
        self.notify_session_changed();
        tab_id
    }

    fn setup_webview_signals(self: &Rc<Self>, tab_id: u32, webview: &webkit6::WebView, tab_row: &TabRow, profile_id: &str) {
        // URL changed - use connect_notify_local for "uri" property (local = not Send+Sync)
        let url_entry = self.url_entry.clone();
        let content_stack = self.content_stack.clone();
        let tid_str = tab_id.to_string();
        webview.connect_notify_local(Some("uri"), move |wv, _| {
            if let Some(uri) = wv.uri() {
                log::info!("URI changed to: {}", uri);
            }

            let is_active = content_stack
                .visible_child_name()
                .map(|n| n.as_str() == tid_str)
                .unwrap_or(false);

            if is_active {
                if let Some(uri) = wv.uri() {
                    url_entry.set_text(&uri);
                }
            }
        });

        // Title changed
        let row = tab_row.clone();
        let window_ref = self.window.clone();
        let content_stack = self.content_stack.clone();
        let tid_str = tab_id.to_string();
        webview.connect_notify_local(Some("title"), move |wv, _| {
            let title = wv.title();
            row.set_title(title.as_deref().unwrap_or("New Tab"));

            let is_active = content_stack
                .visible_child_name()
                .map(|n| n.as_str() == tid_str)
                .unwrap_or(false);

            if is_active {
                if let Some(window) = window_ref.upgrade() {
                    let window_title = match title.as_deref() {
                        Some(t) if !t.is_empty() => format!("{} - {}", t, crate::config::APP_NAME),
                        _ => crate::config::APP_NAME.to_string(),
                    };
                    window.set_title(Some(&window_title));
                }
            }
        });

        // Load progress changed - use load-changed signal
        let progress_bar = self.load_progress.clone();
        let content_stack = self.content_stack.clone();
        let tid_str = tab_id.to_string();
        webview.connect_load_changed(move |wv, event| {
            use webkit6::LoadEvent;

            // Only update progress for the currently visible tab
            let is_visible = content_stack
                .visible_child_name()
                .map(|n| n.as_str() == tid_str)
                .unwrap_or(false);

            if !is_visible {
                return;
            }

            match event {
                LoadEvent::Started => {
                    progress_bar.set_fraction(0.1);
                    progress_bar.set_visible(true);
                }
                LoadEvent::Committed => {
                    let progress = wv.estimated_load_progress();
                    progress_bar.set_fraction(progress.max(0.3));
                }
                LoadEvent::Finished => {
                    progress_bar.set_visible(false);
                    progress_bar.set_fraction(0.0);
                }
                _ => {
                    let progress = wv.estimated_load_progress();
                    progress_bar.set_fraction(progress);
                }
            }
        });

        // Load changed (for loading indicator, nav buttons, history recording, and session save)
        let row = tab_row.clone();
        let back_btn = self.back_button.clone();
        let forward_btn = self.forward_button.clone();
        let content_stack = self.content_stack.clone();
        let app_ref = self.app.clone();
        let tid_str = tab_id.to_string();
        let tab_profile_id = profile_id.to_string();
        let manager_weak = Rc::downgrade(self);
        webview.connect_notify_local(Some("is-loading"), move |wv, _| {
            let is_loading = wv.is_loading();
            row.set_loading(is_loading);

            // Check if this tab is the visible one
            let is_active = content_stack
                .visible_child_name()
                .map(|n| n.as_str() == tid_str)
                .unwrap_or(false);

            if is_active {
                back_btn.set_sensitive(wv.can_go_back());
                forward_btn.set_sensitive(wv.can_go_forward());
            }

            // Record history and save session when navigation STARTS (not finishes)
            // This ensures back button works immediately and session is saved even if page load is interrupted
            if is_loading {
                if let Some(uri) = wv.uri() {
                    // Skip internal pages
                    if !uri.starts_with("about:") {
                        if let Ok(url) = Url::parse(&uri) {
                            // Record history immediately so back button works
                            if let Some(app) = app_ref.upgrade() {
                                if let Some(pm) = app.profile_manager() {
                                    if let Some(profile) = pm.get_profile(&tab_profile_id) {
                                        if let Err(e) = profile.history().record_visit(&url, None) {
                                            log::warn!("Failed to record history for profile '{}': {}", tab_profile_id, e);
                                        }
                                    }
                                }
                            }
                        }
                        // Notify session changed to trigger immediate save
                        if let Some(manager) = manager_weak.upgrade() {
                            manager.notify_session_changed();
                        }
                    }
                }
            }
        });

        // Favicon changed
        let row = tab_row.clone();
        webview.connect_notify_local(Some("favicon"), move |wv, _| {
            if let Some(favicon) = wv.favicon() {
                row.set_favicon(Some(&favicon));
            } else {
                row.set_favicon(None);
            }
        });

        // Webview focus - notify when user clicks on web content
        let focus_controller = gtk4::EventControllerFocus::new();
        let manager = Rc::downgrade(self);
        focus_controller.connect_enter(move |_| {
            if let Some(manager) = manager.upgrade() {
                manager.notify_webview_focus();
            }
        });
        webview.add_controller(focus_controller);


        // Context menu - detect "Save As" actions to show file dialog
        webview.connect_context_menu(move |_wv, context_menu, hit_test_result| {
            // Check for download-related context menu items
            use webkit6::ContextMenuAction;

            let n_items = context_menu.n_items();
            for i in 0..n_items {
                if let Some(item) = context_menu.item_at_position(i) {
                    let action = item.stock_action();
                    match action {
                        ContextMenuAction::DownloadImageToDisk => {
                            // User is about to save an image
                            if let Some(uri) = hit_test_result.image_uri() {
                                download::mark_save_as(&uri);
                            }
                        }
                        ContextMenuAction::DownloadLinkToDisk => {
                            // User is about to download a link
                            if let Some(uri) = hit_test_result.link_uri() {
                                download::mark_save_as(&uri);
                            }
                        }
                        ContextMenuAction::DownloadVideoToDisk | ContextMenuAction::DownloadAudioToDisk => {
                            // User is about to download media
                            if let Some(uri) = hit_test_result.media_uri() {
                                download::mark_save_as(&uri);
                            }
                        }
                        _ => {}
                    }
                }
            }
            false // Allow context menu to show
        });

        // Handle navigation policy decisions, including target="_blank" links
        let manager = Rc::downgrade(self);
        let parent_tab_id = tab_id; // Capture current tab ID for parent reference
        webview.connect_decide_policy(move |_wv, decision, decision_type| {
            use webkit6::PolicyDecisionType;

            match decision_type {
                PolicyDecisionType::NavigationAction => {
                    if let Some(nav_decision) = decision.downcast_ref::<webkit6::NavigationPolicyDecision>() {
                        if let Some(mut nav_action) = nav_decision.navigation_action() {
                            if let Some(request) = nav_action.request() {
                                let uri = request.uri().map(|u| u.to_string()).unwrap_or_default();
                                let nav_type = nav_action.navigation_type();
                                log::debug!(
                                    "Navigation policy: type={:?}, uri={}, mouse_button={}",
                                    nav_type,
                                    uri,
                                    nav_action.mouse_button()
                                );
                            }
                        }
                    }
                }
                PolicyDecisionType::Response => {
                    // Check if this is a downloadable response based on MIME type
                    if let Some(response_decision) = decision.downcast_ref::<webkit6::ResponsePolicyDecision>() {
                        if let Some(response) = response_decision.response() {
                            let mime_type = response.mime_type().map(|s| s.to_string()).unwrap_or_default();

                            // MIME types that should always trigger downloads
                            const DOWNLOAD_MIME_TYPES: &[&str] = &[
                                "application/octet-stream",
                                "application/x-iso9660-image",
                                "application/zip",
                                "application/x-tar",
                                "application/gzip",
                                "application/x-gzip",
                                "application/x-bzip2",
                                "application/x-xz",
                                "application/x-7z-compressed",
                                "application/x-rar-compressed",
                                "application/vnd.rar",
                                "application/x-executable",
                                "application/x-deb",
                                "application/x-rpm",
                                "application/x-redhat-package-manager",
                                "application/x-apple-diskimage",
                                "application/x-msdos-program",
                                "application/x-msdownload",
                            ];

                            if DOWNLOAD_MIME_TYPES.iter().any(|t| mime_type.starts_with(t)) {
                                log::info!("Detected downloadable response: {}", mime_type);
                                decision.download();
                                return true;
                            }

                            log::debug!("Response policy: mime_type={}", mime_type);
                        }
                    }
                }
                PolicyDecisionType::NewWindowAction => {
                    // Handle target="_blank" and window.open() - open in new tab with parent reference
                    log::info!("New window policy decision - opening in new tab from tab {}", parent_tab_id);
                    if let Some(nav_decision) = decision.downcast_ref::<webkit6::NavigationPolicyDecision>() {
                        if let Some(mut nav_action) = nav_decision.navigation_action() {
                            if let Some(mut request) = nav_action.request() {
                                if let Some(uri) = request.uri() {
                                    log::info!("Opening in new tab: {}", uri);
                                    if let Some(manager) = manager.upgrade() {
                                        if let Ok(url) = Url::parse(&uri) {
                                            manager.new_tab_with_parent(Some(&url), None, Some(parent_tab_id));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // Ignore the decision (we handled it ourselves)
                    decision.ignore();
                    return true; // We handled it
                }
                _ => {}
            }

            false // Use default policy (allow)
        });
    }

    /// Close a tab and return its data for recently closed tracking.
    pub fn close_tab(&self, tab_id: u32) -> Option<ClosedTab> {
        // Extract tab data and determine if we need to switch tabs
        let (closed_tab, need_switch, parent_tab_id) = {
            let mut tabs = self.tabs.borrow_mut();
            if let Some(tab) = tabs.remove(&tab_id) {
                // Capture tab state before removing
                let title = tab.row.title();
                let closed = ClosedTab {
                    url: tab.webview.uri().map(|s| s.to_string()).unwrap_or_default(),
                    title: if title.is_empty() { None } else { Some(title) },
                    container: tab.profile_id.clone(),
                    closed_at: SystemTime::now(),
                };

                self.tabs_list.remove(&tab.row);
                self.content_stack.remove(&tab.webview);

                // Check if we need to switch tabs after releasing the borrow
                let need_switch = self.active_tab_id.get() == Some(tab_id);
                (Some(closed), need_switch, tab.parent_tab_id)
            } else {
                (None, false, None)
            }
        };
        // RefCell borrow is released here

        // Now we can safely trigger selection which may call switch_to_tab
        if need_switch {
            // Try to switch to parent tab first, then fall back to first tab
            let target_row = if let Some(parent_id) = parent_tab_id {
                // Check if parent tab still exists
                self.tabs.borrow().get(&parent_id).map(|t| t.row.clone())
            } else {
                None
            }.or_else(|| {
                // Fall back to first tab
                self.tabs_list.row_at_index(0).and_then(|row| row.downcast::<TabRow>().ok())
            });

            if let Some(row) = target_row {
                self.tabs_list.select_row(Some(&row));
            } else {
                self.active_tab_id.set(None);
            }
        }

        self.notify_tab_count_changed();
        self.notify_session_changed();
        closed_tab
    }

    /// Close the current tab and return its data for recently closed tracking.
    pub fn close_current_tab(&self) -> Option<ClosedTab> {
        if let Some(tab_id) = self.active_tab_id.get() {
            return self.close_tab(tab_id);
        }
        None
    }

    pub fn switch_to_tab(&self, tab_id: u32) {
        if self.tabs.borrow().contains_key(&tab_id) {
            // Remove active class from all tabs, add to new active
            for tab in self.tabs.borrow().values() {
                tab.row.set_active(tab.id == tab_id);
            }

            self.active_tab_id.set(Some(tab_id));
            self.content_stack.set_visible_child_name(&tab_id.to_string());

            // Select the row in the ListBox
            if let Some(tab) = self.tabs.borrow().get(&tab_id) {
                self.tabs_list.select_row(Some(&tab.row));
            }

            // Update URL bar, title, and progress for the new active tab
            if let Some(tab) = self.tabs.borrow().get(&tab_id) {
                if let Some(uri) = tab.webview.uri() {
                    self.url_entry.set_text(&uri);
                } else {
                    self.url_entry.set_text("");
                }

                if let Some(window) = self.window.upgrade() {
                    let title = tab.webview.title();
                    let window_title = match title.as_deref() {
                        Some(t) if !t.is_empty() => format!("{} - {}", t, crate::config::APP_NAME),
                        _ => crate::config::APP_NAME.to_string(),
                    };
                    window.set_title(Some(&window_title));
                }

                self.back_button.set_sensitive(tab.webview.can_go_back());
                self.forward_button.set_sensitive(tab.webview.can_go_forward());

                // Update progress bar for the switched tab
                let is_loading = tab.webview.is_loading();
                if is_loading {
                    self.load_progress.set_fraction(tab.webview.estimated_load_progress());
                    self.load_progress.set_visible(true);
                } else {
                    self.load_progress.set_visible(false);
                }
            }
        }
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.borrow().len()
    }

    pub fn navigate_to(&self, url: &Url) {
        if let Some(tab_id) = self.active_tab_id.get() {
            self.navigate_to_in_tab(tab_id, url);
        }
    }

    fn navigate_to_in_tab(&self, tab_id: u32, url: &Url) {
        if let Some(tab) = self.tabs.borrow().get(&tab_id) {
            tab.webview.load_url(url);
        }
    }

    pub fn reload_current(&self) {
        if let Some(tab_id) = self.active_tab_id.get() {
            if let Some(tab) = self.tabs.borrow().get(&tab_id) {
                tab.webview.reload();
            }
        }
    }

    pub fn go_back(&self) {
        if let Some(tab_id) = self.active_tab_id.get() {
            if let Some(tab) = self.tabs.borrow().get(&tab_id) {
                tab.webview.go_back();
            }
        }
    }

    pub fn go_forward(&self) {
        if let Some(tab_id) = self.active_tab_id.get() {
            if let Some(tab) = self.tabs.borrow().get(&tab_id) {
                tab.webview.go_forward();
            }
        }
    }

    pub fn can_go_back(&self) -> bool {
        if let Some(tab_id) = self.active_tab_id.get() {
            if let Some(tab) = self.tabs.borrow().get(&tab_id) {
                return tab.webview.can_go_back();
            }
        }
        false
    }

    pub fn can_go_forward(&self) -> bool {
        if let Some(tab_id) = self.active_tab_id.get() {
            if let Some(tab) = self.tabs.borrow().get(&tab_id) {
                return tab.webview.can_go_forward();
            }
        }
        false
    }

    pub fn current_url(&self) -> Option<Url> {
        if let Some(tab_id) = self.active_tab_id.get() {
            if let Some(tab) = self.tabs.borrow().get(&tab_id) {
                return tab.webview.current_url();
            }
        }
        None
    }

    pub fn current_title(&self) -> Option<String> {
        if let Some(tab_id) = self.active_tab_id.get() {
            if let Some(tab) = self.tabs.borrow().get(&tab_id) {
                return tab.webview.title().map(|s| s.to_string());
            }
        }
        None
    }

    /// Get all tabs with their URLs and titles for session saving
    /// Returns (url, title) pairs in tab order, plus the active tab index
    pub fn get_all_tabs(&self) -> (Vec<(String, Option<String>)>, usize) {
        let tabs = self.tabs.borrow();
        let active_id = self.active_tab_id.get();

        // Collect tabs with their IDs for sorting
        let mut tab_data: Vec<(u32, String, Option<String>)> = tabs
            .iter()
            .filter_map(|(&id, tab)| {
                tab.webview.uri().map(|uri| {
                    let title = tab.webview.title().map(|t| t.to_string());
                    (id, uri.to_string(), title)
                })
            })
            .collect();

        // Sort by tab ID to maintain order
        tab_data.sort_by_key(|(id, _, _)| *id);

        // Find active tab index
        let active_index = tab_data
            .iter()
            .position(|(id, _, _)| Some(*id) == active_id)
            .unwrap_or(0);

        let tabs_vec: Vec<(String, Option<String>)> = tab_data
            .into_iter()
            .map(|(_, url, title)| (url, title))
            .collect();

        (tabs_vec, active_index)
    }

    /// Get all tabs with their container info (for recording closed windows).
    /// Returns (url, title, container) tuples and active tab index.
    pub fn get_all_tabs_with_containers(&self) -> (Vec<(String, Option<String>, String)>, usize) {
        let tabs = self.tabs.borrow();
        let active_id = self.active_tab_id.get();

        // Collect tabs with their IDs for sorting
        let mut tab_data: Vec<(u32, String, Option<String>, String)> = tabs
            .iter()
            .filter_map(|(&id, tab)| {
                tab.webview.uri().map(|uri| {
                    let title = tab.webview.title().map(|t| t.to_string());
                    (id, uri.to_string(), title, tab.profile_id.clone())
                })
            })
            .collect();

        // Sort by tab ID to maintain order
        tab_data.sort_by_key(|(id, _, _, _)| *id);

        // Find active tab index
        let active_index = tab_data
            .iter()
            .position(|(id, _, _, _)| Some(*id) == active_id)
            .unwrap_or(0);

        let tabs_vec: Vec<(String, Option<String>, String)> = tab_data
            .into_iter()
            .map(|(_, url, title, container)| (url, title, container))
            .collect();

        (tabs_vec, active_index)
    }
}
