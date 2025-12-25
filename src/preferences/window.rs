//! Preferences window implementation.

#![allow(deprecated)] // PreferencesWindow is deprecated since 1.6, but still works

use crate::config;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{gio, glib};
use libadwaita::prelude::*;
use libadwaita::subclass::prelude::*;

mod imp {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default, gtk4::CompositeTemplate)]
    #[template(resource = "/dev/myyc/qayeq/ui/preferences.ui")]
    pub struct PreferencesWindow {
        #[template_child]
        pub search_engine_row: TemplateChild<libadwaita::ComboRow>,
        #[template_child]
        pub custom_search_row: TemplateChild<libadwaita::EntryRow>,
        #[template_child]
        pub search_suggestions_row: TemplateChild<libadwaita::SwitchRow>,
        #[template_child]
        pub homepage_row: TemplateChild<libadwaita::EntryRow>,
        #[template_child]
        pub new_tab_row: TemplateChild<libadwaita::EntryRow>,
        #[template_child]
        pub restore_session_row: TemplateChild<libadwaita::SwitchRow>,

        pub settings: RefCell<Option<gio::Settings>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PreferencesWindow {
        const NAME: &'static str = "QayeqPreferencesWindow";
        type Type = super::PreferencesWindow;
        type ParentType = libadwaita::PreferencesWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for PreferencesWindow {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();
            obj.setup_settings();
            obj.bind_settings();
            obj.setup_search_engine_visibility();
        }
    }

    impl WidgetImpl for PreferencesWindow {}
    impl WindowImpl for PreferencesWindow {}
    impl AdwWindowImpl for PreferencesWindow {}
    impl PreferencesWindowImpl for PreferencesWindow {}
}

glib::wrapper! {
    pub struct PreferencesWindow(ObjectSubclass<imp::PreferencesWindow>)
        @extends libadwaita::PreferencesWindow, libadwaita::Window, gtk4::Window, gtk4::Widget,
        @implements gio::ActionGroup, gio::ActionMap, gtk4::Accessible, gtk4::Buildable,
                    gtk4::ConstraintTarget, gtk4::Native, gtk4::Root, gtk4::ShortcutManager;
}

impl PreferencesWindow {
    pub fn new(parent: &impl IsA<gtk4::Window>) -> Self {
        glib::Object::builder()
            .property("transient-for", parent)
            .build()
    }

    fn setup_settings(&self) {
        let settings = gio::Settings::new(config::APP_ID);
        self.imp().settings.replace(Some(settings));
    }

    fn bind_settings(&self) {
        let imp = self.imp();
        let settings = imp.settings.borrow();
        let settings = settings.as_ref().unwrap();

        // Bind search engine selection
        let search_engine = settings.string("search-engine");
        let engine_index = config::SEARCH_ENGINES
            .iter()
            .position(|(id, _, _)| *id == search_engine.as_str())
            .unwrap_or(0) as u32;
        imp.search_engine_row.set_selected(engine_index);

        imp.search_engine_row.connect_selected_notify(
            glib::clone!(
                #[weak(rename_to = window)]
                self,
                move |row| {
                    let index = row.selected() as usize;
                    if let Some((id, _, _)) = config::SEARCH_ENGINES.get(index) {
                        if let Some(settings) = window.imp().settings.borrow().as_ref() {
                            let _ = settings.set_string("search-engine", id);
                        }
                    }
                    window.update_custom_search_visibility();
                }
            ),
        );

        // Bind custom search URL
        settings
            .bind("custom-search-url", &*imp.custom_search_row, "text")
            .build();

        // Bind search suggestions
        settings
            .bind(
                "search-suggestions-enabled",
                &*imp.search_suggestions_row,
                "active",
            )
            .build();

        // Bind homepage
        settings
            .bind("homepage", &*imp.homepage_row, "text")
            .build();

        // Bind new tab URL
        settings
            .bind("new-tab-url", &*imp.new_tab_row, "text")
            .build();

        // Bind restore session
        settings
            .bind("restore-session", &*imp.restore_session_row, "active")
            .build();
    }

    fn setup_search_engine_visibility(&self) {
        self.update_custom_search_visibility();
    }

    fn update_custom_search_visibility(&self) {
        let imp = self.imp();
        let selected = imp.search_engine_row.selected() as usize;
        let is_custom = config::SEARCH_ENGINES
            .get(selected)
            .map(|(id, _, _)| *id == "custom")
            .unwrap_or(false);
        imp.custom_search_row.set_visible(is_custom);
    }
}

impl Default for PreferencesWindow {
    fn default() -> Self {
        glib::Object::builder().build()
    }
}
