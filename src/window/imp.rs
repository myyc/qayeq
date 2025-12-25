use crate::application::QayeqApplication;
use crate::completion::history_provider::get_history_completions;
use crate::completion::provider::CompletionItem;
use crate::completion::row::CompletionRow;
use crate::completion::search_provider::fetch_search_suggestions;
use crate::config;
use crate::download::{self, DownloadRow};
use crate::recently_closed::{ClosedTab, ClosedWindow, RecentlyClosedTabs};
use crate::session::{SavedWindow, Session};
use crate::tab::{TabManager, TabRow};
use crate::url_bar;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{gio, glib, CompositeTemplate, TemplateChild};
use libadwaita::prelude::{AdwDialogExt, AlertDialogExt};
use libadwaita::subclass::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;
use url::Url;

#[derive(Default, CompositeTemplate)]
#[template(file = "../../resources/ui/window.ui")]
pub struct QayeqWindow {
    #[template_child]
    pub toolbar: TemplateChild<gtk4::Box>,

    #[template_child]
    pub url_entry: TemplateChild<gtk4::Entry>,

    #[template_child]
    pub load_progress: TemplateChild<gtk4::ProgressBar>,

    #[template_child]
    pub back_button: TemplateChild<gtk4::Button>,

    #[template_child]
    pub forward_button: TemplateChild<gtk4::Button>,

    #[template_child]
    pub refresh_button: TemplateChild<gtk4::Button>,

    #[template_child]
    pub tab_sidebar: TemplateChild<gtk4::Box>,

    #[template_child]
    pub tabs_list: TemplateChild<gtk4::ListBox>,

    #[template_child]
    pub new_tab_button: TemplateChild<gtk4::Button>,

    #[template_child]
    pub content_stack: TemplateChild<gtk4::Stack>,

    #[template_child]
    pub close_button: TemplateChild<gtk4::Button>,

    #[template_child]
    pub menu_button: TemplateChild<gtk4::MenuButton>,

    #[template_child]
    pub window_controls: TemplateChild<gtk4::Box>,

    #[template_child]
    pub download_button: TemplateChild<gtk4::MenuButton>,

    #[template_child]
    pub download_progress: TemplateChild<gtk4::ProgressBar>,

    #[template_child]
    pub download_list: TemplateChild<gtk4::ListBox>,

    #[template_child]
    pub download_empty_label: TemplateChild<gtk4::Label>,

    #[template_child]
    pub toast_overlay: TemplateChild<libadwaita::ToastOverlay>,

    #[template_child]
    pub main_overlay: TemplateChild<gtk4::Overlay>,

    #[template_child]
    pub completion_scroll: TemplateChild<gtk4::ScrolledWindow>,

    #[template_child]
    pub completion_list: TemplateChild<gtk4::ListBox>,

    pub tab_manager: RefCell<Option<Rc<TabManager>>>,

    // Hide timeout source IDs (Rc for sharing in closures)
    pub toolbar_hide_source: Rc<RefCell<Option<glib::SourceId>>>,
    pub sidebar_hide_source: Rc<RefCell<Option<glib::SourceId>>>,

    // Completion debounce and cancellation
    pub completion_debounce: Rc<RefCell<Option<glib::SourceId>>>,
    pub completion_cancellable: RefCell<Option<gio::Cancellable>>,

    // Flag to track if initial startup is complete
    pub initialized: Rc<Cell<bool>>,

    // Flag to track if menu is open (keep toolbar visible)
    pub menu_open: Rc<Cell<bool>>,

    // Flag to track if toolbar is pinned (via Ctrl+L, stays until click on webview)
    pub toolbar_pinned: Rc<Cell<bool>>,

    // Flag to track if sidebar is pinned (via Ctrl+T, stays until click on webview)
    pub sidebar_pinned: Rc<Cell<bool>>,

    // Inline autocomplete state
    pub autocomplete_url: RefCell<Option<String>>,   // The full autocompleted URL
    pub user_input_len: Cell<usize>,                 // Length of user's actual input
    pub explicit_selection: Cell<bool>,              // True if user used arrow keys to select
    pub setting_autocomplete: Cell<bool>,            // True while programmatically setting text

    // Double-tap Shift detection state
    pub last_shift_release: RefCell<Option<std::time::Instant>>,  // When Shift was last released
    pub shift_clean: Cell<bool>,                     // True if current Shift press has no other keys

    // Recently closed tabs tracking
    pub recently_closed_tabs: RefCell<RecentlyClosedTabs>,

    // Flag to skip session restore (for empty new windows)
    pub skip_session_restore: Cell<bool>,

    // Pending session data to restore (for new_with_session)
    pub pending_session: RefCell<Option<SavedWindow>>,
}

