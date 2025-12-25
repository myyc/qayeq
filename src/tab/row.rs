use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::glib;
use std::cell::{Cell, RefCell};

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct TabRow {
        pub tab_id: Cell<u32>,
        pub title: RefCell<String>,
        pub favicon: RefCell<Option<gtk4::gdk::Texture>>,
        pub is_loading: Cell<bool>,
        pub container_id: RefCell<String>,

        // Child widgets
        pub content_box: RefCell<Option<gtk4::Box>>,
        pub container_indicator: RefCell<Option<gtk4::Box>>,
        pub icon_stack: RefCell<Option<gtk4::Stack>>,
        pub favicon_image: RefCell<Option<gtk4::Image>>,
        pub title_label: RefCell<Option<gtk4::Label>>,
        pub close_button: RefCell<Option<gtk4::Button>>,
        pub spinner: RefCell<Option<gtk4::Spinner>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TabRow {
        const NAME: &'static str = "QayeqTabRow";
        type Type = super::TabRow;
        type ParentType = gtk4::ListBoxRow;
    }

    impl ObjectImpl for TabRow {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();
            obj.add_css_class("tab-row");

            // Content box - compact padding
            let content_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            content_box.set_margin_start(4);
            content_box.set_margin_end(4);
            content_box.set_margin_top(4);
            content_box.set_margin_bottom(4);
            content_box.set_valign(gtk4::Align::Center);
            obj.set_child(Some(&content_box));

            // Container indicator - small colored bar
            let container_indicator = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            container_indicator.set_width_request(3);
            container_indicator.set_margin_end(6);
            container_indicator.add_css_class("container-indicator");
            content_box.append(&container_indicator);
            *self.container_indicator.borrow_mut() = Some(container_indicator);

            // Favicon / spinner container
            let icon_stack = gtk4::Stack::new();
            icon_stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
            icon_stack.add_css_class("tab-favicon");

            let favicon_image = gtk4::Image::from_icon_name("web-browser-symbolic");
            favicon_image.set_pixel_size(20);
            icon_stack.add_named(&favicon_image, Some("favicon"));

            let spinner = gtk4::Spinner::new();
            spinner.set_spinning(false);
            icon_stack.add_named(&spinner, Some("spinner"));

            icon_stack.set_visible_child_name("favicon");
            content_box.append(&icon_stack);

            *self.favicon_image.borrow_mut() = Some(favicon_image);
            *self.spinner.borrow_mut() = Some(spinner);
            *self.icon_stack.borrow_mut() = Some(icon_stack);

            // Title label - shown when sidebar is expanded
            let title_label = gtk4::Label::new(Some("New Tab"));
            title_label.set_hexpand(false);
            title_label.set_halign(gtk4::Align::Start);
            title_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            title_label.set_max_width_chars(12);
            title_label.add_css_class("tab-title");
            content_box.append(&title_label);
            *self.title_label.borrow_mut() = Some(title_label);

            // Close button - hidden by default, shown on sidebar hover
            let close_button = gtk4::Button::from_icon_name("window-close-symbolic");
            close_button.add_css_class("flat");
            close_button.add_css_class("circular");
            close_button.add_css_class("tab-close");
            close_button.set_valign(gtk4::Align::Center);
            close_button.set_tooltip_text(Some("Close Tab"));
            content_box.append(&close_button);
            *self.close_button.borrow_mut() = Some(close_button);
            *self.content_box.borrow_mut() = Some(content_box);
        }
    }

    impl WidgetImpl for TabRow {}
    impl ListBoxRowImpl for TabRow {}
}

glib::wrapper! {
    pub struct TabRow(ObjectSubclass<imp::TabRow>)
        @extends gtk4::ListBoxRow, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Actionable;
}

impl TabRow {
    pub fn new(tab_id: u32) -> Self {
        let obj: Self = glib::Object::new();
        obj.imp().tab_id.set(tab_id);
        obj
    }

    /// Set the container this tab belongs to and update the visual indicator
    pub fn set_container(&self, container_id: &str) {
        *self.imp().container_id.borrow_mut() = container_id.to_string();

        // Apply color via CSS class (container-$id)
        if let Some(indicator) = self.imp().container_indicator.borrow().as_ref() {
            // Remove any existing container color classes
            for (id, _, _) in crate::config::DEFAULT_CONTAINERS {
                indicator.remove_css_class(&format!("container-{}", id));
            }
            indicator.add_css_class(&format!("container-{}", container_id));
        }
    }

    pub fn container_id(&self) -> String {
        self.imp().container_id.borrow().clone()
    }

    pub fn tab_id(&self) -> u32 {
        self.imp().tab_id.get()
    }

    pub fn set_title(&self, title: &str) {
        *self.imp().title.borrow_mut() = title.to_string();
        if let Some(label) = self.imp().title_label.borrow().as_ref() {
            label.set_text(if title.is_empty() { "New Tab" } else { title });
        }
    }

    pub fn title(&self) -> String {
        self.imp().title.borrow().clone()
    }

    pub fn set_favicon(&self, texture: Option<&gtk4::gdk::Texture>) {
        *self.imp().favicon.borrow_mut() = texture.cloned();
        if let Some(image) = self.imp().favicon_image.borrow().as_ref() {
            match texture {
                Some(tex) => image.set_paintable(Some(tex)),
                None => image.set_icon_name(Some("web-browser-symbolic")),
            }
        }
    }

    pub fn set_loading(&self, loading: bool) {
        self.imp().is_loading.set(loading);

        if let Some(spinner) = self.imp().spinner.borrow().as_ref() {
            spinner.set_spinning(loading);
        }

        if let Some(stack) = self.imp().icon_stack.borrow().as_ref() {
            if loading {
                stack.set_visible_child_name("spinner");
            } else {
                stack.set_visible_child_name("favicon");
            }
        }
    }

    pub fn is_loading(&self) -> bool {
        self.imp().is_loading.get()
    }

    pub fn connect_close_clicked<F: Fn(&Self) + 'static>(&self, f: F) {
        if let Some(button) = self.imp().close_button.borrow().as_ref() {
            let this = self.clone();
            button.connect_clicked(move |_| {
                f(&this);
            });
        }
    }

    pub fn set_close_button_visible(&self, visible: bool) {
        if let Some(button) = self.imp().close_button.borrow().as_ref() {
            button.set_visible(visible);
        }
    }

    pub fn set_title_visible(&self, visible: bool) {
        if let Some(label) = self.imp().title_label.borrow().as_ref() {
            label.set_visible(visible);
        }
    }

    pub fn set_active(&self, active: bool) {
        if active {
            self.add_css_class("active");
        } else {
            self.remove_css_class("active");
        }
    }
}

impl Default for TabRow {
    fn default() -> Self {
        Self::new(0)
    }
}
