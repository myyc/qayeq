mod imp;

use crate::application::QayeqApplication;
use crate::session::SavedWindow;
use glib::subclass::prelude::*;
use gtk4::glib;
use url::Url;

glib::wrapper! {
    pub struct QayeqWindow(ObjectSubclass<imp::QayeqWindow>)
        @extends libadwaita::ApplicationWindow, gtk4::ApplicationWindow, gtk4::Window, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Native, gtk4::Root, gtk4::ShortcutManager, gio::ActionGroup, gio::ActionMap;
}

impl QayeqWindow {
    /// Create a new window that will restore session on startup.
    pub fn new(app: &QayeqApplication) -> Self {
        glib::Object::builder()
            .property("application", app)
            .build()
    }

    /// Create an empty window (no session restore, just one blank tab).
    pub fn new_empty(app: &QayeqApplication) -> Self {
        let window: Self = glib::Object::builder()
            .property("application", app)
            .build();
        window.imp().skip_session_restore.set(true);
        window
    }

    /// Create a window and restore it from a saved session.
    pub fn new_with_session(app: &QayeqApplication, saved: SavedWindow) -> Self {
        let window: Self = glib::Object::builder()
            .property("application", app)
            .build();
        // Skip automatic session restore - we'll do it manually with the provided data
        window.imp().skip_session_restore.set(true);
        // Store the saved window data for restoration after tab manager initializes
        *window.imp().pending_session.borrow_mut() = Some(saved);
        window
    }

    pub fn new_tab(&self, url: Option<&Url>) {
        self.new_tab_in_container(url, None);
    }

    pub fn new_tab_in_container(&self, url: Option<&Url>, container: Option<&str>) {
        self.imp().new_tab(url, container);
        self.imp().focus_url_bar();
    }

    pub fn close_current_tab(&self) {
        self.imp().close_current_tab();
        self.imp().focus_url_bar();
    }

    pub fn focus_url_bar(&self) {
        self.imp().focus_url_bar();
    }

    pub fn reload(&self) {
        self.imp().reload();
    }

    pub fn go_back(&self) {
        self.imp().go_back();
    }

    pub fn go_forward(&self) {
        self.imp().go_forward();
    }

    pub fn navigate_to(&self, url: &Url) {
        self.imp().navigate_to(url);
    }

    pub fn show_sidebar(&self) {
        self.imp().show_sidebar();
    }

    pub fn toggle_sidebar(&self) {
        self.imp().toggle_sidebar();
    }

    /// Get the current window's session data for multi-window save.
    pub fn get_session_data(&self) -> Option<crate::session::SavedWindow> {
        self.imp().get_session_data()
    }
}

use gtk4::gio;
