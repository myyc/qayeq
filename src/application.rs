use crate::clear_data::{ClearDataDialog, ClearDataOptions};
use crate::config;
use crate::content_filter::ContentFilterManager;
use crate::preferences::PreferencesWindow;
use crate::profile::ProfileManager;
use crate::recently_closed::RecentlyClosedWindows;
use crate::session::MultiWindowSession;
use crate::window::QayeqWindow;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{gio, glib};
use libadwaita::prelude::*;
use libadwaita::subclass::prelude::*;
use std::cell::{Cell, OnceCell, RefCell};

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct QayeqApplication {
        pub profile_manager: OnceCell<ProfileManager>,
        pub content_filter_manager: OnceCell<ContentFilterManager>,
        pub closed_windows: RefCell<RecentlyClosedWindows>,
        pub session_restored: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for QayeqApplication {
        const NAME: &'static str = "QayeqApplication";
        type Type = super::QayeqApplication;
        type ParentType = libadwaita::Application;
    }

    impl ObjectImpl for QayeqApplication {}

    impl ApplicationImpl for QayeqApplication {
        fn activate(&self) {
            let app = self.obj();

            // On first activation, try to restore multi-window session
            if !self.session_restored.get() {
                self.session_restored.set(true);

                if let Some(pm) = self.profile_manager.get() {
                    if let Some(profile) = pm.current_profile() {
                        match MultiWindowSession::load(profile.path()) {
                            Ok(session) if session.has_windows() => {
                                log::info!("Restoring {} windows from session", session.windows.len());
                                for saved_window in session.windows {
                                    let window = QayeqWindow::new_with_session(&app, saved_window);
                                    window.present();
                                }
                                return;
                            }
                            Ok(_) => {
                                log::debug!("No windows in session to restore");
                            }
                            Err(e) => {
                                log::warn!("Failed to load session: {}", e);
                            }
                        }
                    }
                }
            }

            // Default: create a single empty window
            let window = QayeqWindow::new_empty(&app);
            window.present();
        }

        fn startup(&self) {
            self.parent_startup();

            let app = self.obj();

            // Load CSS
            app.load_css();

            // Initialize profile manager
            match ProfileManager::new() {
                Ok(pm) => {
                    // Initialize content filter manager with the profile's data directory
                    let data_dir = pm.base_dir().clone();
                    let cfm = ContentFilterManager::new(&data_dir);
                    cfm.initialize_filters();

                    if self.content_filter_manager.set(cfm).is_err() {
                        log::warn!("Content filter manager already initialized");
                    } else {
                        log::info!("Content filter manager initialized");
                    }

                    self.profile_manager.set(pm).unwrap();
                    log::info!("Profile manager initialized");
                }
                Err(e) => {
                    log::error!("Failed to initialize profile manager: {}", e);
                }
            }

            // Set up application actions
            app.setup_actions();
            app.setup_accels();
        }

        fn shutdown(&self) {
            self.parent_shutdown();

            // Save all windows to multi-window session
            let app = self.obj();
            app.save_all_windows();
        }
    }

    impl GtkApplicationImpl for QayeqApplication {}
    impl AdwApplicationImpl for QayeqApplication {}
}