#[glib::object_subclass]
impl ObjectSubclass for QayeqWindow {
    const NAME: &'static str = "QayeqWindow";
    type Type = super::QayeqWindow;
    type ParentType = libadwaita::ApplicationWindow;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for QayeqWindow {
    fn constructed(&self) {
        self.parent_constructed();

        let obj = self.obj();

        // Set up window properties
        obj.set_title(Some(config::APP_NAME));
        obj.set_default_size(1200, 800);

        // Overlays start hidden via CSS class in window.ui

        // Set up window actions
        let action_show_shortcuts = gio::ActionEntry::builder("show-shortcuts")
            .activate(|window: &super::QayeqWindow, _, _| {
                window.imp().show_shortcuts_window();
            })
            .build();
        obj.add_action_entries([action_show_shortcuts]);

        // Defer initialization until application is available
        let obj_weak = obj.downgrade();
        glib::idle_add_local_once(move || {
            if let Some(obj) = obj_weak.upgrade() {
                obj.imp().initialize_tab_manager();
            }
        });

        // Window-level key handler for Escape (toggle toolbar), F1 (toggle sidebar), and double-tap Shift
        let window_key_controller = gtk4::EventControllerKey::new();
        window_key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let obj_weak = obj.downgrade();
        window_key_controller.connect_key_pressed(move |_, key, _, modifier| {
            if let Some(obj) = obj_weak.upgrade() {
                let imp = obj.imp();
                match key {
                    gtk4::gdk::Key::Escape => {
                        // Cancel any pending completion requests
                        if let Some(source_id) = imp.completion_debounce.borrow_mut().take() {
                            source_id.remove();
                        }
                        if let Some(cancellable) = imp.completion_cancellable.borrow_mut().take() {
                            cancellable.cancel();
                        }
                        // Hide completion list if visible
                        if imp.completion_scroll.is_visible() {
                            imp.completion_scroll.set_visible(false);
                        }
                        // Reset URL bar to current page URL before toggling
                        if let Some(tm) = imp.tab_manager.borrow().as_ref() {
                            if let Some(url) = tm.current_url() {
                                imp.url_entry.set_text(url.as_str());
                            }
                        }
                        // Toggle toolbar
                        imp.toggle_toolbar();
                        return glib::Propagation::Stop;
                    }
                    gtk4::gdk::Key::F1 => {
                        // Toggle sidebar
                        imp.toggle_sidebar();
                        return glib::Propagation::Stop;
                    }
                    // Track Shift press - start clean if no other modifiers held
                    gtk4::gdk::Key::Shift_L | gtk4::gdk::Key::Shift_R => {
                        // Only start tracking if no other modifiers are held
                        let other_mods = modifier.difference(gtk4::gdk::ModifierType::SHIFT_MASK);
                        if other_mods.is_empty() {
                            imp.shift_clean.set(true);
                        }
                    }
                    // Any other key invalidates the clean Shift press
                    _ => {
                        imp.shift_clean.set(false);
                    }
                }
            }
            glib::Propagation::Proceed
        });

        // Key-released handler for double-tap Shift detection
        let obj_weak = obj.downgrade();
        window_key_controller.connect_key_released(move |_, key, _, _| {
            if let Some(obj) = obj_weak.upgrade() {
                let imp = obj.imp();
                if matches!(key, gtk4::gdk::Key::Shift_L | gtk4::gdk::Key::Shift_R) && imp.shift_clean.get() {
                    let now = std::time::Instant::now();
                    let double_tap_threshold = Duration::from_millis(350);

                    // Copy out the last release time to avoid borrow conflict
                    let last_release = *imp.last_shift_release.borrow();

                    if let Some(last) = last_release {
                        if now.duration_since(last) < double_tap_threshold {
                            // Double-tap detected! Toggle sidebar
                            imp.toggle_sidebar();
                            // Clear state to prevent triple-tap triggering again
                            *imp.last_shift_release.borrow_mut() = None;
                            return;
                        }
                    }

                    // Record this release time for potential double-tap
                    *imp.last_shift_release.borrow_mut() = Some(now);
                }
            }
        });

        obj.add_controller(window_key_controller);

        // URL entry key handler for arrow navigation in completions
        let entry_key_controller = gtk4::EventControllerKey::new();
        entry_key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let obj_weak = obj.downgrade();
        entry_key_controller.connect_key_pressed(move |_, key, _, _| {
            if let Some(obj) = obj_weak.upgrade() {
                let imp = obj.imp();
                match key {
                    gtk4::gdk::Key::Down => {
                        if imp.completion_scroll.is_visible() {
                            imp.explicit_selection.set(true);
                            imp.move_completion_selection(1);
                        } else {
                            imp.trigger_completions();
                        }
                        return glib::Propagation::Stop;
                    }
                    gtk4::gdk::Key::Up => {
                        if imp.completion_scroll.is_visible() {
                            imp.explicit_selection.set(true);
                            imp.move_completion_selection(-1);
                            return glib::Propagation::Stop;
                        }
                    }
                    _ => {}
                }
            }
            glib::Propagation::Proceed
        });
        self.url_entry.add_controller(entry_key_controller);

        // Connect close button
        let obj_weak = obj.downgrade();
        self.close_button.connect_clicked(move |_| {
            if let Some(obj) = obj_weak.upgrade() {
                obj.close();
            }
        });

        // Track menu open state to keep toolbar visible
        let menu_open = Rc::clone(&self.menu_open);
        let toolbar = self.toolbar.clone();
        let window_controls = self.window_controls.clone();
        let toolbar_hide_source = Rc::clone(&self.toolbar_hide_source);
        self.menu_button.connect_active_notify(move |btn| {
            let is_active = btn.is_active();
            menu_open.set(is_active);
            if is_active {
                // Cancel any pending hide and show both
                if let Some(source_id) = toolbar_hide_source.borrow_mut().take() {
                    let _ = source_id.remove();
                }
                toolbar.remove_css_class("hidden");
                toolbar.set_can_target(true);
                window_controls.remove_css_class("hidden");
                window_controls.set_can_target(true);
            }
        });

        // Subscribe to download changes to update UI
        let download_button = self.download_button.clone();
        let download_progress = self.download_progress.clone();
        let download_list = self.download_list.clone();
        let download_empty_label = self.download_empty_label.clone();
        let toast_overlay = self.toast_overlay.clone();
        let last_download_count: Rc<Cell<usize>> = Rc::new(Cell::new(0));

        download::subscribe_to_changes(move || {
            let downloads = download::recent_downloads(10);
            let current_count = downloads.len();
            let prev_count = last_download_count.get();

            // Show toast for new downloads
            if current_count > prev_count {
                // Find the newest download (first in the list since it's reversed)
                if let Some(newest) = downloads.first() {
                    if newest.is_active() {
                        let toast = libadwaita::Toast::new(&format!("Downloading {}", newest.filename));
                        toast.set_timeout(3);
                        toast_overlay.add_toast(toast);
                    }
                }
            }
            last_download_count.set(current_count);

            // Update button visibility
            let has_downloads = download::has_downloads();

            download_button.set_visible(has_downloads);

            // Update progress bar - show aggregate progress of active or paused downloads
            let in_progress_downloads: Vec<_> = downloads.iter()
                .filter(|d| d.is_active() || d.is_paused())
                .collect();

            if !in_progress_downloads.is_empty() {
                let total_bytes: u64 = in_progress_downloads.iter().map(|d| d.total_bytes).sum();
                let received_bytes: u64 = in_progress_downloads.iter().map(|d| d.received_bytes).sum();

                let fraction = if total_bytes > 0 {
                    received_bytes as f64 / total_bytes as f64
                } else {
                    0.0
                };

                download_progress.set_visible(true);
                download_progress.set_fraction(fraction);
            } else {
                download_progress.set_visible(false);
            }

            // Update download list by reusing existing rows to prevent flickering
            download_empty_label.set_visible(downloads.is_empty());

            // Collect current download IDs
            let download_ids: std::collections::HashSet<u64> = downloads.iter().map(|d| d.id).collect();

            // First pass: remove rows for downloads that no longer exist
            let mut rows_to_remove = Vec::new();
            let mut child = download_list.first_child();
            while let Some(widget) = child {
                let next = widget.next_sibling();
                if let Some(row) = widget.downcast_ref::<gtk4::ListBoxRow>() {
                    if let Some(download_row) = row.child().and_then(|c| c.downcast::<DownloadRow>().ok()) {
                        if !download_ids.contains(&download_row.download_id()) {
                            rows_to_remove.push(row.clone());
                        }
                    }
                }
                child = next;
            }
            for row in rows_to_remove {
                download_list.remove(&row);
            }

            // Second pass: update existing rows or create new ones
            for item in &downloads {
                // Look for existing row with this ID
                let mut found = false;
                let mut child = download_list.first_child();
                while let Some(widget) = child {
                    if let Some(row) = widget.downcast_ref::<gtk4::ListBoxRow>() {
                        if let Some(download_row) = row.child().and_then(|c| c.downcast::<DownloadRow>().ok()) {
                            if download_row.download_id() == item.id {
                                download_row.update(item);
                                found = true;
                                break;
                            }
                        }
                    }
                    child = widget.next_sibling();
                }

                // Create new row if not found
                if !found {
                    let row = DownloadRow::new();
                    row.update(item);
                    row.connect_action_clicked(|id, is_cancel| {
                        if is_cancel {
                            download::cancel_download(id);
                        } else {
                            // Open folder containing the download
                            if let Some(item) = download::recent_downloads(100)
                                .iter()
                                .find(|d| d.id == id)
                            {
                                let path = item.destination.clone();
                                let file = gtk4::gio::File::for_path(&path);
                                let launcher = gtk4::FileLauncher::new(Some(&file));
                                launcher.open_containing_folder(
                                    gtk4::Window::NONE,
                                    gtk4::gio::Cancellable::NONE,
                                    move |result| {
                                        if let Err(e) = result {
                                            log::warn!("Portal open_containing_folder failed: {}, trying xdg-open", e);
                                            // Fallback to xdg-open
                                            if let Some(parent) = path.parent() {
                                                if let Err(e) = std::process::Command::new("xdg-open")
                                                    .arg(parent)
                                                    .spawn()
                                                {
                                                    log::error!("Failed to open folder with xdg-open: {}", e);
                                                }
                                            }
                                        }
                                    },
                                );
                            }
                        }
                    });
                    row.connect_dismiss_clicked(|id| {
                        download::remove_download(id);
                    });
                    row.connect_pause_clicked(|id, is_paused| {
                        if is_paused {
                            download::resume_download(id);
                        } else {
                            download::pause_download(id);
                        }
                    });
                    download_list.append(&row);
                }
            }
        });

    }
}

impl QayeqWindow {
    fn show_toolbar(&self) {
        // Cancel any pending hide
        if let Some(source_id) = self.toolbar_hide_source.borrow_mut().take() {
            let _ = source_id.remove();
        }
        // Show both toolbar and window controls together
        self.toolbar.remove_css_class("hidden");
        self.toolbar.set_can_target(true);
        self.window_controls.remove_css_class("hidden");
        self.window_controls.set_can_target(true);
    }

