//! Completion row widget.

use crate::completion::provider::{CompletionItem, CompletionType};
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::glib;
use std::cell::RefCell;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct CompletionRow {
        pub item: RefCell<Option<CompletionItem>>,
        pub icon: gtk4::Image,
        pub title_label: gtk4::Label,
        pub url_label: gtk4::Label,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CompletionRow {
        const NAME: &'static str = "QayeqCompletionRow";
        type Type = super::CompletionRow;
        type ParentType = gtk4::Box;
    }

    impl ObjectImpl for CompletionRow {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();
            obj.set_orientation(gtk4::Orientation::Horizontal);
            obj.set_spacing(8);
            obj.set_margin_start(4);
            obj.set_margin_end(4);
            obj.set_margin_top(2);
            obj.set_margin_bottom(2);

            // Favicon/type icon
            self.icon.set_icon_size(gtk4::IconSize::Normal);
            self.icon.set_pixel_size(16);
            obj.append(&self.icon);

            // Title label
            self.title_label.set_halign(gtk4::Align::Start);
            self.title_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            self.title_label.set_use_markup(true);
            self.title_label.set_max_width_chars(40);
            obj.append(&self.title_label);

            // URL label (dimmed, for history items)
            self.url_label.set_hexpand(true);
            self.url_label.set_halign(gtk4::Align::Start);
            self.url_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            self.url_label.add_css_class("dim-label");
            self.url_label.set_max_width_chars(30);
            obj.append(&self.url_label);
        }
    }

    impl WidgetImpl for CompletionRow {}
    impl BoxImpl for CompletionRow {}
}

glib::wrapper! {
    pub struct CompletionRow(ObjectSubclass<imp::CompletionRow>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl CompletionRow {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// Set the completion item to display.
    pub fn set_item(&self, item: CompletionItem, query: &str) {
        let imp = self.imp();

        // Set icon - use favicon if available, otherwise type icon
        if let Some(ref favicon) = item.favicon {
            let texture = gtk4::gdk::Texture::for_pixbuf(favicon);
            imp.icon.set_paintable(Some(&texture));
        } else {
            let icon_name = match item.completion_type {
                CompletionType::History => "globe-symbolic",
                CompletionType::SearchSuggestion => "system-search-symbolic",
            };
            imp.icon.set_icon_name(Some(icon_name));
        }

        // Set title with highlighted match
        let title = highlight_match(&item.title, query);
        imp.title_label.set_markup(&title);

        // Set URL for history items (show simplified URL like Firefox)
        if let Some(ref url) = item.url {
            let display_url = simplify_url(url);
            imp.url_label.set_text(&display_url);
            imp.url_label.set_visible(true);
        } else {
            imp.url_label.set_visible(false);
        }

        *imp.item.borrow_mut() = Some(item);
    }

    /// Get the completion item.
    pub fn item(&self) -> Option<CompletionItem> {
        self.imp().item.borrow().clone()
    }
}

impl Default for CompletionRow {
    fn default() -> Self {
        Self::new()
    }
}

/// Highlight matching portions of text using Pango markup.
fn highlight_match(text: &str, query: &str) -> String {
    if query.is_empty() {
        return glib::markup_escape_text(text).to_string();
    }

    let text_lower = text.to_lowercase();
    let query_lower = query.to_lowercase();

    if let Some(pos) = text_lower.find(&query_lower) {
        let before = glib::markup_escape_text(&text[..pos]);
        let matched = glib::markup_escape_text(&text[pos..pos + query.len()]);
        let after = glib::markup_escape_text(&text[pos + query.len()..]);
        format!("{}<b>{}</b>{}", before, matched, after)
    } else {
        glib::markup_escape_text(text).to_string()
    }
}

/// Simplify URL for display (like Firefox) - show domain + path
fn simplify_url(url: &str) -> String {
    if let Ok(parsed) = url::Url::parse(url) {
        let host = parsed.host_str().unwrap_or("");
        let path = parsed.path();
        // Remove www. prefix and trailing slash
        let host = host.strip_prefix("www.").unwrap_or(host);
        let path = path.strip_suffix('/').unwrap_or(path);
        if path.is_empty() || path == "/" {
            host.to_string()
        } else {
            format!("{}{}", host, path)
        }
    } else {
        url.to_string()
    }
}