glib::wrapper! {
    pub struct QayeqApplication(ObjectSubclass<imp::QayeqApplication>)
        @extends libadwaita::Application, gtk4::Application, gio::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl QayeqApplication {
    pub fn new() -> Self {
        glib::Object::builder()
            .property("application-id", config::APP_ID)
            .property("flags", gio::ApplicationFlags::HANDLES_OPEN)
            .build()
    }

    pub fn profile_manager(&self) -> Option<&ProfileManager> {
        self.imp().profile_manager.get()
    }

    /// Get the content filter manager (for ad blocking)
    pub fn content_filter_manager(&self) -> Option<&ContentFilterManager> {
        self.imp().content_filter_manager.get()
    }

    fn load_css(&self) {
        let provider = gtk4::CssProvider::new();
        provider.load_from_resource("/dev/myyc/qayeq/style.css");

        gtk4::style_context_add_provider_for_display(
            &gtk4::gdk::Display::default().expect("Could not get default display"),
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        log::info!("CSS loaded");
    }

    fn setup_actions(&self) {
        // Quit action
        let quit_action = gio::SimpleAction::new("quit", None);
        quit_action.connect_activate(glib::clone!(
            #[weak(rename_to = app)]
            self,
            move |_, _| {
                app.quit();
            }
        ));
        self.add_action(&quit_action);

        // New window action (opens empty window, not session restore)
        let new_window_action = gio::SimpleAction::new("new-window", None);
        new_window_action.connect_activate(glib::clone!(
            #[weak(rename_to = app)]
            self,
            move |_, _| {
                let window = QayeqWindow::new_empty(&app);
                window.present();
            }
        ));
        self.add_action(&new_window_action);

        // New tab action
        let new_tab_action = gio::SimpleAction::new("new-tab", None);
        new_tab_action.connect_activate(glib::clone!(
            #[weak(rename_to = app)]
            self,
            move |_, _| {
                if let Some(window) = app.active_window() {
                    if let Some(qayeq_window) = window.downcast_ref::<QayeqWindow>() {
                        qayeq_window.new_tab(None);
                    }
                }
            }
        ));
        self.add_action(&new_tab_action);

        // New tab in container actions (Ctrl+Shift+1-4)
        for (container_id, container_name, _) in config::DEFAULT_CONTAINERS {
            let action_name = format!("new-tab-{}", container_id);
            let container_id_owned = container_id.to_string();
            let action = gio::SimpleAction::new(&action_name, None);
            action.connect_activate(glib::clone!(
                #[weak(rename_to = app)]
                self,
                move |_, _| {
                    if let Some(window) = app.active_window() {
                        if let Some(qayeq_window) = window.downcast_ref::<QayeqWindow>() {
                            qayeq_window.new_tab_in_container(None, Some(&container_id_owned));
                        }
                    }
                }
            ));
            self.add_action(&action);
            log::debug!("Added action app.{} for container '{}'", action_name, container_name);
        }

        // Close tab action
        let close_tab_action = gio::SimpleAction::new("close-tab", None);
        close_tab_action.connect_activate(glib::clone!(
            #[weak(rename_to = app)]
            self,
            move |_, _| {
                if let Some(window) = app.active_window() {
                    if let Some(qayeq_window) = window.downcast_ref::<QayeqWindow>() {
                        qayeq_window.close_current_tab();
                    }
                }
            }
        ));
        self.add_action(&close_tab_action);

        // Reopen closed tab action
        let reopen_tab_action = gio::SimpleAction::new("reopen-tab", None);
        reopen_tab_action.connect_activate(glib::clone!(
            #[weak(rename_to = app)]
            self,
            move |_, _| {
                if let Some(window) = app.active_window() {
                    if let Some(qayeq_window) = window.downcast_ref::<QayeqWindow>() {
                        qayeq_window.imp().reopen_last_closed_tab();
                    }
                }
            }
        ));
        self.add_action(&reopen_tab_action);

        // Reopen closed window action
        let reopen_window_action = gio::SimpleAction::new("reopen-window", None);
        reopen_window_action.connect_activate(glib::clone!(
            #[weak(rename_to = app)]
            self,
            move |_, _| {
                app.reopen_last_closed_window();
            }
        ));
        self.add_action(&reopen_window_action);

        // Focus URL bar action
        let focus_url_action = gio::SimpleAction::new("focus-url-bar", None);
        focus_url_action.connect_activate(glib::clone!(
            #[weak(rename_to = app)]
            self,
            move |_, _| {
                if let Some(window) = app.active_window() {
                    if let Some(qayeq_window) = window.downcast_ref::<QayeqWindow>() {
                        qayeq_window.focus_url_bar();
                    }
                }
            }
        ));
        self.add_action(&focus_url_action);

        // Reload action
        let reload_action = gio::SimpleAction::new("reload", None);
        reload_action.connect_activate(glib::clone!(
            #[weak(rename_to = app)]
            self,
            move |_, _| {
                if let Some(window) = app.active_window() {
                    if let Some(qayeq_window) = window.downcast_ref::<QayeqWindow>() {
                        qayeq_window.reload();
                    }
                }
            }
        ));
        self.add_action(&reload_action);

        // Go back action
        let back_action = gio::SimpleAction::new("go-back", None);
        back_action.connect_activate(glib::clone!(
            #[weak(rename_to = app)]
            self,
            move |_, _| {
                if let Some(window) = app.active_window() {
                    if let Some(qayeq_window) = window.downcast_ref::<QayeqWindow>() {
                        qayeq_window.go_back();
                    }
                }
            }
        ));
        self.add_action(&back_action);

        // Go forward action
        let forward_action = gio::SimpleAction::new("go-forward", None);
        forward_action.connect_activate(glib::clone!(
            #[weak(rename_to = app)]
            self,
            move |_, _| {
                if let Some(window) = app.active_window() {
                    if let Some(qayeq_window) = window.downcast_ref::<QayeqWindow>() {
                        qayeq_window.go_forward();
                    }
                }
            }
        ));
        self.add_action(&forward_action);

        // About action
        let about_action = gio::SimpleAction::new("about", None);
        about_action.connect_activate(glib::clone!(
            #[weak(rename_to = app)]
            self,
            move |_, _| {
                app.show_about();
            }
        ));
        self.add_action(&about_action);

        // Toggle sidebar action (TAB key)
        let toggle_sidebar_action = gio::SimpleAction::new("toggle-sidebar", None);
        toggle_sidebar_action.connect_activate(glib::clone!(
            #[weak(rename_to = app)]
            self,
            move |_, _| {
                if let Some(window) = app.active_window() {
                    if let Some(qayeq_window) = window.downcast_ref::<QayeqWindow>() {
                        qayeq_window.toggle_sidebar();
                    }
                }
            }
        ));
        self.add_action(&toggle_sidebar_action);

        // Clear browsing data action
        let clear_data_action = gio::SimpleAction::new("clear-data", None);
        clear_data_action.connect_activate(glib::clone!(
            #[weak(rename_to = app)]
            self,
            move |_, _| {
                app.show_clear_data_dialog();
            }
        ));
        self.add_action(&clear_data_action);

        // Preferences action (placeholder for now)
        let preferences_action = gio::SimpleAction::new("preferences", None);
        preferences_action.connect_activate(glib::clone!(
            #[weak(rename_to = app)]
            self,
            move |_, _| {
                app.show_preferences();
            }
        ));
        self.add_action(&preferences_action);
    }

    fn setup_accels(&self) {
        self.set_accels_for_action("app.quit", &["<Control>q"]);
        self.set_accels_for_action("app.new-window", &["<Control><Shift>n"]);
        self.set_accels_for_action("app.new-tab", &["<Control>t"]);
        self.set_accels_for_action("app.close-tab", &["<Control>w"]);
        self.set_accels_for_action("app.reopen-tab", &["<Control><Shift>t"]);
        self.set_accels_for_action("app.focus-url-bar", &["<Control>l"]);
        self.set_accels_for_action("app.reload", &["<Control>r", "F5"]);
        self.set_accels_for_action("app.go-back", &["<Alt>Left"]);
        self.set_accels_for_action("app.go-forward", &["<Alt>Right"]);
        self.set_accels_for_action("app.toggle-sidebar", &["F1"]);
        self.set_accels_for_action("app.preferences", &["<Control>comma"]);

        // Container shortcuts: Ctrl+Shift+1-4 for opening tabs in containers
        // Matches order in config::DEFAULT_CONTAINERS
        self.set_accels_for_action("app.new-tab-personal", &["<Control><Shift>1"]);
        self.set_accels_for_action("app.new-tab-work", &["<Control><Shift>2"]);
        self.set_accels_for_action("app.new-tab-shopping", &["<Control><Shift>3"]);
        self.set_accels_for_action("app.new-tab-banking", &["<Control><Shift>4"]);
    }

    fn show_about(&self) {
        let about = libadwaita::AboutDialog::builder()
            .application_name(config::APP_NAME)
            .version(config::APP_VERSION)
            .application_icon(config::APP_ID)
            .developer_name("myyc")
            .license_type(gtk4::License::Gpl30)
            .website("https://github.com/myyc/qayeq")
            .issue_url("https://github.com/myyc/qayeq/issues")
            .comments("A GNOME web browser")
            .build();

        if let Some(window) = self.active_window() {
            about.present(Some(&window));
        }
    }

    fn show_clear_data_dialog(&self) {
        let Some(window) = self.active_window() else {
            return;
        };

        let dialog = ClearDataDialog::new();
        let app = self.clone();

        dialog.present(&window, move |options| {
            app.clear_browsing_data(options);
        });
    }

    fn clear_browsing_data(&self, options: ClearDataOptions) {
        log::info!(
            "Clearing browsing data: history={}, cookies={}, cache={}, range={:?}",
            options.clear_history,
            options.clear_cookies,
            options.clear_cache,
            options.time_range
        );

        let timestamp = options.time_range.to_timestamp();

        // Clear history
        if options.clear_history {
            if let Some(pm) = self.profile_manager() {
                if let Some(profile) = pm.current_profile() {
                    let result = match timestamp {
                        Some(ts) => profile.history().clear_older_than(ts).map(|_| ()),
                        None => profile.history().clear_all(),
                    };
                    if let Err(e) = result {
                        log::error!("Failed to clear history: {}", e);
                    } else {
                        log::info!("History cleared successfully");
                    }
                }
            }
        }

        // Clear cookies and cache via WebKit
        if options.clear_cookies || options.clear_cache {
            if let Some(pm) = self.profile_manager() {
                if let Some(profile) = pm.current_profile() {
                    let session = profile.network_session();
                    if let Some(data_manager) = session.website_data_manager() {
                        let mut types = webkit6::WebsiteDataTypes::empty();
                        if options.clear_cookies {
                            types |= webkit6::WebsiteDataTypes::COOKIES;
                        }
                        if options.clear_cache {
                            types |= webkit6::WebsiteDataTypes::DISK_CACHE;
                            types |= webkit6::WebsiteDataTypes::MEMORY_CACHE;
                        }

                        // WebKit's clear method needs a timespan (seconds from now going back)
                        // For "all time", use a very large value
                        let timespan = match timestamp {
                            Some(ts) => {
                                let now = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs() as i64;
                                glib::TimeSpan::from_seconds((now - ts) as i64)
                            }
                            None => glib::TimeSpan::from_seconds(i64::MAX / 1_000_000), // All time
                        };

                        data_manager.clear(
                            types,
                            timespan,
                            gio::Cancellable::NONE,
                            |result| {
                                if let Err(e) = result {
                                    log::error!("Failed to clear website data: {}", e);
                                } else {
                                    log::info!("Website data cleared successfully");
                                }
                            },
                        );
                    }
                }
            }
        }
    }

    fn show_preferences(&self) {
        let Some(window) = self.active_window() else {
            return;
        };

        let prefs = PreferencesWindow::new(&window);
        prefs.present();
    }

    /// Record a closed window for later reopening.
    pub fn record_closed_window(&self, closed_window: crate::recently_closed::ClosedWindow) {
        self.imp().closed_windows.borrow_mut().push(closed_window);
    }

    /// Reopen the most recently closed window.
    pub fn reopen_last_closed_window(&self) {
        if let Some(closed_window) = self.imp().closed_windows.borrow_mut().pop() {
            let window = QayeqWindow::new_empty(self);

            // Restore tabs from the closed window
            for closed_tab in &closed_window.tabs {
                if let Ok(url) = url::Url::parse(&closed_tab.url) {
                    window.new_tab_in_container(Some(&url), Some(&closed_tab.container));
                }
            }

            window.present();
        }
    }

    /// Save all open windows to multi-window session.
    /// Called immediately when session changes (tab added/closed, navigation).
    pub fn save_all_windows(&self) {
        let Some(pm) = self.profile_manager() else {
            log::warn!("Cannot save windows: no profile manager");
            return;
        };
        let Some(profile) = pm.current_profile() else {
            log::warn!("Cannot save windows: no current profile");
            return;
        };

        let mut session = MultiWindowSession::new();

        // Iterate through all windows and collect their session data
        for window in self.windows() {
            if let Some(qayeq_window) = window.downcast_ref::<QayeqWindow>() {
                if let Some(window_data) = qayeq_window.get_session_data() {
                    if !window_data.tabs.is_empty() {
                        session.add_window(window_data);
                    }
                }
            }
        }

        if session.has_windows() {
            if let Err(e) = session.save(profile.path()) {
                log::error!("Failed to save multi-window session: {}", e);
            } else {
                log::info!("Saved {} windows to session", session.windows.len());
            }
        } else {
            log::debug!("No windows to save");
        }
    }
}

impl Default for QayeqApplication {
    fn default() -> Self {
        Self::new()
    }
}