    fn hide_toolbar(&self) {
        // Cancel any pending hide
        if let Some(source_id) = self.toolbar_hide_source.borrow_mut().take() {
            let _ = source_id.remove();
        }
        self.toolbar.add_css_class("hidden");
        self.toolbar.set_can_target(false);
        self.window_controls.add_css_class("hidden");
        self.window_controls.set_can_target(false);
    }

    fn toggle_toolbar(&self) {
        if self.toolbar.has_css_class("hidden") {
            self.show_toolbar();
            self.focus_url_bar();
        } else {
            self.hide_toolbar();
        }
    }

    pub fn toggle_sidebar(&self) {
        if let Some(source_id) = self.sidebar_hide_source.borrow_mut().take() {
            source_id.remove();
        }
        if self.tab_sidebar.has_css_class("hidden") {
            self.tab_sidebar.remove_css_class("hidden");
            self.tab_sidebar.set_can_target(true);
        } else {
            self.tab_sidebar.add_css_class("hidden");
            self.tab_sidebar.set_can_target(false);
        }
    }

    pub fn show_sidebar(&self) {
        if let Some(source_id) = self.sidebar_hide_source.borrow_mut().take() {
            source_id.remove();
        }
        self.tab_sidebar.remove_css_class("hidden");
        self.tab_sidebar.set_can_target(true);
    }

    /// Show the sidebar (public for keyboard shortcut)
    pub fn show_sidebar_action(&self) {
        self.show_sidebar();
    }

