//! Clear browsing data dialog implementation.

use gtk4::prelude::*;
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Time range options for clearing data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeRange {
    LastHour,
    Last24Hours,
    Last7Days,
    Last4Weeks,
    AllTime,
}

impl TimeRange {
    /// Convert time range to a Unix timestamp threshold.
    /// Returns None for AllTime (clear everything).
    pub fn to_timestamp(&self) -> Option<i64> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        match self {
            TimeRange::LastHour => Some(now - 3600),
            TimeRange::Last24Hours => Some(now - 86400),
            TimeRange::Last7Days => Some(now - 604800),
            TimeRange::Last4Weeks => Some(now - 2419200),
            TimeRange::AllTime => None,
        }
    }

    fn from_index(index: u32) -> Self {
        match index {
            0 => TimeRange::LastHour,
            1 => TimeRange::Last24Hours,
            2 => TimeRange::Last7Days,
            3 => TimeRange::Last4Weeks,
            _ => TimeRange::AllTime,
        }
    }
}

/// Options selected by the user in the clear data dialog.
#[derive(Debug, Clone)]
pub struct ClearDataOptions {
    pub time_range: TimeRange,
    pub clear_history: bool,
    pub clear_cookies: bool,
    pub clear_cache: bool,
}

impl ClearDataOptions {
    /// Check if any data type is selected for clearing.
    pub fn has_selection(&self) -> bool {
        self.clear_history || self.clear_cookies || self.clear_cache
    }
}

/// Dialog for clearing browsing data.
pub struct ClearDataDialog {
    dialog: libadwaita::AlertDialog,
    time_dropdown: gtk4::DropDown,
    history_check: gtk4::CheckButton,
    cookies_check: gtk4::CheckButton,
    cache_check: gtk4::CheckButton,
}

impl ClearDataDialog {
    /// Create a new clear data dialog.
    pub fn new() -> Self {
        let dialog = libadwaita::AlertDialog::new(
            Some("Clear Browsing Data"),
            Some("Select what data to clear. This action cannot be undone."),
        );

        // Build the content
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        content.set_margin_top(12);
        content.set_margin_bottom(12);
        content.set_margin_start(12);
        content.set_margin_end(12);

        // Time range selector
        let time_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        let time_label = gtk4::Label::new(Some("Time range:"));
        time_box.append(&time_label);

        let time_model = gtk4::StringList::new(&[
            "Last hour",
            "Last 24 hours",
            "Last 7 days",
            "Last 4 weeks",
            "All time",
        ]);
        let time_dropdown = gtk4::DropDown::new(Some(time_model), gtk4::Expression::NONE);
        time_dropdown.set_selected(4); // Default to "All time"
        time_dropdown.set_hexpand(true);
        time_box.append(&time_dropdown);
        content.append(&time_box);

        // Separator
        let separator = gtk4::Separator::new(gtk4::Orientation::Horizontal);
        separator.set_margin_top(6);
        separator.set_margin_bottom(6);
        content.append(&separator);

        // Checkboxes for data types
        let checks_box = gtk4::Box::new(gtk4::Orientation::Vertical, 8);

        let history_check = gtk4::CheckButton::with_label("Browsing history");
        history_check.set_active(true);
        checks_box.append(&history_check);

        let cookies_check = gtk4::CheckButton::with_label("Cookies and site data");
        cookies_check.set_active(false);
        checks_box.append(&cookies_check);

        let cache_check = gtk4::CheckButton::with_label("Cached images and files");
        cache_check.set_active(false);
        checks_box.append(&cache_check);

        content.append(&checks_box);

        dialog.set_extra_child(Some(&content));

        // Buttons
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("clear", "Clear Data");
        dialog.set_response_appearance("clear", libadwaita::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");

        Self {
            dialog,
            time_dropdown,
            history_check,
            cookies_check,
            cache_check,
        }
    }

    /// Present the dialog and call the callback with the selected options.
    pub fn present<F>(&self, parent: &impl IsA<gtk4::Window>, callback: F)
    where
        F: Fn(ClearDataOptions) + 'static,
    {
        let time_dropdown = self.time_dropdown.clone();
        let history_check = self.history_check.clone();
        let cookies_check = self.cookies_check.clone();
        let cache_check = self.cache_check.clone();
        let callback = Rc::new(RefCell::new(Some(callback)));

        self.dialog.connect_response(None, move |_, response| {
            if response == "clear" {
                let options = ClearDataOptions {
                    time_range: TimeRange::from_index(time_dropdown.selected()),
                    clear_history: history_check.is_active(),
                    clear_cookies: cookies_check.is_active(),
                    clear_cache: cache_check.is_active(),
                };

                if options.has_selection() {
                    if let Some(cb) = callback.borrow_mut().take() {
                        cb(options);
                    }
                }
            }
        });

        self.dialog.present(Some(parent.upcast_ref()));
    }
}

impl Default for ClearDataDialog {
    fn default() -> Self {
        Self::new()
    }
}