    /// Show the keyboard shortcuts window
    fn show_shortcuts_window(&self) {
        let builder = gtk4::Builder::from_resource("/dev/myyc/qayeq/ui/shortcuts.ui");
        let shortcuts_window: gtk4::ShortcutsWindow = builder
            .object("shortcuts_window")
            .expect("Could not get shortcuts_window from builder");

        shortcuts_window.set_transient_for(Some(&*self.obj()));
        shortcuts_window.present();
    }

    /// Initialize tab manager - called after application is available
    fn initialize_tab_manager(&self) {
        // Skip if already initialized
        if self.tab_manager.borrow().is_some() {
            return;
        }

        let obj = self.obj();

        // Get the application for history recording
        let app = obj
            .application()
            .and_then(|a| a.downcast::<QayeqApplication>().ok());

        let Some(app) = app else {
            log::error!("No application available for TabManager");
            return;
        };

        // Get NetworkSession from the current profile for cookie persistence
        let network_session = app
            .profile_manager()
            .and_then(|pm| pm.current_profile())
            .map(|p| p.network_session().clone())
            .unwrap_or_else(|| {
                log::warn!("No profile available, using ephemeral session");
                webkit6::NetworkSession::new_ephemeral()
            });

        // Get UserContentManager from content filter manager (if available)
        let user_content_manager = app
            .content_filter_manager()
            .map(|cfm| cfm.user_content_manager().clone());

        // Initialize tab manager with all needed widgets
        let tab_manager = TabManager::new(
            self.tabs_list.get(),
            self.content_stack.get(),
            self.url_entry.get(),
            self.load_progress.get(),
            obj.upcast_ref::<libadwaita::ApplicationWindow>(),
            self.back_button.get(),
            self.forward_button.get(),
            &app,
            network_session,
            user_content_manager,
        );

        // Set window reference
        tab_manager.set_window(obj.upcast_ref::<libadwaita::ApplicationWindow>());

        // Set up tab count change callback for sidebar visibility
        let tab_sidebar = self.tab_sidebar.clone();
        let sidebar_hide_source = Rc::clone(&self.sidebar_hide_source);
        let initialized = Rc::clone(&self.initialized);
        tab_manager.set_on_tab_count_changed(move |_count| {
            // Skip during startup to avoid flash
            if !initialized.get() {
                return;
            }
            // Show sidebar briefly when tabs change
            if let Some(source_id) = sidebar_hide_source.borrow_mut().take() {
                source_id.remove();
            }
            tab_sidebar.remove_css_class("hidden");
            tab_sidebar.set_can_target(true);

            // Schedule auto-hide after 2 seconds
            let sidebar_for_hide = tab_sidebar.clone();
            let hide_source = sidebar_hide_source.clone();
            let source_id = glib::timeout_add_local_once(Duration::from_secs(2), move || {
                sidebar_for_hide.add_css_class("hidden");
                sidebar_for_hide.set_can_target(false);
                hide_source.borrow_mut().take();
            });
            *sidebar_hide_source.borrow_mut() = Some(source_id);
        });

        // Set up webview focus callback - hide UI elements when user clicks on web content
        let obj_weak = obj.downgrade();
        tab_manager.set_on_webview_focus(move || {
            if let Some(obj) = obj_weak.upgrade() {
                let imp = obj.imp();
                imp.completion_scroll.set_visible(false);
                // Hide sidebar if it was pinned
                if imp.sidebar_pinned.get() {
                    imp.sidebar_pinned.set(false);
                    imp.tab_sidebar.add_css_class("hidden");
                    imp.tab_sidebar.set_can_target(false);
                }
                // Hide toolbar if it was pinned
                if imp.toolbar_pinned.get() {
                    imp.toolbar_pinned.set(false);
                    imp.hide_toolbar();
                }
            }
        });

        // Set up session changed callback - save session immediately on changes
        let app_weak = app.downgrade();
        tab_manager.set_on_session_changed(move || {
            if let Some(app) = app_weak.upgrade() {
                app.save_all_windows();
            }
        });

        // Store tab manager before connecting signals
        *self.tab_manager.borrow_mut() = Some(Rc::clone(&tab_manager));

        // Initialize completion list
        self.setup_completion_list(&app, &tab_manager);

        // Connect URL entry activate signal
        {
            let tm = Rc::clone(&tab_manager);
            let completion_scroll = self.completion_scroll.clone();
            let completion_list = self.completion_list.clone();
            let completion_debounce = Rc::clone(&self.completion_debounce);
            let obj_weak = obj.downgrade();
            let settings = gio::Settings::new(config::APP_ID);
            let search_engine = settings.string("search-engine");
            let search_template: Rc<str> = config::SEARCH_ENGINES
                .iter()
                .find(|(id, _, _)| *id == search_engine.as_str())
                .map(|(_, _, url)| *url)
                .unwrap_or("https://duckduckgo.com/?q={}")
                .into();
            let tm_for_reset = Rc::clone(&tm);
            self.url_entry.connect_activate(move |entry| {
                // Cancel any pending completion requests
                if let Some(source_id) = completion_debounce.borrow_mut().take() {
                    source_id.remove();
                }
                if let Some(obj) = obj_weak.upgrade() {
                    if let Some(cancellable) = obj.imp().completion_cancellable.borrow_mut().take() {
                        cancellable.cancel();
                    }
                }

                // Hide completion list
                completion_scroll.set_visible(false);

                // If empty, just restore URL without navigating
                if entry.text().is_empty() {
                    if let Some(url) = tm_for_reset.current_url() {
                        entry.set_text(url.as_str());
                        entry.select_region(0, -1);
                    }
                    return;
                }

                // Check if there's an explicitly selected completion item (user navigated with arrows)
                if let Some(obj) = obj_weak.upgrade() {
                    if obj.imp().explicit_selection.get() {
                        if let Some(selected_row) = completion_list.selected_row() {
                            if let Some(child) = selected_row.child() {
                                if let Ok(completion_row) = child.downcast::<CompletionRow>() {
                                    if let Some(item) = completion_row.item() {
                                        let url = item.action_url(&search_template);
                                        if let Ok(parsed_url) = Url::parse(&url) {
                                            tm.navigate_to(&parsed_url);
                                            entry.set_text(&url);
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // No selection - use text as-is
                let text = entry.text();
                if text.is_empty() {
                    return;
                }

                match url_bar::parse_input(&text) {
                    url_bar::UrlBarInput::Url(url) => {
                        tm.navigate_to(&url);
                    }
                    url_bar::UrlBarInput::Search(query) => {
                        if let Ok(url) = url_bar::build_search_url(&query) {
                            tm.navigate_to(&url);
                        }
                    }
                }
            });
        }

        // Connect back button
        {
            let tm = Rc::clone(&tab_manager);
            self.back_button.connect_clicked(move |_| {
                tm.go_back();
            });
        }

        // Connect forward button
        {
            let tm = Rc::clone(&tab_manager);
            self.forward_button.connect_clicked(move |_| {
                tm.go_forward();
            });
        }

        // Connect refresh button
        {
            let tm = Rc::clone(&tab_manager);
            self.refresh_button.connect_clicked(move |_| {
                tm.reload_current();
            });
        }

        // Connect new tab button
        {
            let obj_weak = obj.downgrade();
            self.new_tab_button.connect_clicked(move |_| {
                if let Some(obj) = obj_weak.upgrade() {
                    obj.imp().new_tab(None, None);
                }
            });
        }

        // Connect row-selected signal for tab switching
        {
            let tm = Rc::clone(&tab_manager);
            self.tabs_list.connect_row_selected(move |_list_box, row| {
                if let Some(row) = row {
                    // TabRow extends ListBoxRow directly, so downcast the row itself
                    if let Ok(tab_row) = row.clone().downcast::<TabRow>() {
                        tm.switch_to_tab(tab_row.tab_id());
                    }
                }
            });
        }

        // Check for pending session from new_with_session
        let pending = self.pending_session.borrow_mut().take();
        if let Some(saved_window) = pending {
            self.restore_from_saved_window(&tab_manager, saved_window);
        } else if self.skip_session_restore.get() || !self.restore_session() {
            // Try to restore session (unless skip_session_restore is set), otherwise create initial tab
            tab_manager.new_tab(None, None);
        }
        self.update_sidebar_visibility();
        self.focus_url_bar();

        // Mark initialization complete - sidebar will now show on tab changes
        self.initialized.set(true);
    }


    pub fn new_tab(&self, url: Option<&Url>, container: Option<&str>) {
        if let Some(tab_manager) = self.tab_manager.borrow().as_ref() {
            tab_manager.new_tab(url, container);
        }
        self.update_sidebar_visibility();
        // Pin sidebar so it stays visible until user clicks on webview
        self.sidebar_pinned.set(true);
        self.show_sidebar();
        // Show toolbar and focus URL bar for new tab
        self.focus_url_bar();
    }

    pub fn close_current_tab(&self) {
        let tab_count = self.tab_manager.borrow().as_ref().map(|tm| tm.tab_count()).unwrap_or(0);

        if tab_count <= 1 {
            // Last tab - show confirmation to close window
            self.show_close_window_dialog();
        } else {
            if let Some(tab_manager) = self.tab_manager.borrow().as_ref() {
                if let Some(closed_tab) = tab_manager.close_current_tab() {
                    // Record for recently closed
                    self.recently_closed_tabs.borrow_mut().push(closed_tab);
                }
            }
            self.update_sidebar_visibility();
        }
    }

    /// Reopen the most recently closed tab.
    pub fn reopen_last_closed_tab(&self) {
        if let Some(closed_tab) = self.recently_closed_tabs.borrow_mut().pop() {
            if let Ok(url) = Url::parse(&closed_tab.url) {
                self.new_tab(Some(&url), Some(&closed_tab.container));
            } else {
                // URL couldn't be parsed, just open a blank tab
                self.new_tab(None, Some(&closed_tab.container));
            }
        }
    }

    fn show_close_window_dialog(&self) {
        let dialog = libadwaita::AlertDialog::new(
            Some("Close Window?"),
            Some("This will close the last tab and the window."),
        );
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("close", "Close Window");
        dialog.set_response_appearance("close", libadwaita::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");

        let window = self.obj().clone();
        dialog.connect_response(None, move |_, response| {
            if response == "close" {
                window.close();
            }
        });

        dialog.present(Some(&*self.obj()));
    }

    pub fn focus_url_bar(&self) {
        // Pin toolbar so it stays visible until user clicks on webview
        self.toolbar_pinned.set(true);
        self.show_toolbar();
        self.url_entry.grab_focus();
        self.url_entry.select_region(0, -1);
    }

    pub fn reload(&self) {
        if let Some(tab_manager) = self.tab_manager.borrow().as_ref() {
            tab_manager.reload_current();
            // Reset URL bar to current page URL
            if let Some(url) = tab_manager.current_url() {
                self.url_entry.set_text(url.as_str());
            }
        }
    }

    pub fn go_back(&self) {
        if let Some(tab_manager) = self.tab_manager.borrow().as_ref() {
            tab_manager.go_back();
        }
    }

    pub fn go_forward(&self) {
        if let Some(tab_manager) = self.tab_manager.borrow().as_ref() {
            tab_manager.go_forward();
        }
    }

    pub fn navigate_to(&self, url: &Url) {
        if let Some(tab_manager) = self.tab_manager.borrow().as_ref() {
            tab_manager.navigate_to(url);
        }
    }

    /// Update sidebar visibility based on tab count
    /// With proximity mode, sidebar is always controlled by proximity/TAB key
    pub fn update_sidebar_visibility(&self) {
        // No-op: sidebar visibility is now controlled by proximity and TAB key
    }

    /// Restore window from saved session data
    fn restore_from_saved_window(&self, tab_manager: &Rc<TabManager>, saved: SavedWindow) {
        // Restore window size if available
        if let (Some(width), Some(height)) = (saved.window_width, saved.window_height) {
            let obj = self.obj();
            obj.set_default_size(width, height);
            log::info!("Restored window size: {}x{}", width, height);
        }

        if saved.tabs.is_empty() {
            tab_manager.new_tab(None, None);
            return;
        }

        log::info!("Restoring window with {} tabs", saved.tabs.len());

        let mut tab_ids = Vec::new();
        for saved_tab in saved.tabs.iter() {
            let container = saved_tab.container.as_deref();
            let tab_id = if let Ok(url) = Url::parse(&saved_tab.url) {
                tab_manager.new_tab(Some(&url), container)
            } else {
                tab_manager.new_tab(None, container)
            };
            tab_ids.push(tab_id);
        }

        // Switch to the previously active tab
        if let Some(&active_tab_id) = tab_ids.get(saved.active_tab_index) {
            tab_manager.switch_to_tab(active_tab_id);
        }
    }

    /// Get the current window's session data for multi-window save
    pub fn get_session_data(&self) -> Option<SavedWindow> {
        let tab_manager = self.tab_manager.borrow();
        let tab_manager = tab_manager.as_ref()?;

        let (tabs_data, active_index) = tab_manager.get_all_tabs_with_containers();

        let tabs = tabs_data
            .into_iter()
            .map(|(url, title, container)| crate::session::SavedTab {
                url,
                title,
                container: Some(container),
            })
            .collect();

        let obj = self.obj();
        Some(SavedWindow {
            tabs,
            active_tab_index: active_index,
            window_width: Some(obj.width()),
            window_height: Some(obj.height()),
        })
    }

    /// Save current session to the profile directory (legacy single-window)
    pub fn save_session(&self) {
        let profile_path = self.get_profile_path();
        if profile_path.is_none() {
            log::warn!("Cannot save session: no profile path available");
            return;
        }
        let profile_path = profile_path.unwrap();

        if let Some(tab_manager) = self.tab_manager.borrow().as_ref() {
            let (tabs_data, active_index) = tab_manager.get_all_tabs();

            let mut session = Session::new();
            for (url, title) in tabs_data {
                session.add_tab(url, title);
            }
            session.active_tab_index = active_index;

            // Save window size (current size, not default)
            let obj = self.obj();
            session.window_width = Some(obj.width());
            session.window_height = Some(obj.height());

            if let Err(e) = session.save(&profile_path) {
                log::error!("Failed to save session: {}", e);
            } else {
                log::info!("Session saved with {} tabs", session.tabs.len());
            }
        }
    }

    /// Restore session from the profile directory
    pub fn restore_session(&self) -> bool {
        let profile_path = self.get_profile_path();
        if profile_path.is_none() {
            log::warn!("Cannot restore session: no profile path available");
            return false;
        }
        let profile_path = profile_path.unwrap();

        match Session::load(&profile_path) {
            Ok(session) => {
                // Restore window size if available
                if let (Some(width), Some(height)) = (session.window_width, session.window_height) {
                    let obj = self.obj();
                    obj.set_default_size(width, height);
                    log::info!("Restored window size: {}x{}", width, height);
                }

                if session.has_tabs() {
                    log::info!("Restoring session with {} tabs", session.tabs.len());

                    if let Some(tab_manager) = self.tab_manager.borrow().as_ref() {
                        let mut tab_ids = Vec::new();
                        for saved_tab in session.tabs.iter() {
                            // TODO: Restore tab's container from saved session
                            let tab_id = if let Ok(url) = Url::parse(&saved_tab.url) {
                                tab_manager.new_tab(Some(&url), None)
                            } else {
                                tab_manager.new_tab(None, None)
                            };
                            tab_ids.push(tab_id);
                        }

                        // Switch to the previously active tab
                        if let Some(&active_tab_id) = tab_ids.get(session.active_tab_index) {
                            tab_manager.switch_to_tab(active_tab_id);
                        }
                    }
                    true
                } else {
                    log::debug!("No tabs to restore");
                    false
                }
            }
            Err(e) => {
                log::warn!("Failed to load session: {}", e);
                false
            }
        }
    }

    /// Get the profile path from the application
    fn get_profile_path(&self) -> Option<std::path::PathBuf> {
        let obj = self.obj();
        let app = obj.application()?;
        let qayeq_app = app.downcast_ref::<QayeqApplication>()?;
        let profile_manager = qayeq_app.profile_manager()?;
        let profile = profile_manager.current_profile()?;
        Some(profile.path().clone())
    }

    /// Set up the integrated completion list
    fn setup_completion_list(&self, app: &QayeqApplication, tab_manager: &Rc<TabManager>) {
        // Get search URL template from settings
        let settings = gio::Settings::new(config::APP_ID);
        let search_engine = settings.string("search-engine");
        let search_url_template: Rc<str> = config::SEARCH_ENGINES
            .iter()
            .find(|(id, _, _)| *id == search_engine.as_str())
            .map(|(_, _, url)| *url)
            .unwrap_or("https://duckduckgo.com/?q={}")
            .into();

        // Check if suggestions are enabled
        let suggestions_enabled = settings.boolean("search-suggestions-enabled");

        // Connect URL entry changed signal
        let obj = self.obj().clone();
        let app_weak = app.downgrade();
        let search_template = Rc::clone(&search_url_template);
        self.url_entry.connect_changed(move |entry| {
            let imp = obj.imp();

            // Skip if this is from apply_inline_autocomplete setting text
            if imp.setting_autocomplete.get() {
                return;
            }

            let text = entry.text().to_string();

            // Reset explicit selection when user types
            imp.explicit_selection.set(false);

            // Hide completions if empty
            if text.is_empty() {
                imp.completion_scroll.set_visible(false);
                *imp.autocomplete_url.borrow_mut() = None;
                imp.user_input_len.set(0);
                return;
            }

            // Skip if this looks like a full URL being set programmatically (contains ://)
            if text.contains("://") {
                return;
            }

            // Check if this change is from user typing while autocomplete is active
            let prev_input_len = imp.user_input_len.get();
            if prev_input_len > 0 {
                // We have active autocomplete - check if user typed one char
                let autocomplete_active = imp.autocomplete_url.borrow().is_some();
                if autocomplete_active && text.len() == prev_input_len + 1 {
                    // User typed one character that replaced the selection
                    // Update user_input_len and re-select the suffix
                    imp.user_input_len.set(prev_input_len + 1);

                    // Set flag before modifying selection
                    imp.setting_autocomplete.set(true);
                    entry.select_region((prev_input_len + 1) as i32, -1);
                    imp.setting_autocomplete.set(false);
                    return;
                }
            }

            // New input or autocomplete invalidated - use actual text as query
            let query = text.clone();

            // Clear autocomplete state
            *imp.autocomplete_url.borrow_mut() = None;
            imp.user_input_len.set(0);

            imp.update_completions(&query, &app_weak, suggestions_enabled, &search_template);
        });

        // Connect list row activation to navigate
        let tm = Rc::clone(tab_manager);
        let url_entry = self.url_entry.clone();
        let completion_scroll = self.completion_scroll.clone();
        let search_template = search_url_template;
        self.completion_list.connect_row_activated(move |_, list_row| {
            if let Some(row) = list_row.child() {
                if let Ok(completion_row) = row.downcast::<CompletionRow>() {
                    if let Some(item) = completion_row.item() {
                        let url = item.action_url(&search_template);
                        completion_scroll.set_visible(false);
                        if let Ok(parsed_url) = Url::parse(&url) {
                            tm.navigate_to(&parsed_url);
                            url_entry.set_text(&url);
                        } else if let Ok(search_url) = url_bar::build_search_url(&url) {
                            tm.navigate_to(&search_url);
                        }
                    }
                }
            }
        });
    }

    /// Trigger completion fetch for current URL entry text (used by Down arrow)
    fn trigger_completions(&self) {
        let query = self.url_entry.text().to_string();
        if query.is_empty() || query.contains("://") {
            return;
        }

        let obj = self.obj();
        let Some(app) = obj.application().and_then(|a| a.downcast::<QayeqApplication>().ok()) else {
            return;
        };
        let app_weak = app.downgrade();

        let settings = gio::Settings::new(config::APP_ID);
        let suggestions_enabled = settings.boolean("search-suggestions-enabled");

        // Get search URL template from settings
        let search_engine = settings.string("search-engine");
        let search_template: Rc<str> = config::SEARCH_ENGINES
            .iter()
            .find(|(id, _, _)| *id == search_engine.as_str())
            .map(|(_, _, url)| *url)
            .unwrap_or("https://duckduckgo.com/?q={}")
            .into();

        self.update_completions(&query, &app_weak, suggestions_enabled, &search_template);
    }

    /// Update completions for the given query
    fn update_completions(
        &self,
        query: &str,
        app_weak: &glib::WeakRef<QayeqApplication>,
        suggestions_enabled: bool,
        search_template: &Rc<str>,
    ) {
        // Cancel any pending debounce
        if let Some(source_id) = self.completion_debounce.borrow_mut().take() {
            let _ = source_id.remove();
        }

        // Cancel any pending search request
        if let Some(cancellable) = self.completion_cancellable.borrow_mut().take() {
            cancellable.cancel();
        }

        if query.is_empty() {
            self.completion_scroll.set_visible(false);
            return;
        }

        // Debounce the request
        let query = query.to_string();
        let obj = self.obj().clone();
        let app_weak = app_weak.clone();
        let search_template = Rc::clone(search_template);
        let source_ref = Rc::clone(&self.completion_debounce);

        let source_id = glib::timeout_add_local_once(
            Duration::from_millis(config::COMPLETION_DEBOUNCE_MS),
            move || {
                *source_ref.borrow_mut() = None;
                obj.imp().fetch_completions(&query, &app_weak, suggestions_enabled, &search_template);
            },
        );
        *self.completion_debounce.borrow_mut() = Some(source_id);
    }

    /// Fetch completions (called after debounce)
    fn fetch_completions(
        &self,
        query: &str,
        app_weak: &glib::WeakRef<QayeqApplication>,
        suggestions_enabled: bool,
        _search_template: &Rc<str>,
    ) {
        // Get history completions
        let history_items = if let Some(app) = app_weak.upgrade() {
            if let Some(pm) = app.profile_manager() {
                if let Some(profile) = pm.current_profile() {
                    let items = get_history_completions(profile.history(), query);
                    log::debug!("History completions for '{}': {} items from profile '{}'",
                        query, items.len(), profile.name());
                    items
                } else {
                    log::warn!("No current profile for history completions");
                    Vec::new()
                }
            } else {
                log::warn!("No profile manager for history completions");
                Vec::new()
            }
        } else {
            log::warn!("No app for history completions");
            Vec::new()
        };

        // Update list with history items first
        self.update_completion_list(&history_items, query);

        // Apply inline autocomplete from best history match
        self.apply_inline_autocomplete(&history_items, query);

        // Fetch search suggestions if enabled
        if suggestions_enabled {
            let cancellable = gio::Cancellable::new();
            *self.completion_cancellable.borrow_mut() = Some(cancellable.clone());

            let obj = self.obj().clone();
            let query_owned = query.to_string();
            let history_items = history_items.clone();

            fetch_search_suggestions(&query_owned.clone(), Some(&cancellable), move |suggestions| {
                log::debug!("Search suggestions callback: {} history + {} suggestions",
                    history_items.len(), suggestions.len());
                let mut combined = history_items;
                combined.extend(suggestions);
                combined.truncate(config::COMPLETION_MAX_RESULTS);
                log::debug!("Combined completions: {} items", combined.len());
                obj.imp().update_completion_list(&combined, &query_owned);
            });
        }
    }

    /// Move completion selection up or down
    fn move_completion_selection(&self, delta: i32) {
        let list = &*self.completion_list;
        let selected = list.selected_row();

        let new_index = if let Some(row) = selected {
            row.index() + delta
        } else if delta > 0 {
            0 // Start at first item when pressing Down
        } else {
            return; // No selection and pressing Up, do nothing
        };

        // Clamp to valid range
        let mut count = 0;
        let mut child = list.first_child();
        while child.is_some() {
            count += 1;
            child = child.and_then(|c| c.next_sibling());
        }

        if new_index >= 0 && new_index < count {
            if let Some(row) = list.row_at_index(new_index) {
                list.select_row(Some(&row));
                // Scroll the row into view
                row.grab_focus();
                self.url_entry.grab_focus();
            }
        }
    }

    /// Update the completion list with items
    fn update_completion_list(&self, items: &[CompletionItem], query: &str) {
        // Remember current selection index before clearing
        let previous_selection_index = self.completion_list.selected_row().map(|r| r.index());

        // Remove all existing rows
        while let Some(child) = self.completion_list.first_child() {
            self.completion_list.remove(&child);
        }

        // Clear selection
        self.completion_list.unselect_all();

        if items.is_empty() {
            self.completion_scroll.set_visible(false);
            return;
        }

        // Add rows for each item
        for item in items {
            let row = CompletionRow::new();
            row.set_item(item.clone(), query);
            self.completion_list.append(&row);
        }

        self.completion_scroll.set_visible(true);

        // Restore selection if there was one and it's still valid
        if let Some(prev_index) = previous_selection_index {
            let item_count = items.len() as i32;
            if prev_index >= 0 && prev_index < item_count {
                if let Some(row) = self.completion_list.row_at_index(prev_index) {
                    self.completion_list.select_row(Some(&row));
                }
            }
        }
    }

    /// Apply inline autocomplete from history items
    /// Sets entry text to best match with suffix selected
    fn apply_inline_autocomplete(&self, history_items: &[CompletionItem], user_input: &str) {
        // Clear previous autocomplete state
        *self.autocomplete_url.borrow_mut() = None;
        self.user_input_len.set(0);

        if user_input.is_empty() || history_items.is_empty() {
            return;
        }

        // Find first history item whose simplified URL starts with user input
        let user_input_lower = user_input.to_lowercase();
        let best_match = history_items.iter().find(|item| {
            if let Some(url) = &item.url {
                let simplified = simplify_url_for_autocomplete(url);
                simplified.to_lowercase().starts_with(&user_input_lower)
            } else {
                false
            }
        });

        if let Some(item) = best_match {
            if let Some(url) = &item.url {
                let simplified = simplify_url_for_autocomplete(url);

                // Only autocomplete if simplified URL is longer than input
                if simplified.len() > user_input.len() {
                    // Store autocomplete state
                    *self.autocomplete_url.borrow_mut() = Some(url.clone());
                    self.user_input_len.set(user_input.len());

                    // Set flag to prevent connect_changed from triggering
                    self.setting_autocomplete.set(true);

                    // Set entry text to simplified URL with suffix selected
                    self.url_entry.set_text(&simplified);
                    self.url_entry.select_region(user_input.len() as i32, -1);

                    self.setting_autocomplete.set(false);

                    log::debug!("Inline autocomplete: '{}' -> '{}' (selected from {})",
                        user_input, simplified, user_input.len());
                }
            }
        }
    }
}

/// Simplify URL for autocomplete display (strip scheme, www prefix)
fn simplify_url_for_autocomplete(url_str: &str) -> String {
    if let Ok(url) = Url::parse(url_str) {
        let host = url.host_str().unwrap_or("");
        let host_clean = host.strip_prefix("www.").unwrap_or(host);
        let path = url.path();

        if path == "/" || path.is_empty() {
            host_clean.to_string()
        } else {
            format!("{}{}", host_clean, path)
        }
    } else {
        url_str.to_string()
    }
}

impl WidgetImpl for QayeqWindow {}

impl WindowImpl for QayeqWindow {
    fn close_request(&self) -> glib::Propagation {
        // Note: Session saving is handled by the application on shutdown,
        // which aggregates all open windows into a multi-window session.

        // Record closed window for "Reopen Closed Window" functionality
        if let Some(tab_manager) = self.tab_manager.borrow().as_ref() {
            let (tabs_data, active_index) = tab_manager.get_all_tabs_with_containers();
            if !tabs_data.is_empty() {
                let closed_tabs: Vec<ClosedTab> = tabs_data
                    .into_iter()
                    .map(|(url, title, container)| ClosedTab {
                        url,
                        title,
                        container,
                        closed_at: std::time::SystemTime::now(),
                    })
                    .collect();

                let closed_window = ClosedWindow {
                    tabs: closed_tabs,
                    active_tab_index: active_index,
                    closed_at: std::time::SystemTime::now(),
                };

                // Record to application's closed windows storage
                if let Some(app) = self.obj().application() {
                    if let Some(qayeq_app) = app.downcast_ref::<QayeqApplication>() {
                        qayeq_app.record_closed_window(closed_window);
                    }
                }
            }
        }

        // Continue with default close behavior
        glib::Propagation::Proceed
    }
}

impl ApplicationWindowImpl for QayeqWindow {}
impl AdwApplicationWindowImpl for QayeqWindow {}

